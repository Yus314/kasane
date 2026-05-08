use kasane_core::config::ColorsConfig;
use kasane_core::protocol::{Brush, Color, NamedColor, Style};

/// Resolves kasane-core `Color` values to GPU-ready `[f32; 4]` (sRGB, alpha=1.0).
///
/// In the TUI backend, `Color::Default` maps to terminal reset. In the GUI backend
/// there is no terminal, so we need explicit RGB values from the color configuration.
pub struct ColorResolver {
    /// [0] = default_fg, [1] = default_bg, [2..18] = 16 named colors
    palette: [[f32; 4]; 18],
}

impl ColorResolver {
    pub fn from_config(colors: &ColorsConfig) -> Self {
        let palette = [
            parse_hex_color(&colors.default_fg),
            parse_hex_color(&colors.default_bg),
            parse_hex_color(&colors.black),
            parse_hex_color(&colors.red),
            parse_hex_color(&colors.green),
            parse_hex_color(&colors.yellow),
            parse_hex_color(&colors.blue),
            parse_hex_color(&colors.magenta),
            parse_hex_color(&colors.cyan),
            parse_hex_color(&colors.white),
            parse_hex_color(&colors.bright_black),
            parse_hex_color(&colors.bright_red),
            parse_hex_color(&colors.bright_green),
            parse_hex_color(&colors.bright_yellow),
            parse_hex_color(&colors.bright_blue),
            parse_hex_color(&colors.bright_magenta),
            parse_hex_color(&colors.bright_cyan),
            parse_hex_color(&colors.bright_white),
        ];
        ColorResolver { palette }
    }

    /// Convert a kasane-core `Color` to a GPU-ready `[f32; 4]`.
    pub fn resolve(&self, color: Color, is_fg: bool) -> [f32; 4] {
        match color {
            Color::Default => {
                if is_fg {
                    self.palette[0]
                } else {
                    self.palette[1]
                }
            }
            Color::Named(n) => self.palette[2 + named_color_index(n)],
            Color::Rgb { r, g, b } => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0],
        }
    }

    /// Sync default fg/bg from Kakoune's resolved `default_style`.
    ///
    /// The draw event sends a default style with the theme's resolved default
    /// colours. Use those instead of the static `ColorsConfig` fallback so
    /// that `Brush::Default` resolves to the active colorscheme's defaults.
    pub fn sync_defaults(&mut self, style: &Style) {
        if !style.fg.is_inherit() {
            self.palette[0] = self.resolve_brush(style.fg, true);
        }
        if !style.bg.is_inherit() {
            self.palette[1] = self.resolve_brush(style.bg, false);
        }
    }

    /// Convert a kasane-core `Color` to a GPU-ready `[f32; 4]` in **linear** color space.
    pub fn resolve_linear(&self, color: Color, is_fg: bool) -> [f32; 4] {
        srgb_color_to_linear(self.resolve(color, is_fg))
    }

    /// Default background color as `[f32; 4]`.
    pub fn default_bg(&self) -> [f32; 4] {
        self.palette[1]
    }

    /// Default background color in linear color space.
    pub fn default_bg_linear(&self) -> [f32; 4] {
        srgb_color_to_linear(self.palette[1])
    }

    /// Convert a kasane-core [`Brush`] to a GPU-ready `[f32; 4]` (sRGB).
    ///
    /// `Brush::Default` resolves to the renderer's default fg or bg
    /// depending on `is_fg`. `Brush::Solid` is passed through with its
    /// own alpha channel (`Color::Rgb` does not carry alpha, so this is
    /// the only path that produces a non-1.0 alpha).
    pub fn resolve_brush(&self, brush: Brush, is_fg: bool) -> [f32; 4] {
        match brush {
            Brush::Default => {
                if is_fg {
                    self.palette[0]
                } else {
                    self.palette[1]
                }
            }
            Brush::Named(n) => self.palette[2 + named_color_index(n)],
            Brush::Solid([r, g, b, a]) => [
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ],
        }
    }

    /// Resolve a [`Style`]'s fg/bg to GPU colors (sRGB), applying the
    /// `reverse` attribute. Returns `(visual_fg, visual_bg, needs_bg)`.
    pub fn resolve_style_colors(&self, style: &Style) -> ([f32; 4], [f32; 4], bool) {
        let raw_fg = self.resolve_brush(style.fg, true);
        let raw_bg = self.resolve_brush(style.bg, false);
        if style.reverse {
            (raw_bg, raw_fg, true)
        } else {
            (raw_fg, raw_bg, style.bg != Brush::Default)
        }
    }

    /// [`Self::resolve_style_colors`] in linear colour space.
    pub fn resolve_style_colors_linear(&self, style: &Style) -> ([f32; 4], [f32; 4], bool) {
        let (fg, bg, needs_bg) = self.resolve_style_colors(style);
        (srgb_color_to_linear(fg), srgb_color_to_linear(bg), needs_bg)
    }
}

