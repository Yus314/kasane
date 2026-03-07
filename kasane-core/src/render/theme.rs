use std::collections::HashMap;

use crate::config::ThemeConfig;
use crate::element::{Style, StyleToken};
use crate::protocol::{Attributes, Color, Face, NamedColor};

/// Theme maps StyleTokens to Faces for consistent visual styling.
#[derive(Debug, Clone)]
pub struct Theme {
    map: HashMap<StyleToken, Face>,
}

impl Theme {
    /// Create an empty theme (all tokens resolve to fallback).
    pub fn new() -> Self {
        Theme {
            map: HashMap::new(),
        }
    }

    /// Create the default theme with Kakoune-compatible face values.
    pub fn default_theme() -> Self {
        let mut map = HashMap::new();

        // Menu
        map.insert(
            StyleToken::MenuItemNormal,
            Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::MenuItemSelected,
            Face {
                fg: Color::Named(NamedColor::Blue),
                bg: Color::Named(NamedColor::White),
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::MenuScrollbar,
            Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::MenuScrollbarThumb,
            Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
        );

        // Info
        map.insert(
            StyleToken::InfoText,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::InfoBorder,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );

        // Status
        map.insert(
            StyleToken::StatusLine,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::StatusMode,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );

        // Shadow
        map.insert(
            StyleToken::Shadow,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                underline: Color::Default,
                attributes: Attributes::DIM,
            },
        );

        Theme { map }
    }

    /// Set a token's face.
    pub fn set(&mut self, token: StyleToken, face: Face) {
        self.map.insert(token, face);
    }

    /// Resolve a Style to a Face.
    /// - Direct(face) → returns that face
    /// - Token(token) → looks up in theme map, falls back to `fallback`
    pub fn resolve(&self, style: &Style, fallback: &Face) -> Face {
        match style {
            Style::Direct(face) => *face,
            Style::Token(token) => self.map.get(token).copied().unwrap_or(*fallback),
        }
    }

    /// Look up a token directly (without fallback).
    pub fn get(&self, token: &StyleToken) -> Option<&Face> {
        self.map.get(token)
    }

    /// Build a theme from the default plus config overrides.
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = Self::default_theme();
        for (name, face_spec) in &config.faces {
            if let (Some(token), Some(face)) = (token_from_name(name), parse_face_spec(face_spec)) {
                theme.set(token, face);
            }
        }
        theme
    }
}

/// Map config key names to StyleToken variants.
fn token_from_name(name: &str) -> Option<StyleToken> {
    match name {
        "buffer_text" => Some(StyleToken::BufferText),
        "buffer_padding" => Some(StyleToken::BufferPadding),
        "status_line" => Some(StyleToken::StatusLine),
        "status_mode" => Some(StyleToken::StatusMode),
        "menu_item_normal" => Some(StyleToken::MenuItemNormal),
        "menu_item_selected" => Some(StyleToken::MenuItemSelected),
        "menu_scrollbar" => Some(StyleToken::MenuScrollbar),
        "menu_scrollbar_thumb" => Some(StyleToken::MenuScrollbarThumb),
        "info_text" => Some(StyleToken::InfoText),
        "info_border" => Some(StyleToken::InfoBorder),
        "border" => Some(StyleToken::Border),
        "shadow" => Some(StyleToken::Shadow),
        _ => None,
    }
}

/// Parse a simple face spec like "red,blue+bi" into a Face.
/// Format: "fg,bg+attrs" where fg/bg are color names or "default",
/// and attrs is a combination of b(old), i(talic), u(nderline), r(everse), d(im).
pub(crate) fn parse_face_spec(spec: &str) -> Option<Face> {
    let (colors_part, attrs_part) = if let Some(pos) = spec.find('+') {
        (&spec[..pos], Some(&spec[pos + 1..]))
    } else {
        (spec, None)
    };

    let mut parts = colors_part.splitn(2, ',');
    let fg = parse_color_name(parts.next().unwrap_or("default").trim());
    let bg = parse_color_name(parts.next().unwrap_or("default").trim());

    let attributes = attrs_part
        .map(|a| {
            let mut attrs = Attributes::empty();
            for ch in a.chars() {
                match ch {
                    'b' => attrs |= Attributes::BOLD,
                    'i' => attrs |= Attributes::ITALIC,
                    'u' => attrs |= Attributes::UNDERLINE,
                    'r' => attrs |= Attributes::REVERSE,
                    'd' => attrs |= Attributes::DIM,
                    _ => {}
                }
            }
            attrs
        })
        .unwrap_or(Attributes::empty());

    Some(Face {
        fg,
        bg,
        underline: Color::Default,
        attributes,
    })
}

