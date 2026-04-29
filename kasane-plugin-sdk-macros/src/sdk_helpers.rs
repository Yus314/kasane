use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

/// SDK dirty::ALL value (excludes PLUGIN_STATE bit 7).
/// Must match `kasane_plugin_sdk::dirty::ALL`.
const SDK_DIRTY_ALL: u16 = 0x37F;

/// Implementation of the `kasane_generate` proc macro.
///
/// Generates Kasane WIT bindings with embedded WIT content plus SDK helper
/// functions (style/brush helpers, element builders, etc.).
pub(crate) fn kasane_generate_impl(input: TokenStream) -> TokenStream {
    let wit_bindings = if input.is_empty() {
        let wit_content = include_str!("../wit/plugin.wit");
        quote! {
            wit_bindgen::generate!({
                world: "kasane-plugin",
                inline: #wit_content,
            });
        }
    } else {
        let path = parse_macro_input!(input as syn::LitStr);
        quote! {
            wit_bindgen::generate!({
                world: "kasane-plugin",
                path: #path,
            });
        }
    };

    let sdk_helpers = generate_sdk_helpers();

    quote! {
        #wit_bindings
        #sdk_helpers
    }
    .into()
}

/// Generate SDK helper code emitted alongside WIT bindings.
///
/// This code lives in the user's crate so it can reference WIT-generated types
/// (Style, Brush, RgbColor, etc.) which are not accessible from the SDK crate.
pub(crate) fn generate_sdk_helpers() -> proc_macro2::TokenStream {
    let sdk_dirty_all = SDK_DIRTY_ALL;
    quote! {
        /// SDK-generated prelude and helper functions.
        ///
        /// Items are re-exported via glob import so that explicit user imports
        /// shadow them without conflict (standard Rust prelude pattern).
        #[doc(hidden)]
        #[allow(dead_code)]
        mod __kasane_sdk {
            pub use super::exports::kasane::plugin::plugin_api::Guest;
            pub use super::kasane::plugin::host_state;
            pub use super::kasane::plugin::element_builder;
            pub use super::kasane::plugin::types::*;

            // Re-export host-log for direct use
            pub use super::kasane::plugin::host_log;

            use super::kasane::plugin::types::*;

            impl ::core::default::Default for BootstrapEffects {
                fn default() -> Self {
                    Self { redraw: 0 }
                }
            }

            impl ::core::default::Default for SessionReadyEffects {
                fn default() -> Self {
                    Self {
                        redraw: 0,
                        commands: vec![],
                        scroll_plans: vec![],
                    }
                }
            }

            impl ::core::default::Default for RuntimeEffects {
                fn default() -> Self {
                    Self {
                        redraw: 0,
                        commands: vec![],
                        scroll_plans: vec![],
                    }
                }
            }

            /// Unified effects type — alias for `RuntimeEffects`.
            ///
            /// Use this in all lifecycle hooks. The `#[plugin]` macro
            /// auto-converts to the WIT-specific return type via `Into`.
            pub type Effects = RuntimeEffects;

            impl ::core::convert::From<RuntimeEffects> for BootstrapEffects {
                fn from(e: RuntimeEffects) -> Self {
                    Self { redraw: e.redraw }
                }
            }

            impl ::core::convert::From<RuntimeEffects> for SessionReadyEffects {
                fn from(e: RuntimeEffects) -> Self {
                    Self {
                        redraw: e.redraw,
                        commands: e.commands.into_iter().filter_map(|c| match c {
                            Command::SendKeys(keys) => Some(SessionReadyCommand::SendKeys(keys)),
                            Command::PasteClipboard => Some(SessionReadyCommand::PasteClipboard),
                            Command::PluginMessage(msg) => Some(SessionReadyCommand::PluginMessage(msg)),
                            _ => None,
                        }).collect(),
                        scroll_plans: e.scroll_plans,
                    }
                }
            }

            /// Create a default style (all brushes default, normal weight, no decorations).
            pub fn default_style() -> Style {
                Style {
                    fg: Brush::DefaultColor,
                    bg: Brush::DefaultColor,
                    font_weight: 400,
                    font_slant: FontSlant::Normal,
                    font_features: 0,
                    font_variations: vec![],
                    letter_spacing: 0.0,
                    underline: None,
                    strikethrough: None,
                    blink: false,
                    reverse: false,
                    dim: false,
                }
            }

            /// Create a style with only the foreground brush set.
            pub fn style_fg(brush: Brush) -> Style {
                let mut s = default_style();
                s.fg = brush;
                s
            }

            /// Create a style with only the background brush set.
            pub fn style_bg(brush: Brush) -> Style {
                let mut s = default_style();
                s.bg = brush;
                s
            }

            /// Create a style with foreground and background brushes.
            pub fn style_with(fg: Brush, bg: Brush) -> Style {
                let mut s = default_style();
                s.fg = fg;
                s.bg = bg;
                s
            }

            /// Create a style from `(fg, bg, underline_color, attrs_bits)`. The
            /// `attrs` parameter takes the legacy `attributes::*` bitset and
            /// decomposes it into the post-resolve `Style` fields. Plugin
            /// authors writing new code should construct `Style` directly or
            /// use the granular helpers.
            pub fn style_full(fg: Brush, bg: Brush, underline_color: Brush, attrs: u16) -> Style {
                const UNDERLINE: u16 = 1 << 0;
                const CURLY_UNDERLINE: u16 = 1 << 1;
                const DOUBLE_UNDERLINE: u16 = 1 << 2;
                const REVERSE: u16 = 1 << 3;
                const BLINK: u16 = 1 << 4;
                const BOLD: u16 = 1 << 5;
                const DIM: u16 = 1 << 6;
                const ITALIC: u16 = 1 << 7;
                const STRIKETHROUGH: u16 = 1 << 8;

                let underline = if attrs & (UNDERLINE | CURLY_UNDERLINE | DOUBLE_UNDERLINE) != 0 {
                    let dec_style = if attrs & CURLY_UNDERLINE != 0 {
                        DecorationStyle::Curly
                    } else if attrs & DOUBLE_UNDERLINE != 0 {
                        DecorationStyle::Double
                    } else {
                        DecorationStyle::Solid
                    };
                    Some(TextDecoration { style: dec_style, color: underline_color, thickness: None })
                } else {
                    None
                };
                let strikethrough = if attrs & STRIKETHROUGH != 0 {
                    Some(TextDecoration { style: DecorationStyle::Solid, color: Brush::DefaultColor, thickness: None })
                } else {
                    None
                };
                Style {
                    fg,
                    bg,
                    font_weight: if attrs & BOLD != 0 { 700 } else { 400 },
                    font_slant: if attrs & ITALIC != 0 { FontSlant::Italic } else { FontSlant::Normal },
                    font_features: 0,
                    font_variations: vec![],
                    letter_spacing: 0.0,
                    underline,
                    strikethrough,
                    blink: attrs & BLINK != 0,
                    reverse: attrs & REVERSE != 0,
                    dim: attrs & DIM != 0,
                }
            }

            /// Create an RGB brush.
            pub fn rgb(r: u8, g: u8, b: u8) -> Brush {
                Brush::Rgb(RgbColor { r, g, b })
            }

            /// Create a named brush.
            pub fn named(n: NamedColor) -> Brush {
                Brush::Named(n)
            }

            // ----- Element builder shorthand wrappers -----

            /// Create a text element.
            pub fn text(content: &str, s: Style) -> ElementHandle {
                super::kasane::plugin::element_builder::create_text(content, &s)
            }

            /// Create a styled-line element from atoms.
            pub fn styled_line(atoms: &[Atom]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_styled_line(atoms)
            }

            /// Create a vertical column of children.
            pub fn column(children: &[ElementHandle]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_column(children)
            }

            /// Create a horizontal row of children.
            pub fn row(children: &[ElementHandle]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_row(children)
            }

            /// Create an interactive wrapper element.
            pub fn interactive(child: ElementHandle, id: InteractiveId) -> ElementHandle {
                super::kasane::plugin::element_builder::create_interactive(child, id)
            }

            /// Create an image element from a file path. Size is in cells.
            pub fn image_file(path: &str, width: u16, height: u16) -> ElementHandle {
                let source = super::kasane::plugin::types::ImageSource::FilePath(path.to_string());
                super::kasane::plugin::element_builder::create_image(
                    &source,
                    width,
                    height,
                    super::kasane::plugin::types::ImageFit::Contain,
                    1.0,
                )
            }

            /// Create an image element from inline RGBA data. Size is in cells.
            pub fn image_rgba(
                data: &[u8],
                img_width: u32,
                img_height: u32,
                cell_width: u16,
                cell_height: u16,
            ) -> ElementHandle {
                let source = super::kasane::plugin::types::ImageSource::RgbaData(
                    super::kasane::plugin::types::RgbaImage {
                        data: data.to_vec(),
                        width: img_width,
                        height: img_height,
                    },
                );
                super::kasane::plugin::element_builder::create_image(
                    &source,
                    cell_width,
                    cell_height,
                    super::kasane::plugin::types::ImageFit::Contain,
                    1.0,
                )
            }

            /// Create an image element from inline SVG data. Size is in cells.
            pub fn image_svg(svg_data: &[u8], width: u16, height: u16) -> ElementHandle {
                let source = super::kasane::plugin::types::ImageSource::SvgData(
                    svg_data.to_vec(),
                );
                super::kasane::plugin::element_builder::create_image(
                    &source,
                    width,
                    height,
                    super::kasane::plugin::types::ImageFit::Contain,
                    1.0,
                )
            }

            /// Create an empty element.
            pub fn empty() -> ElementHandle {
                super::kasane::plugin::element_builder::create_empty()
            }

            /// Create a horizontal flex layout with proportional children.
            ///
            /// Each `FlexEntry` specifies a child and its flex weight.
            /// Use `flex_entry()` to create entries conveniently.
            pub fn flex_row(children: &[FlexEntry], gap: u16) -> ElementHandle {
                super::kasane::plugin::element_builder::create_row_flex(children, gap)
            }

            /// Create a vertical flex layout with proportional children.
            ///
            /// Each `FlexEntry` specifies a child and its flex weight.
            /// Use `flex_entry()` to create entries conveniently.
            pub fn flex_column(children: &[FlexEntry], gap: u16) -> ElementHandle {
                super::kasane::plugin::element_builder::create_column_flex(children, gap)
            }

            /// Create a 2D grid layout.
            ///
            /// `columns` defines the width of each column (fixed, flex, or auto).
            /// `children` are placed left-to-right, top-to-bottom.
            pub fn grid(
                columns: &[GridWidth],
                children: &[ElementHandle],
                col_gap: u16,
                row_gap: u16,
            ) -> ElementHandle {
                super::kasane::plugin::element_builder::create_grid(columns, children, col_gap, row_gap)
            }

            /// Create a scrollable wrapper around a child element.
            ///
            /// `offset` is the scroll position (in lines for vertical, columns for horizontal).
            /// `vertical` selects the scroll axis.
            pub fn scrollable(child: ElementHandle, offset: u16, vertical: bool) -> ElementHandle {
                super::kasane::plugin::element_builder::create_scrollable(child, offset, vertical)
            }

            /// Create a text panel element: scrollable rich text area with optional line numbers.
            ///
            /// `lines` contains the styled text for each line.
            /// `scroll_offset` is the number of lines scrolled from the top.
            /// `cursor` is an optional (line, column) position for highlighting.
            pub fn text_panel(
                lines: &[Vec<Atom>],
                scroll_offset: u32,
                cursor: Option<(u32, u32)>,
                line_numbers: bool,
                wrap: bool,
            ) -> ElementHandle {
                let (cursor_line, cursor_col) = match cursor {
                    Some((l, c)) => (Some(l), Some(c)),
                    None => (None, None),
                };
                super::kasane::plugin::element_builder::create_text_panel(
                    lines, scroll_offset, cursor_line, cursor_col, line_numbers, wrap,
                )
            }

            /// Create a canvas element for GPU drawing operations.
            ///
            /// Size is in cells (width, height). The ops are rendered by the GPU
            /// backend; the TUI backend shows an empty area.
            pub fn canvas(width: u16, height: u16, ops: &[CanvasDrawOp]) -> ElementHandle {
                super::kasane::plugin::element_builder::create_canvas(width, height, ops)
            }

            /// Create a `FlexEntry` pairing a child element with a flex weight.
            pub fn flex_entry(child: ElementHandle, flex: f32) -> FlexEntry {
                FlexEntry { child, flex }
            }

            impl FlexEntry {
                /// Create a new `FlexEntry` pairing a child element with a flex weight.
                pub fn new(child: ElementHandle, flex: f32) -> Self {
                    Self { child, flex }
                }
            }

            // ----- LineAnnotation shortcuts -----

            /// Create a background-only line annotation.
            pub fn bg_annotation(s: Style) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: Some(BackgroundLayer {
                        style: s,
                        z_order: 0,
                        blend_opaque: true,
                    }),
                    priority: 0,
                    inline: None,
                    virtual_text: vec![],
                }
            }

            /// Create a left-gutter line annotation.
            pub fn gutter_annotation(el: ElementHandle, priority: i16) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: Some(el),
                    right_gutter: None,
                    background: None,
                    priority,
                    inline: None,
                    virtual_text: vec![],
                }
            }

            /// Create an EOL virtual text annotation.
            pub fn eol_annotation(atoms: Vec<Atom>, priority: i16) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: None,
                    priority: 0,
                    inline: None,
                    virtual_text: vec![VirtualTextItem { atoms, priority }],
                }
            }

            // ----- Contribution shortcut -----

            /// Create a contribution with auto size hint and priority 0.
            pub fn auto_contribution(element: ElementHandle) -> Contribution {
                Contribution {
                    element,
                    priority: 0,
                    size_hint: ContribSizeHint::Auto,
                }
            }

            // ----- Convenience shortcuts -----

            /// Create a text element with the default style.
            pub fn plain(s: &str) -> ElementHandle {
                text(s, default_style())
            }

            /// Create a text element with a foreground brush.
            pub fn colored(s: &str, fg: Brush) -> ElementHandle {
                text(s, style_fg(fg))
            }

            /// Check if a key event is Ctrl+key (no Alt/Shift).
            pub fn is_ctrl(event: &KeyEvent, c: char) -> bool {
                matches!(event.key, KeyCode::Char(cp) if cp == c as u32)
                    && event.modifiers == 0x01 // CTRL only
            }

            /// Check if a key event is Alt+key (no Ctrl/Shift).
            pub fn is_alt(event: &KeyEvent, c: char) -> bool {
                matches!(event.key, KeyCode::Char(cp) if cp == c as u32)
                    && event.modifiers == 0x02 // ALT only
            }

            /// Check if a key event is Ctrl+Shift+key (no Alt).
            pub fn is_ctrl_shift(event: &KeyEvent, c: char) -> bool {
                matches!(event.key, KeyCode::Char(cp) if cp == c as u32)
                    && event.modifiers == (0x01 | 0x04) // CTRL + SHIFT
            }

            /// Check if a key event is a plain character with no command modifiers (no Ctrl/Alt).
            pub fn is_plain(event: &KeyEvent, c: char) -> bool {
                matches!(event.key, KeyCode::Char(cp) if cp == c as u32)
                    && event.modifiers & 0x03 == 0 // no CTRL or ALT
            }

            /// Extract the plain character from a key event (no Ctrl/Alt), if any.
            pub fn plain_char(event: &KeyEvent) -> Option<char> {
                if event.modifiers & 0x03 != 0 { return None; }
                match event.key {
                    KeyCode::Char(cp) => char::from_u32(cp),
                    _ => None,
                }
            }

            /// Check if a key event has no command modifiers (Ctrl/Alt).
            pub fn has_no_command_modifier(event: &KeyEvent) -> bool {
                event.modifiers & 0x03 == 0
            }

            /// Conditional status bar badge: returns a contribution if `condition` is true.
            pub fn status_badge(condition: bool, label: &str) -> Option<Contribution> {
                condition.then(|| auto_contribution(plain(label)))
            }

            /// Parse a hex color string (`"#rrggbb"` or `"#rgb"`) into a `Brush`.
            /// Returns `Brush::DefaultColor` on invalid input.
            pub fn hex(s: &str) -> Brush {
                let s = s.strip_prefix('#').unwrap_or(s);
                match s.len() {
                    6 => {
                        let Ok(r) = u8::from_str_radix(&s[0..2], 16) else { return Brush::DefaultColor };
                        let Ok(g) = u8::from_str_radix(&s[2..4], 16) else { return Brush::DefaultColor };
                        let Ok(b) = u8::from_str_radix(&s[4..6], 16) else { return Brush::DefaultColor };
                        Brush::Rgb(RgbColor { r, g, b })
                    }
                    3 => {
                        let Ok(r) = u8::from_str_radix(&s[0..1], 16) else { return Brush::DefaultColor };
                        let Ok(g) = u8::from_str_radix(&s[1..2], 16) else { return Brush::DefaultColor };
                        let Ok(b) = u8::from_str_radix(&s[2..3], 16) else { return Brush::DefaultColor };
                        Brush::Rgb(RgbColor { r: r * 17, g: g * 17, b: b * 17 })
                    }
                    _ => Brush::DefaultColor,
                }
            }

            // ----- Theme helpers -----

            /// Query the host theme for a style associated with a token name.
            /// Returns `None` if the token is not in the theme.
            pub fn get_theme_style(token: &str) -> Option<Style> {
                super::kasane::plugin::host_state::get_theme_face(token)
            }

            /// Whether the current editor background is dark.
            pub fn is_dark_background() -> bool {
                super::kasane::plugin::host_state::is_dark_background()
            }

            /// Query the host theme for a style, falling back to `fallback` if not found.
            pub fn theme_style_or(token: &str, fallback: Style) -> Style {
                get_theme_style(token).unwrap_or(fallback)
            }

            // ----- Command shortcuts -----

            /// Request a full redraw (all dirty flags).
            pub fn redraw() -> Vec<Command> {
                vec![Command::RequestRedraw(#sdk_dirty_all)]
            }

            /// Request a redraw with specific dirty flags.
            pub fn redraw_flags(flags: u16) -> Vec<Command> {
                vec![Command::RequestRedraw(flags)]
            }

            /// Build a `Command::SendKeys` that runs a Kakoune command.
            pub fn send_command(cmd: &str) -> Command {
                Command::SendKeys(kasane_plugin_sdk::keys::command(cmd))
            }

            /// Build a `Command::PasteClipboard` that inserts text from the host system clipboard.
            ///
            /// This is distinct from committed text input or bracketed paste payloads,
            /// which the host routes through the text-input pipeline directly.
            pub fn paste_clipboard() -> Command {
                Command::PasteClipboard
            }

            /// Build a single-atom Fold display directive.
            pub fn plain_fold(range_start: u32, range_end: u32, text: &str, style: Style) -> DisplayDirective {
                DisplayDirective::Fold(FoldDirective {
                    range_start,
                    range_end,
                    summary: vec![Atom { style, contents: text.to_string() }],
                })
            }

            // ----- Unified display directive helpers -----

            /// Style an entire line with a background style.
            pub fn style_line(line: u32, style: Style) -> DisplayDirective {
                DisplayDirective::StyleLine(StyleLineDirective {
                    line, style, z_order: 0,
                })
            }

            /// Style a line with a specific z-order for layering.
            pub fn style_line_z(line: u32, style: Style, z_order: i16) -> DisplayDirective {
                DisplayDirective::StyleLine(StyleLineDirective {
                    line, style, z_order,
                })
            }

            /// Add a gutter element to a line.
            pub fn gutter_left(line: u32, content: ElementHandle, priority: i16) -> DisplayDirective {
                DisplayDirective::Gutter(GutterDirective {
                    line, side: DisplayGutterSide::Left, content, priority,
                })
            }

            /// Add a right gutter element to a line.
            pub fn gutter_right(line: u32, content: ElementHandle, priority: i16) -> DisplayDirective {
                DisplayDirective::Gutter(GutterDirective {
                    line, side: DisplayGutterSide::Right, content, priority,
                })
            }

            /// Add virtual text at the end of a line.
            pub fn virtual_text_eol(line: u32, atoms: Vec<Atom>, priority: i16) -> DisplayDirective {
                DisplayDirective::VirtualText(VirtualTextDirective {
                    line, position: DisplayVtPosition::EndOfLine, content: atoms, priority,
                })
            }

            /// Insert an element before a line.
            pub fn insert_before(line: u32, content: ElementHandle, priority: i16) -> DisplayDirective {
                DisplayDirective::InsertBefore(InterlineDirective {
                    line, content, priority,
                })
            }

            /// Insert an element after a line.
            pub fn insert_after(line: u32, content: ElementHandle, priority: i16) -> DisplayDirective {
                DisplayDirective::InsertAfter(InterlineDirective {
                    line, content, priority,
                })
            }

            /// Style an inline byte range on a line.
            pub fn style_inline(line: u32, byte_start: u32, byte_end: u32, style: Style) -> DisplayDirective {
                DisplayDirective::StyleInline(StyleInlineDirective {
                    line, byte_start, byte_end, style,
                })
            }

            /// Insert inline content at a byte offset.
            pub fn insert_inline(line: u32, byte_offset: u32, content: Vec<Atom>) -> DisplayDirective {
                DisplayDirective::InsertInline(InsertInlineDirective {
                    line, byte_offset, content, interactive_id: None,
                })
            }

            /// Hide an inline byte range on a line.
            pub fn hide_inline(line: u32, byte_start: u32, byte_end: u32) -> DisplayDirective {
                DisplayDirective::HideInline(HideInlineDirective {
                    line, byte_start, byte_end,
                })
            }

            /// Build a dynamic hosted surface registration command.
            pub fn register_surface(
                surface_key: &str,
                size_hint: SurfaceSizeHint,
                declared_slots: Vec<DeclaredSlot>,
                placement: SurfacePlacement,
            ) -> Command {
                Command::RegisterSurface(DynamicSurfaceConfig {
                    surface_key: surface_key.to_string(),
                    size_hint,
                    declared_slots,
                    placement,
                })
            }

            /// Build a dynamic hosted surface unregistration command.
            pub fn unregister_surface(surface_key: &str) -> Command {
                Command::UnregisterSurface(surface_key.to_string())
            }

            // ----- Edges shortcuts -----

            /// Create edges with explicit values.
            pub fn edges(top: u16, right: u16, bottom: u16, left: u16) -> Edges {
                Edges { top, right, bottom, left }
            }

            /// Create edges with horizontal padding only.
            pub fn padding_h(lr: u16) -> Edges {
                Edges { top: 0, right: lr, bottom: 0, left: lr }
            }

            // ----- Container builder -----

            /// Builder for container elements.
            pub struct ContainerBuilder {
                child: ElementHandle,
                border: Option<BorderLineStyle>,
                shadow: bool,
                padding: Edges,
                style: Style,
                title: Option<Vec<Atom>>,
            }

            impl ContainerBuilder {
                pub fn new(child: ElementHandle) -> Self {
                    Self {
                        child,
                        border: None,
                        shadow: false,
                        padding: Edges { top: 0, right: 0, bottom: 0, left: 0 },
                        style: default_style(),
                        title: None,
                    }
                }

                pub fn border(mut self, style: BorderLineStyle) -> Self {
                    self.border = Some(style);
                    self
                }

                pub fn shadow(mut self) -> Self {
                    self.shadow = true;
                    self
                }

                pub fn padding(mut self, edges: Edges) -> Self {
                    self.padding = edges;
                    self
                }

                pub fn style(mut self, style: Style) -> Self {
                    self.style = style;
                    self
                }

                pub fn title_text(mut self, text: &str) -> Self {
                    self.title = Some(vec![Atom {
                        style: default_style(),
                        contents: text.to_string(),
                    }]);
                    self
                }

                pub fn title(mut self, atoms: &[Atom]) -> Self {
                    self.title = Some(atoms.to_vec());
                    self
                }

                pub fn build(self) -> ElementHandle {
                    super::kasane::plugin::element_builder::create_container_styled(
                        self.child,
                        self.border,
                        self.shadow,
                        self.padding,
                        &self.style,
                        self.title.as_deref(),
                    )
                }
            }

            /// Start building a container element.
            pub fn container(child: ElementHandle) -> ContainerBuilder {
                ContainerBuilder::new(child)
            }

            /// Compute a centered overlay `AbsoluteAnchor` given screen dimensions,
            /// desired size as percentages, and minimum dimensions.
            pub fn centered_overlay(
                screen_cols: u16,
                screen_rows: u16,
                w_pct: u32,
                h_pct: u32,
                min_w: u16,
                min_h: u16,
            ) -> AbsoluteAnchor {
                let w = (screen_cols as u32 * w_pct / 100)
                    .max(min_w as u32)
                    .min(screen_cols as u32) as u16;
                let h = (screen_rows as u32 * h_pct / 100)
                    .max(min_h as u32)
                    .min(screen_rows as u32) as u16;
                let x = (screen_cols.saturating_sub(w)) / 2;
                let y = (screen_rows.saturating_sub(h)) / 2;
                AbsoluteAnchor { x, y, w, h }
            }

            /// Centered overlay sized to fit content rows (auto height).
            pub fn content_fit_overlay(
                screen_cols: u16,
                screen_rows: u16,
                w_pct: u32,
                min_w: u16,
                content_rows: u16,
                chrome_rows: u16,
            ) -> AbsoluteAnchor {
                let w = (screen_cols as u32 * w_pct / 100)
                    .max(min_w as u32)
                    .min(screen_cols as u32) as u16;
                let h = (content_rows + chrome_rows).min(screen_rows);
                let x = (screen_cols.saturating_sub(w)) / 2;
                let y = (screen_rows.saturating_sub(h)) / 2;
                AbsoluteAnchor { x, y, w, h }
            }

            // --- Logging helpers ---

            pub fn log_debug(msg: &str) {
                host_log::log_message(host_log::LogLevel::Debug, msg);
            }

            pub fn log_info(msg: &str) {
                host_log::log_message(host_log::LogLevel::Info, msg);
            }

            pub fn log_warn(msg: &str) {
                host_log::log_message(host_log::LogLevel::Warn, msg);
            }

            pub fn log_error(msg: &str) {
                host_log::log_message(host_log::LogLevel::Error, msg);
            }

            // --- Effects shortcuts ---

            /// Effects with commands only (no redraw flag, no scroll).
            pub fn effects(commands: Vec<Command>) -> Effects {
                Effects { redraw: 0, commands, scroll_plans: vec![] }
            }

            /// Effects with commands + trailing RequestRedraw(ALL).
            pub fn effects_redraw(mut commands: Vec<Command>) -> Effects {
                commands.push(Command::RequestRedraw(#sdk_dirty_all));
                Effects { redraw: 0, commands, scroll_plans: vec![] }
            }

            /// Effects with only RequestRedraw(ALL).
            pub fn just_redraw() -> Effects {
                Effects {
                    redraw: 0,
                    commands: vec![Command::RequestRedraw(#sdk_dirty_all)],
                    scroll_plans: vec![],
                }
            }

            // --- Key handler shortcuts ---

            /// Consume key with RequestRedraw(ALL).
            pub fn consumed_redraw() -> Option<Vec<Command>> {
                Some(vec![Command::RequestRedraw(#sdk_dirty_all)])
            }

            /// Consume key with no side effects.
            pub fn consumed() -> Option<Vec<Command>> {
                Some(vec![])
            }

            /// Navigate up in a list and redraw.
            pub fn nav_up(selected: &mut usize) -> Option<Vec<Command>> {
                if *selected > 0 { *selected -= 1; }
                consumed_redraw()
            }

            /// Navigate down in a list and redraw.
            pub fn nav_down(selected: &mut usize, len: usize) -> Option<Vec<Command>> {
                if len > 0 && *selected < len - 1 { *selected += 1; }
                consumed_redraw()
            }

            // ----- HTTP request helpers -----

            /// Build a simple HTTP GET command.
            pub fn http_get(job_id: u64, url: &str) -> Command {
                Command::HttpRequest(HttpRequestConfig {
                    job_id,
                    url: url.to_string(),
                    method: HttpMethod::Get,
                    headers: vec![],
                    body: None,
                    timeout_ms: 30_000,
                    idle_timeout_ms: 0,
                    streaming: StreamingMode::Buffered,
                })
            }

            /// Build an HTTP POST command with a body.
            pub fn http_post(job_id: u64, url: &str, body: Vec<u8>) -> Command {
                Command::HttpRequest(HttpRequestConfig {
                    job_id,
                    url: url.to_string(),
                    method: HttpMethod::Post,
                    headers: vec![],
                    body: Some(body),
                    timeout_ms: 30_000,
                    idle_timeout_ms: 0,
                    streaming: StreamingMode::Buffered,
                })
            }

            /// Build an HTTP POST command with a JSON body.
            ///
            /// Automatically sets `Content-Type: application/json`.
            pub fn http_post_json(job_id: u64, url: &str, json_body: &str) -> Command {
                Command::HttpRequest(HttpRequestConfig {
                    job_id,
                    url: url.to_string(),
                    method: HttpMethod::Post,
                    headers: vec![("content-type".to_string(), "application/json".to_string())],
                    body: Some(json_body.as_bytes().to_vec()),
                    timeout_ms: 30_000,
                    idle_timeout_ms: 0,
                    streaming: StreamingMode::Buffered,
                })
            }

            /// Build an HTTP request command from a full configuration.
            pub fn http_request(config: HttpRequestConfig) -> Command {
                Command::HttpRequest(config)
            }

            /// Build a cancel-HTTP-request command.
            pub fn cancel_http(job_id: u64) -> Command {
                Command::CancelHttpRequest(job_id)
            }

            // --- WIT → SDK event conversion ---

            /// Convert WIT ProcessEventKind to SDK IoEventKind.
            pub fn to_io_event_kind(kind: &ProcessEventKind) -> ::kasane_plugin_sdk::process::IoEventKind<'_> {
                match kind {
                    ProcessEventKind::Stdout(d) => ::kasane_plugin_sdk::process::IoEventKind::Stdout(d),
                    ProcessEventKind::Stderr(d) => ::kasane_plugin_sdk::process::IoEventKind::Stderr(d),
                    ProcessEventKind::Exited(c) => ::kasane_plugin_sdk::process::IoEventKind::Exited(*c),
                    ProcessEventKind::SpawnFailed(e) => ::kasane_plugin_sdk::process::IoEventKind::SpawnFailed(e),
                }
            }
        }

        #[allow(unused_imports)]
        use __kasane_sdk::*;
    }
}
