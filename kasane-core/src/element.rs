//! The core `Element` type for declarative UI.

use std::ops::Range;
use std::sync::Arc;

use compact_str::CompactString;

use crate::display::DisplayMapRef;
use crate::layout::Rect;
use crate::protocol::{Atom, Coord, Face, Line};

/// Source of image data for `Element::Image`.
#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Path to an image file on disk.
    FilePath(String),
    /// Pre-decoded RGBA pixel data.
    Rgba {
        data: Arc<[u8]>,
        width: u32,
        height: u32,
    },
    /// Inline SVG source data (XML bytes).
    SvgData { data: Arc<[u8]> },
}

impl PartialEq for ImageSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ImageSource::FilePath(a), ImageSource::FilePath(b)) => a == b,
            (
                ImageSource::Rgba {
                    data: a,
                    width: aw,
                    height: ah,
                },
                ImageSource::Rgba {
                    data: b,
                    width: bw,
                    height: bh,
                },
            ) => aw == bw && ah == bh && Arc::ptr_eq(a, b),
            (ImageSource::SvgData { data: a }, ImageSource::SvgData { data: b }) => {
                Arc::ptr_eq(a, b)
            }
            _ => false,
        }
    }
}

/// How an image should be fitted within its allocated area.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImageFit {
    /// Scale to fit within the area, preserving aspect ratio (letterboxing).
    #[default]
    Contain,
    /// Scale to cover the entire area, preserving aspect ratio (cropping).
    Cover,
    /// Stretch to fill the area exactly (may distort).
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Row,
    Column,
}

impl Direction {
    /// Extract the main-axis component of a size.
    pub fn main(self, size: crate::layout::flex::Size) -> u16 {
        match self {
            Direction::Row => size.width,
            Direction::Column => size.height,
        }
    }

    /// Extract the cross-axis component of a size.
    pub fn cross(self, size: crate::layout::flex::Size) -> u16 {
        match self {
            Direction::Row => size.height,
            Direction::Column => size.width,
        }
    }

    /// Decompose a size into (main, cross) components.
    pub fn decompose(self, size: crate::layout::flex::Size) -> (u16, u16) {
        (self.main(size), self.cross(size))
    }

    /// Compose main and cross values back into a `Size`.
    pub fn compose(self, main: u16, cross: u16) -> crate::layout::flex::Size {
        match self {
            Direction::Row => crate::layout::flex::Size {
                width: main,
                height: cross,
            },
            Direction::Column => crate::layout::flex::Size {
                width: cross,
                height: main,
            },
        }
    }

