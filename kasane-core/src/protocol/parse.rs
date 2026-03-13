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
            let (lines, cursor_pos, default_face, padding_face, widget_columns) =
                de_params(method, params)?;
            Ok(KakouneRequest::Draw {
                lines,
                cursor_pos,
                default_face,
                padding_face,
                widget_columns,
            })
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
