use std::collections::HashMap;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::color::Face;
use super::style::{
    DEFAULT_STYLE_ID, Style, StyleId, intern_style_global, style_clone_global, with_global_style,
};

// ---------------------------------------------------------------------------
// Atom / Line / Coord
// ---------------------------------------------------------------------------

/// A styled text fragment.
///
/// `style_id` references a [`Style`] in the process-global style table
/// (see [`crate::protocol::style`]). The id is 4 bytes, `Copy`, and
/// `Eq` is identity, which keeps `Atom` cheap to clone, compare, and hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Atom {
    pub contents: CompactString,
    pub style_id: StyleId,
}

impl Atom {
    /// Construct an atom from a legacy [`Face`], interning its style.
    /// Bridge constructor for the ADR-031 migration; will be retired
    /// alongside [`Face`] in Phase A.3.
    pub fn from_face(face: Face, contents: impl Into<CompactString>) -> Self {
        Self {
            contents: contents.into(),
            style_id: intern_style_global(Style::from_face(&face)),
        }
    }

    /// Construct an atom directly from a known `StyleId`.
    #[inline]
    pub fn from_style_id(contents: impl Into<CompactString>, style_id: StyleId) -> Self {
        Self {
            contents: contents.into(),
            style_id,
        }
    }

    /// Project this atom's style back to a [`Face`]. Bridge for sites
    /// that still consume the legacy representation.
    #[inline]
    pub fn face(&self) -> Face {
        with_global_style(self.style_id, |s| s.to_face())
    }

    /// Return a copy of this atom's style.
    #[inline]
    pub fn style(&self) -> Style {
        style_clone_global(self.style_id)
    }

    /// Run `f` with a borrow of this atom's style. Avoids the `Style` clone
    /// when the caller only needs to read a few fields.
    #[inline]
    pub fn with_style<R>(&self, f: impl FnOnce(&Style) -> R) -> R {
        with_global_style(self.style_id, f)
    }

    /// Construct an atom with [`Style::default`] (no allocation —
    /// the default is pre-interned at [`DEFAULT_STYLE_ID`]).
    #[inline]
    pub fn plain(contents: impl Into<CompactString>) -> Self {
        Self {
            contents: contents.into(),
            style_id: DEFAULT_STYLE_ID,
        }
    }
}

pub type Line = Vec<Atom>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct Coord {
    pub line: i32,
    pub column: i32,
}

// ---------------------------------------------------------------------------
// CursorMode / MenuStyle / InfoStyle
// ---------------------------------------------------------------------------

/// Cursor display mode derived from `draw_status` cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CursorMode {
    Buffer,
    Prompt,
}

/// Menu display style sent by Kakoune's `menu_show` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuStyle {
    Prompt,
    Search,
    Inline,
}

impl MenuStyle {
    /// Prompt and search styles are rendered as horizontal multi-column
    /// layouts above the status bar.
    pub fn is_prompt_like(self) -> bool {
        matches!(self, Self::Prompt | Self::Search)
    }
}

/// Info popup display style sent by Kakoune's `info_show` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InfoStyle {
    Prompt,
    Modal,
    Inline,
    InlineAbove,
    MenuDoc,
}

impl InfoStyle {
    /// Framed styles (prompt, modal) get borders and padding.
    pub fn is_framed(self) -> bool {
        matches!(self, Self::Prompt | Self::Modal)
    }
}

/// Status bar context style sent by Kakoune's `draw_status` message (PR #5458).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusStyle {
    #[default]
    Status,
    Command,
    Search,
    Prompt,
}

// ---------------------------------------------------------------------------
// Kakoune → Kasane messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum KakouneRequest {
    Draw {
        lines: Vec<Line>,
        cursor_pos: Coord,
        default_face: Face,
        padding_face: Face,
        widget_columns: u16,
    },
    DrawStatus {
        prompt: Line,
        content: Line,
        content_cursor_pos: i32,
        mode_line: Line,
        default_face: Face,
        style: StatusStyle,
    },
    MenuShow {
        items: Vec<Line>,
        anchor: Coord,
        selected_item_face: Face,
        menu_face: Face,
        style: MenuStyle,
    },
    MenuSelect {
        selected: i32,
    },
    MenuHide,
    InfoShow {
        title: Line,
        content: Vec<Line>,
        anchor: Coord,
        face: Face,
        style: InfoStyle,
    },
    InfoHide,
    SetUiOptions {
        options: HashMap<String, String>,
    },
    Refresh {
        force: bool,
    },
}

// ---------------------------------------------------------------------------
// Kasane → Kakoune messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum KasaneRequest {
    Keys(Vec<String>),
    Resize {
        rows: u16,
        cols: u16,
    },
    MousePress {
        button: String,
        line: u32,
        column: u32,
    },
    MouseRelease {
        button: String,
        line: u32,
        column: u32,
    },
    MouseMove {
        line: u32,
        column: u32,
    },
    Scroll {
        amount: i32,
        line: u32,
        column: u32,
    },
    MenuSelect(i32),
}

#[derive(Serialize)]
struct JsonRpc<'a, P: Serialize> {
    jsonrpc: &'static str,
    method: &'a str,
    params: P,
}

fn to_json_rpc<P: Serialize>(method: &str, params: P) -> String {
    serde_json::to_string(&JsonRpc {
        jsonrpc: "2.0",
        method,
        params,
    })
    .expect("KasaneRequest serialization should not fail")
}

impl KasaneRequest {
    pub fn to_json(&self) -> String {
        match self {
            Self::Keys(keys) => to_json_rpc("keys", keys),
            Self::Resize { rows, cols } => to_json_rpc("resize", (rows, cols)),
            Self::MousePress {
                button,
                line,
                column,
            } => to_json_rpc("mouse_press", (button, line, column)),
            Self::MouseRelease {
                button,
                line,
                column,
            } => to_json_rpc("mouse_release", (button, line, column)),
            Self::MouseMove { line, column } => to_json_rpc("mouse_move", (line, column)),
            Self::Scroll {
                amount,
                line,
                column,
            } => to_json_rpc("scroll", (amount, line, column)),
            Self::MenuSelect(index) => to_json_rpc("menu_select", (index,)),
        }
    }
}