    /// Build a `Rect` from directional components.
    pub fn rect(
        self,
        origin: (u16, u16),
        main_offset: u16,
        cross_offset: u16,
        main_size: u16,
        cross_size: u16,
    ) -> crate::layout::Rect {
        let (ox, oy) = origin;
        match self {
            Direction::Row => crate::layout::Rect {
                x: ox + main_offset,
                y: oy + cross_offset,
                w: main_size,
                h: cross_size,
            },
            Direction::Column => crate::layout::Rect {
                x: ox + cross_offset,
                y: oy + main_offset,
                w: cross_size,
                h: main_size,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Column width specification for Grid layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridWidth {
    /// Fixed width in cells.
    Fixed(u16),
    /// Proportional share of remaining space.
    Flex(f32),
    /// Width determined by measuring the widest cell in the column.
    Auto,
}

/// Column definition for Grid layout.
#[derive(Debug, Clone, PartialEq)]
pub struct GridColumn {
    pub width: GridWidth,
}

impl GridColumn {
    pub fn fixed(width: u16) -> Self {
        GridColumn {
            width: GridWidth::Fixed(width),
        }
    }

    pub fn flex(factor: f32) -> Self {
        GridColumn {
            width: GridWidth::Flex(factor),
        }
    }

    pub fn auto() -> Self {
        GridColumn {
            width: GridWidth::Auto,
        }
    }
}

/// Semantic style tokens for theme-driven rendering.
///
/// An open type: plugins can define custom tokens by constructing
/// `StyleToken` with arbitrary names (e.g., `StyleToken::new("myplugin.highlight")`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StyleToken(pub CompactString);

impl StyleToken {
    pub const BUFFER_TEXT: Self = Self(CompactString::const_new("buffer.text"));
    pub const BUFFER_PADDING: Self = Self(CompactString::const_new("buffer.padding"));
    pub const STATUS_LINE: Self = Self(CompactString::const_new("status.line"));
    pub const STATUS_MODE: Self = Self(CompactString::const_new("status.mode"));
    pub const MENU_ITEM_NORMAL: Self = Self(CompactString::const_new("menu.item.normal"));
    pub const MENU_ITEM_SELECTED: Self = Self(CompactString::const_new("menu.item.selected"));
    pub const MENU_SCROLLBAR: Self = Self(CompactString::const_new("menu.scrollbar"));
    pub const MENU_SCROLLBAR_THUMB: Self = Self(CompactString::const_new("menu.scrollbar.thumb"));
    pub const INFO_TEXT: Self = Self(CompactString::const_new("info.text"));
    pub const INFO_BORDER: Self = Self(CompactString::const_new("info.border"));
    pub const BORDER: Self = Self(CompactString::const_new("border"));
    pub const SPLIT_DIVIDER: Self = Self(CompactString::const_new("split.divider"));
    pub const SPLIT_DIVIDER_FOCUSED: Self = Self(CompactString::const_new("split.divider.focused"));
    pub const SHADOW: Self = Self(CompactString::const_new("shadow"));
    pub const GUTTER_LINE_NUMBER: Self = Self(CompactString::const_new("gutter.line_number"));
    pub const TEXT_PANEL_CURSOR: Self = Self(CompactString::const_new("text_panel.cursor"));

    /// Create a custom style token with an arbitrary name.
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    /// Get the token name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Style attached to an Element variant — either a direct face, or a semantic
/// [`StyleToken`] that the renderer resolves through the active
/// [`Theme`](crate::render::Theme).
///
/// Renamed from `Style` to `ElementStyle` in ADR-031 Phase B3 to remove the
/// name collision with [`crate::protocol::Style`]. The `Direct` variant still
/// carries [`Face`] for the moment; a follow-up commit converts it to
/// `Arc<UnresolvedStyle>` once the Element-tree-touching call sites are all
/// migrated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementStyle {
    Direct(Face),
    Token(StyleToken),
}

impl From<Face> for ElementStyle {
    fn from(face: Face) -> Self {
        ElementStyle::Direct(face)
    }
}

impl ElementStyle {
    /// Get the face, either directly or as a fallback (Token variants return None).
    pub fn face(&self) -> Option<&Face> {
        match self {
            ElementStyle::Direct(face) => Some(face),
            ElementStyle::Token(_) => None,
        }
    }
}

/// Plugin ownership tag for interactive ID namespace isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginTag(pub u16);

impl PluginTag {
    /// Framework-owned interactive elements (info popups, etc.).
    pub const FRAMEWORK: PluginTag = PluginTag(0);
    /// Sentinel for native plugins before tag assignment.
    pub const UNASSIGNED: PluginTag = PluginTag(u16::MAX);
}

/// Unique identifier for interactive regions (mouse hit testing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractiveId {
    pub local: u32,
    pub owner: PluginTag,
}

impl InteractiveId {
    /// Base ID for info popup interactive regions.
    pub const INFO_BASE: u32 = 1000;

    pub fn new(local: u32, owner: PluginTag) -> Self {
        Self { local, owner }
    }

    /// Framework-owned interactive element (info popups, etc.).
    pub fn framework(local: u32) -> Self {
        Self {
            local,
            owner: PluginTag::FRAMEWORK,
        }
    }

    /// For native plugin authors — tag will be injected by PluginBridge.
    pub fn unassigned(local: u32) -> Self {
        Self {
            local,
            owner: PluginTag::UNASSIGNED,
        }
    }
}

/// Frame-local identifier assigned to a resolved slot instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedSlotInstanceId(pub u64);

/// Line style for borders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderLineStyle {
    Single,
    Rounded,
    Double,
    Heavy,
    Ascii,
    /// Custom border characters: [TL, T, TR, R, BR, B, BL, L, title-left, title-right, shadow].
    Custom(Box<[char; 11]>),
}

/// Backward-compatible alias used in the Element tree.
pub type BorderStyle = BorderLineStyle;

/// Full border configuration: line style + optional face override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorderConfig {
    pub line_style: BorderLineStyle,
    pub style: Option<ElementStyle>,
}

impl BorderConfig {
    pub fn new(line_style: BorderLineStyle) -> Self {
        BorderConfig {
            line_style,
            style: None,
        }
    }
}

impl From<BorderLineStyle> for BorderConfig {
    fn from(style: BorderLineStyle) -> Self {
        BorderConfig::new(style)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edges {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Edges {
    pub const ZERO: Edges = Edges {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub fn uniform(v: u16) -> Self {
        Edges {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub fn horizontal(&self) -> u16 {
        self.left + self.right
    }

    pub fn vertical(&self) -> u16 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OverlayAnchor {
    Fill,
    Absolute {
        x: u16,
        y: u16,
        w: u16,
        h: u16,
    },
    AnchorPoint {
        coord: Coord,
        prefer_above: bool,
        avoid: Vec<Rect>,
    },
}

impl From<crate::layout::FloatingWindow> for OverlayAnchor {
    fn from(win: crate::layout::FloatingWindow) -> Self {
        OverlayAnchor::Absolute {
            x: win.x,
            y: win.y,
            w: win.width,
            h: win.height,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub element: Element,
    pub anchor: OverlayAnchor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlexChild {
    pub element: Element,
    /// 0.0 = fixed size, >0.0 = proportional flex allocation.
    pub flex: f32,
    pub min_size: Option<u16>,
    pub max_size: Option<u16>,
}

impl FlexChild {
    pub fn fixed(element: Element) -> Self {
        FlexChild {
            element,
            flex: 0.0,
            min_size: None,
            max_size: None,
        }
    }

    pub fn flexible(element: Element, flex: f32) -> Self {
        FlexChild {
            element,
            flex,
            min_size: None,
            max_size: None,
        }
    }
}

/// Embedded state for multi-pane BufferRef rendering.
/// When present, `paint_buffer_ref` uses this instead of the walk context state.
#[derive(Debug, Clone, PartialEq)]
pub struct BufferRefState {
    pub lines: Vec<Vec<crate::protocol::Atom>>,
    pub lines_dirty: Vec<bool>,
    pub default_face: Face,
    pub padding_face: Face,
    pub padding_char: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Text(CompactString, ElementStyle),
    StyledLine(Vec<Atom>),
    SlotPlaceholder {
        slot_name: CompactString,
        direction: Direction,
        gap: u16,
    },
    Flex {
        direction: Direction,
        children: Vec<FlexChild>,
        gap: u16,
        align: Align,
        cross_align: Align,
    },
    ResolvedSlot {
        surface_key: CompactString,
        slot_name: CompactString,
        instance_id: ResolvedSlotInstanceId,
        direction: Direction,
        children: Vec<FlexChild>,
        gap: u16,
    },
    Stack {
        base: Box<Element>,
        overlays: Vec<Overlay>,
    },
    Scrollable {
        child: Box<Element>,
        offset: u16,
        direction: Direction,
    },
    Container {
        child: Box<Element>,
        border: Option<BorderConfig>,
        shadow: bool,
        padding: Edges,
        style: ElementStyle,
        title: Option<Line>,
    },
    /// Transparent wrapper for mouse hit testing. Renders child unchanged.
    Interactive {
        child: Box<Element>,
        id: InteractiveId,
    },
    /// 2D grid layout: children placed in row-major order across `columns`.
    Grid {
        columns: Vec<GridColumn>,
        children: Vec<Element>,
        col_gap: u16,
        row_gap: u16,
        align: Align,
        cross_align: Align,
    },
    Empty,
    /// Raster image element for GPU rendering (TUI falls back to text placeholder).
    Image {
        source: ImageSource,
        /// Size in cells (width, height).
        size: (u16, u16),
        fit: ImageFit,
        opacity: f32,
    },
    /// Plugin-owned rich text panel with scrolling and optional line numbers.
    TextPanel {
        /// Lines of styled text to display.
        lines: Vec<Line>,
        /// Vertical scroll offset (number of lines scrolled from top).
        scroll_offset: usize,
        /// Optional cursor position (line, column) for highlighting.
        cursor: Option<(usize, usize)>,
        /// Whether to display line numbers in a left gutter.
        line_numbers: bool,
        /// Whether to wrap long lines (false = truncate).
        wrap: bool,
    },
    /// GPU canvas element: plugin-submitted draw operations.
    /// TUI backend renders a placeholder; GPU backend converts ops to primitives.
    Canvas {
        /// Size in cells (width, height).
        size: (u16, u16),
        /// Drawing operations submitted by the plugin.
        content: crate::plugin::canvas::CanvasContent,
    },

    /// Zero-copy buffer reference: renders lines[line_range] from AppState.
    BufferRef {
        line_range: Range<usize>,
        /// Per-line background overrides from plugins (indexed by line within range).
        line_backgrounds: Option<Arc<Vec<Option<Face>>>>,
        /// Display transformation map (None = identity, no transformations).
        display_map: Option<DisplayMapRef>,
        /// Pane-specific state for multi-pane rendering. When `Some`, `paint_buffer_ref`
        /// reads lines/faces from here instead of the walk context's primary AppState.
        state: Option<Box<BufferRefState>>,
        /// Per-line inline decorations (byte-range Style/Hide) from plugins.
        inline_decorations: Option<Arc<Vec<Option<crate::render::InlineDecoration>>>>,
        /// Per-line EOL virtual text atoms from plugins.
        virtual_text: Option<Arc<Vec<Option<Vec<Atom>>>>>,
    },
}

impl Element {
    pub fn text(s: impl Into<CompactString>, face: Face) -> Self {
        Element::Text(s.into(), ElementStyle::from(face))
    }

    pub fn styled_line(line: Line) -> Self {
        Element::StyledLine(line)
    }

    pub fn row(children: Vec<FlexChild>) -> Self {
        Element::Flex {
            direction: Direction::Row,
            children,
            gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        }
    }

    pub fn column(children: Vec<FlexChild>) -> Self {
        Element::Flex {
            direction: Direction::Column,
            children,
            gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        }
    }

    pub fn slot_placeholder(slot_name: impl Into<CompactString>, direction: Direction) -> Self {
        Element::SlotPlaceholder {
            slot_name: slot_name.into(),
            direction,
            gap: 0,
        }
    }

    pub fn buffer_ref(line_range: Range<usize>) -> Self {
        Element::BufferRef {
            line_range,
            line_backgrounds: None,
            display_map: None,
            state: None,
            inline_decorations: None,
            virtual_text: None,
        }
    }

    pub fn stack(base: Element, overlays: Vec<Overlay>) -> Self {
        Element::Stack {
            base: Box::new(base),
            overlays,
        }
    }

    pub fn grid(columns: Vec<GridColumn>, children: Vec<Element>) -> Self {
        Element::Grid {
            columns,
            children,
            col_gap: 0,
            row_gap: 0,
            align: Align::Start,
            cross_align: Align::Start,
        }
    }

    pub fn image(source: ImageSource, width: u16, height: u16) -> Self {
        Element::Image {
            source,
            size: (width, height),
            fit: ImageFit::default(),
            opacity: 1.0,
        }
    }

    pub fn text_panel(lines: Vec<Line>) -> Self {
        Element::TextPanel {
            lines,
            scroll_offset: 0,
            cursor: None,
            line_numbers: false,
            wrap: false,
        }
    }

    pub fn container(child: Element, style: ElementStyle) -> Self {
        Element::Container {
            child: Box::new(child),
            border: None,
            shadow: false,
            padding: Edges::ZERO,
            style,
            title: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Face;

    #[test]
    fn test_element_text() {
        let el = Element::text("hello", Face::default());
        match el {
            Element::Text(s, _) => assert_eq!(s, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_element_column() {
        let el = Element::column(vec![
            FlexChild::fixed(Element::text("a", Face::default())),
            FlexChild::flexible(Element::text("b", Face::default()), 1.0),
        ]);
        match el {
            Element::Flex {
                direction,
                children,
                ..
            } => {
                assert_eq!(direction, Direction::Column);
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].flex, 0.0);
                assert_eq!(children[1].flex, 1.0);
            }
            _ => panic!("expected Flex"),
        }
    }

    #[test]
    fn test_element_row() {
        let el = Element::row(vec![FlexChild::fixed(Element::Empty)]);
        match el {
            Element::Flex { direction, .. } => assert_eq!(direction, Direction::Row),
            _ => panic!("expected Flex"),
        }
    }

    #[test]
    fn test_element_buffer_ref() {
        let el = Element::buffer_ref(0..10);
        match el {
            Element::BufferRef { line_range, .. } => assert_eq!(line_range, 0..10),
            _ => panic!("expected BufferRef"),
        }
    }

    #[test]
    fn test_element_slot_placeholder() {
        let el = Element::slot_placeholder("kasane.buffer.left", Direction::Row);
        match el {
            Element::SlotPlaceholder {
                slot_name,
                direction,
                gap,
            } => {
                assert_eq!(slot_name.as_str(), "kasane.buffer.left");
                assert_eq!(direction, Direction::Row);
                assert_eq!(gap, 0);
            }
            _ => panic!("expected SlotPlaceholder"),
        }
    }

    #[test]
    fn test_element_stack() {
        let el = Element::stack(Element::Empty, vec![]);
        match el {
            Element::Stack { overlays, .. } => assert!(overlays.is_empty()),
            _ => panic!("expected Stack"),
        }
    }

    #[test]
    fn test_element_container() {
        let style = ElementStyle::from(Face::default());
        let el = Element::container(Element::Empty, style);
        match el {
            Element::Container {
                border,
                shadow,
                padding,
                ..
            } => {
                assert!(border.is_none());
                assert!(!shadow);
                assert_eq!(padding.horizontal(), 0);
                assert_eq!(padding.vertical(), 0);
            }
            _ => panic!("expected Container"),
        }
    }

    #[test]
    fn test_flex_child_fixed() {
        let child = FlexChild::fixed(Element::Empty);
        assert_eq!(child.flex, 0.0);
        assert!(child.min_size.is_none());
        assert!(child.max_size.is_none());
    }

    #[test]
    fn test_flex_child_flexible() {
        let child = FlexChild::flexible(Element::Empty, 2.0);
        assert_eq!(child.flex, 2.0);
    }

    #[test]
    fn test_edges() {
        let e = Edges::uniform(1);
        assert_eq!(e.horizontal(), 2);
        assert_eq!(e.vertical(), 2);

        assert_eq!(Edges::ZERO.horizontal(), 0);
        assert_eq!(Edges::ZERO.vertical(), 0);
    }

    #[test]
    fn test_style_from_face() {
        let face = Face::default();
        let style = ElementStyle::from(face);
        assert_eq!(style, ElementStyle::Direct(face));
        assert_eq!(style.face(), Some(&face));
    }

    #[test]
    fn test_style_token() {
        let style = ElementStyle::Token(StyleToken::MENU_ITEM_NORMAL);
        assert_eq!(style.face(), None);
    }

    #[test]
    fn test_grid_column_constructors() {
        let fixed = GridColumn::fixed(10);
        assert_eq!(fixed.width, GridWidth::Fixed(10));

        let flex = GridColumn::flex(2.0);
        assert_eq!(flex.width, GridWidth::Flex(2.0));

        let auto = GridColumn::auto();
        assert_eq!(auto.width, GridWidth::Auto);
    }

    #[test]
    fn test_element_grid() {
        let el = Element::grid(
            vec![GridColumn::fixed(5), GridColumn::flex(1.0)],
            vec![
                Element::text("a", Face::default()),
                Element::text("b", Face::default()),
            ],
        );
        match el {
            Element::Grid {
                columns,
                children,
                col_gap,
                row_gap,
                align,
                cross_align,
            } => {
                assert_eq!(columns.len(), 2);
                assert_eq!(children.len(), 2);
                assert_eq!(col_gap, 0);
                assert_eq!(row_gap, 0);
                assert_eq!(align, Align::Start);
                assert_eq!(cross_align, Align::Start);
            }
            _ => panic!("expected Grid"),
        }
    }

    #[test]
    fn test_element_image_file_path() {
        let el = Element::image(ImageSource::FilePath("test.png".into()), 10, 5);
        match el {
            Element::Image {
                source,
                size,
                fit,
                opacity,
            } => {
                assert_eq!(source, ImageSource::FilePath("test.png".into()));
                assert_eq!(size, (10, 5));
                assert_eq!(fit, ImageFit::Contain);
                assert_eq!(opacity, 1.0);
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn test_element_image_rgba() {
        let data: Arc<[u8]> = vec![255u8; 4 * 2 * 2].into();
        let el = Element::Image {
            source: ImageSource::Rgba {
                data: data.clone(),
                width: 2,
                height: 2,
            },
            size: (4, 3),
            fit: ImageFit::Cover,
            opacity: 0.5,
        };
        match el {
            Element::Image {
                source: ImageSource::Rgba { width, height, .. },
                size,
                fit,
                opacity,
            } => {
                assert_eq!(width, 2);
                assert_eq!(height, 2);
                assert_eq!(size, (4, 3));
                assert_eq!(fit, ImageFit::Cover);
                assert_eq!(opacity, 0.5);
            }
            _ => panic!("expected Image with Rgba source"),
        }
    }
}
