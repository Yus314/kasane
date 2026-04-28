use compact_str::CompactString;
use serde::Deserialize;
use simd_json::prelude::*;
use thiserror::Error;

use super::color::Face;
use super::message::{Atom, KakouneRequest, Line, StatusStyle};

// ---------------------------------------------------------------------------
// JSON-RPC parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("invalid params for {method}: {reason}")]
    InvalidParams { method: String, reason: String },
    #[error("{0}")]
    OldProtocol(String),
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------
//
// `Atom` no longer derives `Deserialize` because its `style_id` is opaque to
// the wire format — Kakoune only knows about `Face`. Parsing lands in
// `WireAtom` (the legacy shape) and the parser converts it to `Atom` by
// interning the corresponding `Style` in the process-global table.

#[derive(Debug, Clone, Deserialize)]
struct WireAtom {
    face: Face,
    contents: CompactString,
}

type WireLine = Vec<WireAtom>;

fn intern_line(wire: WireLine) -> Line {
    wire.into_iter()
        .map(|w| Atom::from_face(w.face, w.contents))
        .collect()
}

fn intern_lines(wire: Vec<WireLine>) -> Vec<Line> {
    wire.into_iter().map(intern_line).collect()
}

pub fn parse_request(input: &mut [u8]) -> Result<KakouneRequest, ProtocolError> {
    let value: simd_json::OwnedValue =
        simd_json::to_owned_value(input).map_err(|e| ProtocolError::Json(e.to_string()))?;

    let method = value
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProtocolError::Json("missing method field".into()))?
        .to_string();

    let params = value
        .get("params")
        .ok_or_else(|| ProtocolError::Json("missing params field".into()))?;

    parse_method(&method, params)
}

fn parse_method(
    method: &str,
    params: &simd_json::OwnedValue,
) -> Result<KakouneRequest, ProtocolError> {
    if params.as_array().is_none() {
        return Err(ProtocolError::InvalidParams {
            method: method.into(),
            reason: "params must be an array".into(),
        });
    }

    match method {
        "draw" => {
            // widget_columns was added in PR #5455 (merged 2026-03-11) and is
            // not yet in a Kakoune release.  Accept draw with 4 params (pre-
            // #5455) by defaulting widget_columns to 0.
            let arr = params.as_array().expect("params validated as array");
            if arr.len() >= 5 {
                let (wire_lines, cursor_pos, default_face, padding_face, widget_columns): (
                    Vec<WireLine>,
                    _,
                    _,
                    _,
                    _,
                ) = de_params(method, params)?;
                Ok(KakouneRequest::Draw {
                    lines: intern_lines(wire_lines),
                    cursor_pos,
                    default_face,
                    padding_face,
                    widget_columns,
                })
            } else {
                let (wire_lines, cursor_pos, default_face, padding_face): (Vec<WireLine>, _, _, _) =
                    de_params(method, params)?;
                Ok(KakouneRequest::Draw {
                    lines: intern_lines(wire_lines),
                    cursor_pos,
                    default_face,
                    padding_face,
                    widget_columns: 0,
                })
            }
        }
        "draw_status" => {
            // PR #5458 (merged 2026-03-21) adds a 6th parameter `style` to
            // draw_status.  Accept 5 params (pre-#5458) by defaulting style
            // to StatusStyle::Status.
            let arr = params.as_array().expect("params validated as array");
            if arr.len() >= 6 {
                let (
                    wire_prompt,
                    wire_content,
                    content_cursor_pos,
                    wire_mode_line,
                    default_face,
                    style,
                ): (WireLine, WireLine, _, WireLine, _, _) = de_params(method, params)?;
                Ok(KakouneRequest::DrawStatus {
                    prompt: intern_line(wire_prompt),
                    content: intern_line(wire_content),
                    content_cursor_pos,
                    mode_line: intern_line(wire_mode_line),
                    default_face,
                    style,
                })
            } else {
                let (wire_prompt, wire_content, content_cursor_pos, wire_mode_line, default_face): (
                    WireLine,
                    WireLine,
                    _,
                    WireLine,
                    _,
                ) = de_params(method, params)?;
                Ok(KakouneRequest::DrawStatus {
                    prompt: intern_line(wire_prompt),
                    content: intern_line(wire_content),
                    content_cursor_pos,
                    mode_line: intern_line(wire_mode_line),
                    default_face,
                    style: StatusStyle::default(),
                })
            }
        }
        "menu_show" => {
            let (wire_items, anchor, selected_item_face, menu_face, style): (
                Vec<WireLine>,
                _,
                _,
                _,
                _,
            ) = de_params(method, params)?;
            Ok(KakouneRequest::MenuShow {
                items: intern_lines(wire_items),
                anchor,
                selected_item_face,
                menu_face,
                style,
            })
        }
        "menu_select" => {
            let (selected,) = de_params(method, params)?;
            Ok(KakouneRequest::MenuSelect { selected })
        }
        "menu_hide" => Ok(KakouneRequest::MenuHide),
        "info_show" => {
            let (wire_title, wire_content, anchor, face, style): (
                WireLine,
                Vec<WireLine>,
                _,
                _,
                _,
            ) = de_params(method, params)?;
            Ok(KakouneRequest::InfoShow {
                title: intern_line(wire_title),
                content: intern_lines(wire_content),
                anchor,
                face,
                style,
            })
        }
        "info_hide" => Ok(KakouneRequest::InfoHide),
        "set_ui_options" => {
            let (options,) = de_params(method, params)?;
            Ok(KakouneRequest::SetUiOptions { options })
        }
        "refresh" => {
            let (force,) = de_params(method, params)?;
            Ok(KakouneRequest::Refresh { force })
        }
        // Old Kakoune protocol used "set_cursor" instead of including cursor_pos
        // in "draw" params. Detect this and give a helpful error.
        "set_cursor" => Err(ProtocolError::OldProtocol(
            "Kasane requires Kakoune 2024.12.09 or later (commit 3dd6f30d).\n\
             Your Kakoune appears to use an older protocol (set_cursor method detected).\n\
             Please update Kakoune: https://github.com/mawww/kakoune"
                .into(),
        )),
        _ => Err(ProtocolError::UnknownMethod(method.into())),
    }
}

fn de_params<T: serde::de::DeserializeOwned>(
    method: &str,
    params: &simd_json::OwnedValue,
) -> Result<T, ProtocolError> {
    simd_json::serde::from_refowned_value(params).map_err(|e| ProtocolError::InvalidParams {
        method: method.into(),
        reason: e.to_string(),
    })
}