fn parse_color_name(name: &str) -> Color {
    match name {
        "default" | "" => Color::Default,
        "black" => Color::Named(NamedColor::Black),
        "red" => Color::Named(NamedColor::Red),
        "green" => Color::Named(NamedColor::Green),
        "yellow" => Color::Named(NamedColor::Yellow),
        "blue" => Color::Named(NamedColor::Blue),
        "magenta" => Color::Named(NamedColor::Magenta),
        "cyan" => Color::Named(NamedColor::Cyan),
        "white" => Color::Named(NamedColor::White),
        "bright-black" => Color::Named(NamedColor::BrightBlack),
        "bright-red" => Color::Named(NamedColor::BrightRed),
        "bright-green" => Color::Named(NamedColor::BrightGreen),
        "bright-yellow" => Color::Named(NamedColor::BrightYellow),
        "bright-blue" => Color::Named(NamedColor::BrightBlue),
        "bright-magenta" => Color::Named(NamedColor::BrightMagenta),
        "bright-cyan" => Color::Named(NamedColor::BrightCyan),
        "bright-white" => Color::Named(NamedColor::BrightWhite),
        s if s.starts_with("rgb:") => {
            let hex = &s[4..];
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
                Color::Rgb { r, g, b }
            } else {
                Color::Default
            }
        }
        _ => Color::Default,
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_theme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_direct() {
        let theme = Theme::new();
        let face = Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        let style = Style::Direct(face);
        let result = theme.resolve(&style, &Face::default());
        assert_eq!(result.fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn test_resolve_token_found() {
        let mut theme = Theme::new();
        let face = Face {
            fg: Color::Named(NamedColor::Green),
            ..Face::default()
        };
        theme.set(StyleToken::MenuItemNormal, face);
        let style = Style::Token(StyleToken::MenuItemNormal);
        let result = theme.resolve(&style, &Face::default());
        assert_eq!(result.fg, Color::Named(NamedColor::Green));
    }

    #[test]
    fn test_resolve_token_fallback() {
        let theme = Theme::new();
        let fallback = Face {
            fg: Color::Named(NamedColor::Yellow),
            ..Face::default()
        };
        let style = Style::Token(StyleToken::MenuItemNormal);
        let result = theme.resolve(&style, &fallback);
        assert_eq!(result.fg, Color::Named(NamedColor::Yellow));
    }

    #[test]
    fn test_default_theme_has_menu_faces() {
        let theme = Theme::default_theme();
        assert!(theme.get(&StyleToken::MenuItemNormal).is_some());
        assert!(theme.get(&StyleToken::MenuItemSelected).is_some());
        assert!(theme.get(&StyleToken::Shadow).is_some());
    }

    #[test]
    fn test_parse_face_spec() {
        let face = parse_face_spec("red,blue").unwrap();
        assert_eq!(face.fg, Color::Named(NamedColor::Red));
        assert_eq!(face.bg, Color::Named(NamedColor::Blue));

        let face = parse_face_spec("default,default+bi").unwrap();
        assert!(face.attributes.contains(Attributes::BOLD));
        assert!(face.attributes.contains(Attributes::ITALIC));

        let face = parse_face_spec("rgb:ff0000,default").unwrap();
        assert_eq!(face.fg, Color::Rgb { r: 255, g: 0, b: 0 });
    }

    #[test]
    fn test_theme_from_config() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("menu_item_normal".into(), "cyan,black".into());
        let theme = Theme::from_config(&config);
        let face = theme.get(&StyleToken::MenuItemNormal).unwrap();
        assert_eq!(face.fg, Color::Named(NamedColor::Cyan));
        assert_eq!(face.bg, Color::Named(NamedColor::Black));
    }
}