fn named_color_index(c: NamedColor) -> usize {
    match c {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
    }
}

/// Convert all RGB components of a color from sRGB to linear, preserving alpha.
pub fn srgb_color_to_linear(c: [f32; 4]) -> [f32; 4] {
    [
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
        c[3],
    ]
}

/// Convert a single sRGB component (0.0–1.0) to linear light.
///
/// Uses the ITU-R BT.709 transfer function (same as glyphon's shader).
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Parse a `#rrggbb` hex string into `[f32; 4]`. Falls back to opaque black on error.
fn parse_hex_color(hex: &str) -> [f32; 4] {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        let c = parse_hex_color("#ff8000");
        assert!((c[0] - 1.0).abs() < 0.01);
        assert!((c[1] - 0.502).abs() < 0.01);
        assert!((c[2] - 0.0).abs() < 0.01);
        assert!((c[3] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_hex_no_hash() {
        let c = parse_hex_color("00ff00");
        assert!((c[0] - 0.0).abs() < 0.01);
        assert!((c[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_hex_invalid() {
        let c = parse_hex_color("nope");
        assert_eq!(c, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_srgb_to_linear() {
        assert!((srgb_to_linear(0.0) - 0.0).abs() < 1e-6);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert!((srgb_to_linear(0.5) - 0.214).abs() < 0.001);
    }

    #[test]
    fn test_resolve_default() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let fg = resolver.resolve(Color::Default, true);
        let bg = resolver.resolve(Color::Default, false);
        // default_fg = #d4d4d4, default_bg = #1e1e1e
        assert!((fg[0] - 0.831).abs() < 0.01);
        assert!((bg[0] - 0.118).abs() < 0.01);
    }

    #[test]
    fn test_resolve_named() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let red = resolver.resolve(Color::Named(NamedColor::Red), true);
        // red = #cd3131
        assert!((red[0] - 0.804).abs() < 0.01);
    }

    #[test]
    fn test_resolve_rgb() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let c = resolver.resolve(
            Color::Rgb {
                r: 128,
                g: 0,
                b: 255,
            },
            true,
        );
        assert!((c[0] - 0.502).abs() < 0.01);
        assert!((c[1] - 0.0).abs() < 0.01);
        assert!((c[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_resolve_style_colors_no_reverse() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let style = Style {
            fg: Brush::Named(NamedColor::Red),
            bg: Brush::Named(NamedColor::Blue),
            ..Style::default()
        };
        let (vfg, vbg, needs_bg) = resolver.resolve_style_colors(&style);
        let expected_fg = resolver.resolve_brush(Brush::Named(NamedColor::Red), true);
        let expected_bg = resolver.resolve_brush(Brush::Named(NamedColor::Blue), false);
        assert_eq!(vfg, expected_fg);
        assert_eq!(vbg, expected_bg);
        assert!(needs_bg);
    }

    #[test]
    fn test_resolve_style_colors_reverse_default_swap() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let style = Style {
            fg: Brush::Default,
            bg: Brush::Default,
            reverse: true,
            ..Style::default()
        };
        let (vfg, vbg, _) = resolver.resolve_style_colors(&style);
        let default_fg = resolver.resolve_brush(Brush::Default, true);
        let default_bg = resolver.resolve_brush(Brush::Default, false);
        assert_eq!(vfg, default_bg);
        assert_eq!(vbg, default_fg);
    }

    #[test]
    fn test_resolve_style_colors_needs_bg() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);

        // REVERSE → always needs_bg
        let style_rev = Style {
            reverse: true,
            ..Style::default()
        };
        let (_, _, needs_bg) = resolver.resolve_style_colors(&style_rev);
        assert!(needs_bg);

        // No REVERSE, bg=Default → no needs_bg
        let style_default = Style::default();
        let (_, _, needs_bg) = resolver.resolve_style_colors(&style_default);
        assert!(!needs_bg);

        // No REVERSE, explicit bg → needs_bg
        let style_explicit = Style {
            bg: Brush::Named(NamedColor::Green),
            ..Style::default()
        };
        let (_, _, needs_bg) = resolver.resolve_style_colors(&style_explicit);
        assert!(needs_bg);
    }

    #[test]
    fn test_sync_defaults_rgb() {
        let config = ColorsConfig::default();
        let mut resolver = ColorResolver::from_config(&config);

        // Before sync: defaults are from ColorsConfig (dark theme)
        let old_fg = resolver.resolve(Color::Default, true);
        assert!((old_fg[0] - 0.831).abs() < 0.01); // #d4d4d4

        // Sync with Gruvbox Light default style
        let gruvbox_style = Style {
            fg: Brush::rgb(0x3c, 0x38, 0x36),
            bg: Brush::rgb(0xfb, 0xf1, 0xc7),
            ..Style::default()
        };
        resolver.sync_defaults(&gruvbox_style);

        // After sync: defaults match Gruvbox Light
        let new_fg = resolver.resolve(Color::Default, true);
        let new_bg = resolver.resolve(Color::Default, false);
        assert!((new_fg[0] - 0x3c as f32 / 255.0).abs() < 0.01); // dark brown
        assert!((new_bg[0] - 0xfb as f32 / 255.0).abs() < 0.01); // cream
    }

    #[test]
    fn test_sync_defaults_named() {
        let config = ColorsConfig::default();
        let mut resolver = ColorResolver::from_config(&config);
        let style = Style {
            fg: Brush::Named(NamedColor::Red),
            bg: Brush::Default, // should keep ColorsConfig fallback
            ..Style::default()
        };
        resolver.sync_defaults(&style);

        let fg = resolver.resolve(Color::Default, true);
        let red = resolver.resolve(Color::Named(NamedColor::Red), true);
        assert_eq!(fg, red); // default_fg now matches red

        // bg unchanged (style.bg was Default)
        let bg = resolver.resolve(Color::Default, false);
        assert!((bg[0] - 0.118).abs() < 0.01); // still #1e1e1e
    }

    #[test]
    fn test_sync_defaults_skip_default() {
        let config = ColorsConfig::default();
        let mut resolver = ColorResolver::from_config(&config);
        let old_fg = resolver.resolve(Color::Default, true);

        // Sync with style that has fg=Default → should not change
        resolver.sync_defaults(&Style::default());

        let new_fg = resolver.resolve(Color::Default, true);
        assert_eq!(old_fg, new_fg);
    }

    #[test]
    fn test_resolve_brush_default_picks_fg_or_bg() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let fg = resolver.resolve_brush(Brush::Default, true);
        let bg = resolver.resolve_brush(Brush::Default, false);
        // Defaults differ; matches Color::Default behaviour.
        assert_ne!(fg, bg);
        assert_eq!(fg, resolver.resolve(Color::Default, true));
        assert_eq!(bg, resolver.resolve(Color::Default, false));
    }

    #[test]
    fn test_resolve_brush_solid_carries_alpha() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        // Half-transparent red. Color::Rgb cannot represent this, so
        // the Solid path is the only way to reach a non-1.0 alpha.
        let half_red = resolver.resolve_brush(Brush::Solid([255, 0, 0, 128]), true);
        assert!((half_red[0] - 1.0).abs() < 0.01);
        assert!((half_red[3] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_resolve_brush_named_matches_legacy() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        for &n in &[
            NamedColor::Red,
            NamedColor::Cyan,
            NamedColor::BrightWhite,
            NamedColor::Black,
        ] {
            let via_brush = resolver.resolve_brush(Brush::Named(n), true);
            let via_color = resolver.resolve(Color::Named(n), true);
            assert_eq!(via_brush, via_color, "named colour {n:?}");
        }
    }

    #[test]
    fn test_resolve_style_colors_reverse_swaps() {
        let config = ColorsConfig::default();
        let resolver = ColorResolver::from_config(&config);
        let style = Style {
            fg: Brush::Named(NamedColor::Red),
            bg: Brush::Named(NamedColor::Blue),
            reverse: true,
            ..Style::default()
        };
        let (vfg, vbg, needs_bg) = resolver.resolve_style_colors(&style);
        assert_eq!(
            vfg,
            resolver.resolve_brush(Brush::Named(NamedColor::Blue), false)
        );
        assert_eq!(
            vbg,
            resolver.resolve_brush(Brush::Named(NamedColor::Red), true)
        );
        assert!(needs_bg);
    }

    #[test]
    fn test_sync_defaults_updates_default_bg() {
        let config = ColorsConfig::default();
        let mut resolver = ColorResolver::from_config(&config);
        let style = Style {
            bg: Brush::rgb(0xfb, 0xf1, 0xc7),
            ..Style::default()
        };
        resolver.sync_defaults(&style);

        // default_bg() should return the synced value
        let bg = resolver.default_bg();
        assert!((bg[0] - 0xfb as f32 / 255.0).abs() < 0.01);
    }
}
