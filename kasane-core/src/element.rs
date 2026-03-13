use std::ops::Range;

use compact_str::CompactString;

use crate::layout::Rect;
use crate::protocol::{Atom, Coord, Face, Line};

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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleToken {
    BufferText,
    BufferPadding,
    StatusLine,
    StatusMode,
    MenuItemNormal,
    MenuItemSelected,
    MenuScrollbar,
    MenuScrollbarThumb,
    InfoText,
    InfoBorder,
    Border,
    Shadow,
    Custom(CompactString),
}

/// Style can be either a direct Face or a semantic StyleToken resolved via Theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Style {
    Direct(Face),
    Token(StyleToken),
}

impl From<Face> for Style {
    fn from(face: Face) -> Self {
        Style::Direct(face)
    }
}

impl Style {
    /// Get the face, either directly or as a fallback (Token variants return None).
    pub fn face(&self) -> Option<&Face> {
        match self {
            Style::Direct(face) => Some(face),
            Style::Token(_) => None,
        }
    }
}

/// Unique identifier for interactive regions (mouse hit testing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InteractiveId(pub u32);

impl InteractiveId {
    /// Base ID for info popup interactive regions.
    pub const INFO_BASE: u32 = 1000;
}

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
    pub face: Option<Style>,
}

impl BorderConfig {
    pub fn new(line_style: BorderLineStyle) -> Self {
        BorderConfig {
            line_style,
            face: None,
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

#[derive(Debug, Clone)]
pub enum OverlayAnchor {
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

#[derive(Debug, Clone)]
pub struct Overlay {
    pub element: Element,
    pub anchor: OverlayAnchor,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum Element {
    Text(String, Style),
    StyledLine(Vec<Atom>),
    Flex {
        direction: Direction,
        children: Vec<FlexChild>,
        gap: u16,
        align: Align,
        cross_align: Align,
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
        style: Style,
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
    /// Zero-copy buffer reference: renders lines[line_range] from AppState.
    BufferRef {
        line_range: Range<usize>,
        /// Per-line background overrides from plugins (indexed by line within range).
        line_backgrounds: Option<Vec<Option<Face>>>,
    },
}

impl Element {
    pub fn text(s: impl Into<String>, face: Face) -> Self {
        Element::Text(s.into(), Style::from(face))
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

    pub fn buffer_ref(line_range: Range<usize>) -> Self {
        Element::BufferRef {
            line_range,
            line_backgrounds: None,
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

    pub fn container(child: Element, style: Style) -> Self {
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
    fn test_element_stack() {
        let el = Element::stack(Element::Empty, vec![]);
        match el {
            Element::Stack { overlays, .. } => assert!(overlays.is_empty()),
            _ => panic!("expected Stack"),
        }
    }

    #[test]
    fn test_element_container() {
        let style = Style::from(Face::default());
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
        let style = Style::from(face);
        assert_eq!(style, Style::Direct(face));
        assert_eq!(style.face(), Some(&face));
    }

    #[test]
    fn test_style_token() {
        let style = Style::Token(StyleToken::MenuItemNormal);
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
}
