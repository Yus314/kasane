use crate::protocol::{Color, WireFace};

/// Color context derived from Kakoune's default_face.
///
/// Provides automatic chrome color derivation based on the editor's
/// background brightness, enabling zero-config harmony with any Kakoune
/// color scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorContext {
    /// Whether the background is dark (true) or light (false).
    pub is_dark: bool,
    /// Classification of color knowledge.
    pub knowledge: ColorKnowledge,
    /// Derived chrome palette (only available for K2/K3).
    pub chrome: Option<ChromePalette>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorKnowledge {
    /// Color::Default -- RGB unknown. Attribute-based fallback.
    K1,
    /// Named -- xterm RGB approximation for medium-quality derivation.
    K2,
    /// Rgb -- full arithmetic derivation.
    K3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChromePalette {
    /// Border/divider color: bg shifted toward fg.
    pub chrome_bg: Color,
    /// Shadow/muted text color: fg shifted toward bg.
    pub dim_fg: Color,
}

impl Default for ColorContext {
    fn default() -> Self {
        Self {
            is_dark: true,
            knowledge: ColorKnowledge::K1,
            chrome: None,
        }
    }
}

impl ColorContext {
    /// Derive color context from Kakoune's default_face.
    pub fn derive(default_face: &WireFace) -> Self {
        let fg_rgb = default_face.fg.to_rgb();
        let bg_rgb = default_face.bg.to_rgb();

        let knowledge = match (&default_face.fg, &default_face.bg) {
            (Color::Rgb { .. }, _) | (_, Color::Rgb { .. }) => ColorKnowledge::K3,
            (Color::Named(_), _) | (_, Color::Named(_)) => ColorKnowledge::K2,
            _ => ColorKnowledge::K1,
        };

        if knowledge == ColorKnowledge::K1 {
            return Self {
                is_dark: true,
                knowledge,
                chrome: None,
            };
        }

        // Resolve RGB values (Named colors have been converted via to_rgb)
        let bg = bg_rgb.unwrap_or((0, 0, 0));
        let fg = fg_rgb.unwrap_or((229, 229, 229));

        let is_dark = perceived_luminance(bg.0, bg.1, bg.2) < 128;

        let chrome = Some(ChromePalette {
            chrome_bg: linear_blend(bg, fg, 0.15),
            dim_fg: linear_blend(fg, bg, 0.4),
        });

        Self {
            is_dark,
            knowledge,
            chrome,
        }
    }
}

/// Perceived luminance using the ITU-R BT.601 formula.
fn perceived_luminance(r: u8, g: u8, b: u8) -> u16 {
    ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u16
}

// ---------------------------------------------------------------------------
// Linear-space color blending
// ---------------------------------------------------------------------------

/// Convert a single sRGB component (0–255) to linear light (0.0–1.0).
fn srgb_to_linear(c: u8) -> f32 {
    let s = c as f32 / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert a linear light value (0.0–1.0) back to sRGB (0–255).
fn linear_to_srgb(c: f32) -> u8 {
    let s = if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// Blend two colors in linear light space: result = from * (1-ratio) + to * ratio.
///
/// Produces perceptually uniform blending by converting sRGB → linear → blend → sRGB.
pub(crate) fn linear_blend(from: (u8, u8, u8), to: (u8, u8, u8), ratio: f32) -> Color {
    let r = srgb_to_linear(from.0) * (1.0 - ratio) + srgb_to_linear(to.0) * ratio;
    let g = srgb_to_linear(from.1) * (1.0 - ratio) + srgb_to_linear(to.1) * ratio;
    let b = srgb_to_linear(from.2) * (1.0 - ratio) + srgb_to_linear(to.2) * ratio;
    Color::Rgb {
        r: linear_to_srgb(r),
        g: linear_to_srgb(g),
        b: linear_to_srgb(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::NamedColor;

    #[test]
    fn derive_k3_dark_bg() {
        let face = WireFace {
            fg: Color::Rgb {
                r: 200,
                g: 200,
                b: 200,
            },
            bg: Color::Rgb {
                r: 30,
                g: 30,
                b: 30,
            },
            ..WireFace::default()
        };
        let ctx = ColorContext::derive(&face);
        assert_eq!(ctx.knowledge, ColorKnowledge::K3);
        assert!(ctx.is_dark);
        assert!(ctx.chrome.is_some());
    }

    #[test]
    fn derive_k3_light_bg() {
        let face = WireFace {
            fg: Color::Rgb {
                r: 30,
                g: 30,
                b: 30,
            },
            bg: Color::Rgb {
                r: 240,
                g: 240,
                b: 240,
            },
            ..WireFace::default()
        };
        let ctx = ColorContext::derive(&face);
        assert_eq!(ctx.knowledge, ColorKnowledge::K3);
        assert!(!ctx.is_dark);
        assert!(ctx.chrome.is_some());
    }

    #[test]
    fn derive_k2_named_colors() {
        let face = WireFace {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..WireFace::default()
        };
        let ctx = ColorContext::derive(&face);
        assert_eq!(ctx.knowledge, ColorKnowledge::K2);
        assert!(ctx.is_dark);
        assert!(ctx.chrome.is_some());
    }

    #[test]
    fn derive_k1_default_colors() {
        let face = WireFace::default();
        let ctx = ColorContext::derive(&face);
        assert_eq!(ctx.knowledge, ColorKnowledge::K1);
        assert!(ctx.is_dark);
        assert!(ctx.chrome.is_none());
    }

    #[test]
    fn perceived_luminance_black() {
        assert_eq!(perceived_luminance(0, 0, 0), 0);
    }

    #[test]
    fn perceived_luminance_white() {
        assert_eq!(perceived_luminance(255, 255, 255), 255);
    }
}
