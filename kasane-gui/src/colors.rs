use kasane_core::config::ColorsConfig;
use kasane_core::protocol::{Color, NamedColor};

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

    /// Default background color as `[f32; 4]`.
    pub fn default_bg(&self) -> [f32; 4] {
        self.palette[1]
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
}
