use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

/// SDK dirty::ALL value (excludes PLUGIN_STATE bit 7).
/// Must match `kasane_plugin_sdk::dirty::ALL`.
const SDK_DIRTY_ALL: u16 = 0x37F;

/// Implementation of the `kasane_generate` proc macro.
///
/// Generates Kasane WIT bindings with embedded WIT content plus SDK helper
/// functions (face/color helpers, element builders, etc.).
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
/// (Face, Color, RgbColor, etc.) which are not accessible from the SDK crate.
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

            /// Create a default face (all colors default, no attributes).
            pub fn default_face() -> Face {
                Face {
                    fg: Color::DefaultColor,
                    bg: Color::DefaultColor,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with only the foreground color set.
            pub fn face_fg(color: Color) -> Face {
                Face {
                    fg: color,
                    bg: Color::DefaultColor,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with only the background color set.
            pub fn face_bg(color: Color) -> Face {
                Face {
                    fg: Color::DefaultColor,
                    bg: color,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with foreground and background colors.
            pub fn face(fg: Color, bg: Color) -> Face {
                Face {
                    fg,
                    bg,
                    underline: Color::DefaultColor,
                    attributes: 0,
                }
            }

            /// Create a face with all fields specified.
            pub fn face_full(fg: Color, bg: Color, underline: Color, attrs: u16) -> Face {
                Face {
                    fg,
                    bg,
                    underline,
                    attributes: attrs,
                }
            }

            /// Create an RGB color.
            pub fn rgb(r: u8, g: u8, b: u8) -> Color {
                Color::Rgb(RgbColor { r, g, b })
            }

            /// Create a named color.
            pub fn named(n: NamedColor) -> Color {
                Color::Named(n)
            }

            // ----- Element builder shorthand wrappers -----

            /// Create a text element.
            pub fn text(content: &str, f: Face) -> ElementHandle {
                super::kasane::plugin::element_builder::create_text(content, f)
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
            pub fn bg_annotation(f: Face) -> LineAnnotation {
                LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: Some(BackgroundLayer {
                        face: f,
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

            /// Create a text element with the default face.
            pub fn plain(s: &str) -> ElementHandle {
                text(s, default_face())
            }

            /// Create a text element with a foreground color.
            pub fn colored(s: &str, fg: Color) -> ElementHandle {
                text(s, face_fg(fg))
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

            /// Parse a hex color string (`"#rrggbb"` or `"#rgb"`) into a `Color`.
            /// Returns `Color::DefaultColor` on invalid input.
            pub fn hex(s: &str) -> Color {
                let s = s.strip_prefix('#').unwrap_or(s);
                match s.len() {
                    6 => {
                        let Ok(r) = u8::from_str_radix(&s[0..2], 16) else { return Color::DefaultColor };
                        let Ok(g) = u8::from_str_radix(&s[2..4], 16) else { return Color::DefaultColor };
                        let Ok(b) = u8::from_str_radix(&s[4..6], 16) else { return Color::DefaultColor };
                        Color::Rgb(RgbColor { r, g, b })
                    }
                    3 => {
                        let Ok(r) = u8::from_str_radix(&s[0..1], 16) else { return Color::DefaultColor };
                        let Ok(g) = u8::from_str_radix(&s[1..2], 16) else { return Color::DefaultColor };
                        let Ok(b) = u8::from_str_radix(&s[2..3], 16) else { return Color::DefaultColor };
                        Color::Rgb(RgbColor { r: r * 17, g: g * 17, b: b * 17 })
                    }
                    _ => Color::DefaultColor,
                }
            }

            // ----- Theme helpers -----

            /// Query the host theme for a face associated with a token name.
            /// Returns `None` if the token is not in the theme.
            pub fn get_theme_face(token: &str) -> Option<Face> {
                super::kasane::plugin::host_state::get_theme_face(token)
            }

            /// Whether the current editor background is dark.
            pub fn is_dark_background() -> bool {
                super::kasane::plugin::host_state::is_dark_background()
            }

            /// Query the host theme for a face, falling back to `fallback` if not found.
            pub fn theme_face_or(token: &str, fallback: Face) -> Face {
                get_theme_face(token).unwrap_or(fallback)
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

            /// Build a single-atom InsertAfter display directive.
            pub fn plain_insert_after(after: u32, text: &str, face: Face) -> DisplayDirective {
                DisplayDirective::InsertAfter(InsertAfterDirective {
                    after,
                    content: vec![Atom { face, contents: text.to_string() }],
                })
            }

            /// Build a single-atom InsertBefore display directive.
            pub fn plain_insert_before(before: u32, text: &str, face: Face) -> DisplayDirective {
                DisplayDirective::InsertBefore(InsertBeforeDirective {
                    before,
                    content: vec![Atom { face, contents: text.to_string() }],
                })
            }

            /// Build a single-atom Fold display directive.
            pub fn plain_fold(range_start: u32, range_end: u32, text: &str, face: Face) -> DisplayDirective {
                DisplayDirective::Fold(FoldDirective {
                    range_start,
                    range_end,
                    summary: vec![Atom { face, contents: text.to_string() }],
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
                style: Face,
                title: Option<Vec<Atom>>,
            }

            impl ContainerBuilder {
                pub fn new(child: ElementHandle) -> Self {
                    Self {
                        child,
                        border: None,
                        shadow: false,
                        padding: Edges { top: 0, right: 0, bottom: 0, left: 0 },
                        style: Face {
                            fg: Color::DefaultColor,
                            bg: Color::DefaultColor,
                            underline: Color::DefaultColor,
                            attributes: 0,
                        },
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

                pub fn style(mut self, face: Face) -> Self {
                    self.style = face;
                    self
                }

                pub fn title_text(mut self, text: &str) -> Self {
                    self.title = Some(vec![Atom {
                        face: Face {
                            fg: Color::DefaultColor,
                            bg: Color::DefaultColor,
                            underline: Color::DefaultColor,
                            attributes: 0,
                        },
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
                        self.style,
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
