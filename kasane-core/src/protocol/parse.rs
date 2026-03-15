use simd_json::prelude::*;
use thiserror::Error;

use super::message::KakouneRequest;

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
            let arr = params.as_array().unwrap(); // validated above
            if arr.len() >= 5 {
                let (lines, cursor_pos, default_face, padding_face, widget_columns) =
                    de_params(method, params)?;
                Ok(KakouneRequest::Draw {
                    lines,
                    cursor_pos,
                    default_face,
                    padding_face,
                    widget_columns,
                })
            } else {
                let (lines, cursor_pos, default_face, padding_face) = de_params(method, params)?;
                Ok(KakouneRequest::Draw {
                    lines,
                    cursor_pos,
                    default_face,
                    padding_face,
                    widget_columns: 0,
                })
            }
        }
        "draw_status" => {
            let (prompt, content, content_cursor_pos, mode_line, default_face) =
                de_params(method, params)?;
            Ok(KakouneRequest::DrawStatus {
                prompt,
                content,
                content_cursor_pos,
                mode_line,
                default_face,
            })
        }
        "menu_show" => {
            let (items, anchor, selected_item_face, menu_face, style) = de_params(method, params)?;
            Ok(KakouneRequest::MenuShow {
                items,
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
            let (title, content, anchor, face, style) = de_params(method, params)?;
            Ok(KakouneRequest::InfoShow {
                title,
                content,
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
