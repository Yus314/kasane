use std::collections::HashMap;
use std::sync::Arc;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::color::Face;
use super::style::{Style, UnresolvedStyle};

// `Face` is still used by `Atom::from_face`/`Atom::face` bridges and by
// internal call sites that have not yet migrated. Phase B3 progressively
// removes these bridges; the `KakouneRequest` enum has already migrated
// to `Arc<UnresolvedStyle>` for its style-typed fields below.

// ---------------------------------------------------------------------------
// Atom / Line / Coord
// ---------------------------------------------------------------------------

/// A styled text fragment.
///
/// `style` is an `Arc<UnresolvedStyle>` so identical styles in the same
/// frame can share the allocation. Parse-time interning lives in
/// [`crate::protocol::parse`]; once an `Atom` exists, reading its style
/// is a pointer dereference (no locks, no per-cell hashing).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Atom {
    pub contents: CompactString,
    pub style: Arc<UnresolvedStyle>,
}

impl Atom {
    /// **Wire-format-aware** atom constructor. Allocates a fresh `Arc`
    /// wrapping an [`UnresolvedStyle`] that **preserves the Kakoune
    /// `final_*` resolution flags** carried by the input `Face`.
    ///
    /// Use this only for code that mirrors the wire-format shape: the
    /// protocol parser itself, fixtures that simulate Kakoune's `draw_*`
    /// JSON output, and the `detect_cursors` test harness (which keys on
    /// `FINAL_FG | REVERSE` to identify the cursor atom). New host /
    /// plugin / rendering code that holds a [`Style`] should use
    /// [`Atom::with_style`] instead — it bypasses the wire-format
    /// representation entirely and skips the `Style → Face → Style`
    /// round-trip.
    ///
    /// Sites that build many atoms from the same `Face` should reach for
    /// [`crate::protocol::parse`]'s frame-local intern path so the `Arc`
    /// allocation is shared.
    pub fn from_face(face: Face, contents: impl Into<CompactString>) -> Self {
        Self {
            contents: contents.into(),
            style: Arc::new(UnresolvedStyle::from_face(&face)),
        }
    }

    /// **Post-resolve** atom constructor. Allocates a fresh `Arc` wrapping
    /// the input [`Style`] in an [`UnresolvedStyle`] envelope with all
    /// `final_*` flags **set to `false`** (i.e. fully resolved already; no
    /// further deferral against a base style is expected).
    ///
    /// This is the canonical constructor for new host, plugin, and
    /// rendering code that already holds a `Style`. It does **not**
    /// preserve Kakoune `final_*` resolution semantics — for that, see
    /// [`Atom::from_face`].
    #[inline]
    pub fn with_style(contents: impl Into<CompactString>, style: Style) -> Self {
        Self {
            contents: contents.into(),
            style: Arc::new(UnresolvedStyle {
                style,
                final_fg: false,
                final_bg: false,
                final_style: false,
            }),
        }
    }

    /// Construct an atom from an already-interned style `Arc`.
    #[inline]
    pub fn from_style(contents: impl Into<CompactString>, style: Arc<UnresolvedStyle>) -> Self {
        Self {
            contents: contents.into(),
            style,
        }
    }

    /// Borrow this atom's parse-side, unresolved style directly.
    ///
    /// Renamed from `style()` in the ADR-031 split (post-Step-1) and now
    /// returns a borrow rather than an owned clone — there is no longer
    /// a global table, so the borrow is simply `&*self.style`.
    #[inline]
    pub fn unresolved_style(&self) -> &UnresolvedStyle {
        &self.style
    }

    /// Project this atom's style to the post-resolve [`Style`] form,
    /// resolved against [`Style::default`]. Equivalent to
    /// `resolve_style(&atom.style, &Style::default())`.
    #[inline]
    pub fn style_resolved_default(&self) -> Style {
        super::style::resolve_style(&self.style, &Style::default())
    }

    /// Construct an atom with [`UnresolvedStyle::default`]. Cheap: the
    /// default style is allocated once per process (see
    /// [`default_unresolved_style`]) and shared.
    #[inline]
    pub fn plain(contents: impl Into<CompactString>) -> Self {
        Self {
            contents: contents.into(),
            style: super::style::default_unresolved_style(),
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
        /// Default style for buffer rendering (formerly `default_face: Face`).
        /// `Arc<UnresolvedStyle>` lets the parser share the allocation across
        /// frames when Kakoune sends the same style repeatedly (interner-backed).
        default_style: Arc<UnresolvedStyle>,
        /// Padding style (formerly `padding_face: Face`).
        padding_style: Arc<UnresolvedStyle>,
        widget_columns: u16,
    },
    DrawStatus {
        prompt: Line,
        content: Line,
        content_cursor_pos: i32,
        mode_line: Line,
        /// Status default style (formerly `default_face: Face`).
        default_style: Arc<UnresolvedStyle>,
        style: StatusStyle,
    },
    MenuShow {
        items: Vec<Line>,
        anchor: Coord,
        /// Selected menu item style (formerly `selected_item_face: Face`).
        selected_item_style: Arc<UnresolvedStyle>,
        /// Menu base style (formerly `menu_face: Face`).
        menu_style: Arc<UnresolvedStyle>,
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
        /// Info popup style (formerly `face: Face`).
        info_style: Arc<UnresolvedStyle>,
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
