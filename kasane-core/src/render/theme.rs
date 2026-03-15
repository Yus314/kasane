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
            StyleToken::MENU_ITEM_NORMAL,
            Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::MENU_ITEM_SELECTED,
            Face {
                fg: Color::Named(NamedColor::Blue),
                bg: Color::Named(NamedColor::White),
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::MENU_SCROLLBAR,
            Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::MENU_SCROLLBAR_THUMB,
            Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
        );

        // Info
        map.insert(
            StyleToken::INFO_TEXT,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::INFO_BORDER,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );

        // Status
        map.insert(
            StyleToken::STATUS_LINE,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );
        map.insert(
            StyleToken::STATUS_MODE,
            Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            },
        );

        // Workspace split divider
        map.insert(
            StyleToken::SPLIT_DIVIDER,
            Face {
                fg: Color::Default,
                bg: Color::Named(NamedColor::BrightBlack),
                ..Face::default()
            },
        );

        // Shadow
        map.insert(
            StyleToken::SHADOW,
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

    /// Merge another theme's entries into this one (overlay wins on conflict).
    pub fn merge(&mut self, overlay: &Theme) {
        for (token, face) in &overlay.map {
            self.map.insert(token.clone(), *face);
        }
    }

    /// Build a theme from the default plus config overrides.
    ///
    /// Config keys use underscore notation (e.g., "menu_item_normal") which is
    /// normalized to dot notation (e.g., "menu.item.normal") for StyleToken lookup.
    /// Unknown keys are accepted as custom plugin tokens.
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = Self::default_theme();
        for (name, face_spec) in &config.faces {
            if let Some(face) = parse_face_spec(face_spec) {
                let token = normalize_config_key(name);
                theme.set(token, face);
            }
        }
        theme
    }
}

/// Normalize a config key to a StyleToken.
///
/// Underscore-separated config names (e.g., "menu_item_normal") are converted
/// to dot notation (e.g., "menu.item.normal") to match the canonical token names.
fn normalize_config_key(name: &str) -> StyleToken {
    StyleToken::new(name.replace('_', "."))
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
        theme.set(StyleToken::MENU_ITEM_NORMAL, face);
        let style = Style::Token(StyleToken::MENU_ITEM_NORMAL);
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
        let style = Style::Token(StyleToken::MENU_ITEM_NORMAL);
        let result = theme.resolve(&style, &fallback);
        assert_eq!(result.fg, Color::Named(NamedColor::Yellow));
    }

    #[test]
    fn test_default_theme_has_menu_faces() {
        let theme = Theme::default_theme();
        assert!(theme.get(&StyleToken::MENU_ITEM_NORMAL).is_some());
        assert!(theme.get(&StyleToken::MENU_ITEM_SELECTED).is_some());
        assert!(theme.get(&StyleToken::SHADOW).is_some());
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
        let face = theme.get(&StyleToken::MENU_ITEM_NORMAL).unwrap();
        assert_eq!(face.fg, Color::Named(NamedColor::Cyan));
        assert_eq!(face.bg, Color::Named(NamedColor::Black));
    }

    #[test]
    fn test_theme_merge() {
        let mut base = Theme::default_theme();
        let mut overlay = Theme::new();
        let custom_face = Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        overlay.set(StyleToken::MENU_ITEM_NORMAL, custom_face);
        overlay.set(StyleToken::new("myplugin.highlight"), custom_face);

        base.merge(&overlay);

        // Overlay wins for existing token
        assert_eq!(
            base.get(&StyleToken::MENU_ITEM_NORMAL).unwrap().fg,
            Color::Named(NamedColor::Red)
        );
        // Custom token was added
        assert!(base.get(&StyleToken::new("myplugin.highlight")).is_some());
        // Unmodified token preserved
        assert!(base.get(&StyleToken::SHADOW).is_some());
    }

    #[test]
    fn test_custom_token_registration() {
        let mut theme = Theme::default_theme();
        let face = Face {
            fg: Color::Named(NamedColor::Magenta),
            ..Face::default()
        };
        let token = StyleToken::new("color-preview.swatch");
        theme.set(token.clone(), face);
        let resolved = theme.get(&token).unwrap();
        assert_eq!(resolved.fg, Color::Named(NamedColor::Magenta));
    }

    #[test]
    fn test_config_unknown_key_accepted() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("my_plugin_highlight".into(), "green,default".into());
        let theme = Theme::from_config(&config);
        // Unknown keys are normalized and stored
        let token = StyleToken::new("my.plugin.highlight");
        assert!(theme.get(&token).is_some());
    }
}
