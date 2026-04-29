use std::collections::HashMap;

use crate::config::{ThemeConfig, ThemeValue};
use crate::element::{ElementStyle, StyleToken};
use crate::protocol::Style as PStyle;
use crate::protocol::{Attributes, Color, Face, NamedColor};

/// Theme maps StyleTokens to styles for consistent visual styling.
///
/// ADR-031 Phase A.3.2: storage migrated from `Face` to `Style`. The
/// public API still exposes `Face`-shaped methods (`set`, `get`,
/// `resolve`, `resolve_with_protocol_fallback`) for callers that have
/// not yet migrated; new methods (`set_style`, `get_style`,
/// `resolve_style`) return the underlying `Style` directly so the
/// projection cost can be paid lazily where needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    map: HashMap<StyleToken, PStyle>,
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

        // Menu: Default means "use protocol face from Kakoune".
        // User can override via config.toml [theme] section.
        map.insert(StyleToken::MENU_ITEM_NORMAL, (Face::default()).into());
        map.insert(StyleToken::MENU_ITEM_SELECTED, (Face::default()).into());
        map.insert(StyleToken::MENU_SCROLLBAR, (Face::default()).into());
        map.insert(StyleToken::MENU_SCROLLBAR_THUMB, (Face::default()).into());

        // Info
        map.insert(
            StyleToken::INFO_TEXT,
            (Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            })
            .into(),
        );
        map.insert(
            StyleToken::INFO_BORDER,
            (Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            })
            .into(),
        );

        // Status
        map.insert(
            StyleToken::STATUS_LINE,
            (Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            })
            .into(),
        );
        map.insert(
            StyleToken::STATUS_MODE,
            (Face {
                fg: Color::Default,
                bg: Color::Default,
                ..Face::default()
            })
            .into(),
        );

        // Workspace split divider
        map.insert(
            StyleToken::SPLIT_DIVIDER,
            (Face {
                fg: Color::Named(NamedColor::BrightBlack),
                bg: Color::Named(NamedColor::BrightBlack),
                ..Face::default()
            })
            .into(),
        );
        map.insert(
            StyleToken::SPLIT_DIVIDER_FOCUSED,
            (Face {
                fg: Color::Default,
                bg: Color::Named(NamedColor::BrightBlack),
                ..Face::default()
            })
            .into(),
        );

        // Gutter line numbers (TextPanel)
        map.insert(
            StyleToken::GUTTER_LINE_NUMBER,
            Face {
                fg: Color::Rgb {
                    r: 120,
                    g: 120,
                    b: 120,
                },
                ..Face::default()
            }
            .into(),
        );

        // TextPanel cursor highlight
        map.insert(
            StyleToken::TEXT_PANEL_CURSOR,
            Face {
                bg: Color::Rgb {
                    r: 40,
                    g: 40,
                    b: 60,
                },
                ..Face::default()
            }
            .into(),
        );

        // Shadow
        map.insert(
            StyleToken::SHADOW,
            (Face {
                fg: Color::Default,
                bg: Color::Default,
                underline: Color::Default,
                attributes: Attributes::DIM,
            })
            .into(),
        );

        Theme { map }
    }

    /// Set a token's face. Internally stores as Style (Face → Style via `From`).
    pub fn set(&mut self, token: StyleToken, face: Face) {
        self.map.insert(token, face.into());
    }

    /// Set a token's style directly.
    pub fn set_style(&mut self, token: StyleToken, style: PStyle) {
        self.map.insert(token, style);
    }

    /// Resolve a Style to a Face.
    /// - Inline(unresolved) → projects via `to_face()`
    /// - Token(token) → looks up in theme map, falls back to `fallback`
    pub fn resolve(&self, style: &ElementStyle, fallback: &Face) -> Face {
        match style {
            ElementStyle::Inline(arc) => arc.to_face(),
            ElementStyle::Token(token) => self
                .map
                .get(token)
                .map(|s| s.to_face())
                .unwrap_or(*fallback),
        }
    }

    /// Look up a token's style directly (without fallback). Phase A.3 API.
    pub fn get_style(&self, token: &StyleToken) -> Option<&PStyle> {
        self.map.get(token)
    }

    /// Look up a token directly (without fallback). Returns Face for
    /// back-compat; consumers should migrate to [`Self::get_style`].
    pub fn get(&self, token: &StyleToken) -> Option<Face> {
        self.map.get(token).map(|s| s.to_face())
    }

    /// Merge another theme's entries into this one (overlay wins on conflict).
    pub fn merge(&mut self, overlay: &Theme) {
        for (token, style) in &overlay.map {
            self.map.insert(token.clone(), style.clone());
        }
    }

    /// Build a theme from the default plus config overrides.
    ///
    /// Config keys use underscore notation (e.g., "menu_item_normal") which is
    /// normalized to dot notation (e.g., "menu.item.normal") for StyleToken lookup.
    /// Unknown keys are accepted as custom plugin tokens.
    ///
    /// If `variant` is `Some`, the named variant overlay is applied after base
    /// faces, and then token references are re-resolved.
    pub fn from_config(config: &ThemeConfig) -> Self {
        Self::from_config_with_variant(config, None)
    }

    /// Build theme with optional variant selection.
    pub fn from_config_with_variant(config: &ThemeConfig, variant: Option<&str>) -> Self {
        let mut theme = Self::default_theme();

        // Merge base faces: direct specs are resolved immediately, refs are deferred
        let mut pending_refs: Vec<(String, String)> = Vec::new();

        for (name, value) in &config.faces {
            match value {
                ThemeValue::FaceSpec(spec) => {
                    if let Some(face) = parse_face_spec(spec) {
                        let token = normalize_config_key(name);
                        theme.set(token, face);
                    }
                }
                ThemeValue::TokenRef(ref_name) => {
                    pending_refs.push((name.clone(), ref_name.clone()));
                }
            }
        }

        // Apply variant overlay if specified
        if let Some(variant_name) = variant
            && let Some(variant_faces) = config.variants.get(variant_name)
        {
            for (name, value) in variant_faces {
                match value {
                    ThemeValue::FaceSpec(spec) => {
                        if let Some(face) = parse_face_spec(spec) {
                            let token = normalize_config_key(name);
                            theme.set(token, face);
                        }
                        // Remove any pending ref for this key (variant overrides base)
                        pending_refs.retain(|(n, _)| n != name);
                    }
                    ThemeValue::TokenRef(ref_name) => {
                        // Replace or add pending ref
                        if let Some(existing) = pending_refs.iter_mut().find(|(n, _)| n == name) {
                            existing.1 = ref_name.clone();
                        } else {
                            pending_refs.push((name.clone(), ref_name.clone()));
                        }
                    }
                }
            }
        }

        // Resolve token references iteratively (max 10 iterations for chains)
        resolve_token_refs(&mut theme, &pending_refs, config, variant);

        theme
    }

    /// Apply derived color context to the theme.
    ///
    /// Overwrites tokens that are still at their default (Color::Default) values
    /// with derived colors from the color context. User-specified config values
    /// are preserved.
    pub fn apply_color_context(&mut self, ctx: &crate::render::color_context::ColorContext) {
        if let Some(ref palette) = ctx.chrome {
            self.set_if_still_default(
                StyleToken::SHADOW,
                Face {
                    fg: palette.dim_fg,
                    bg: Color::Default,
                    underline: Color::Default,
                    attributes: Attributes::DIM,
                },
            );
            self.set_if_still_default(
                StyleToken::SPLIT_DIVIDER,
                Face {
                    fg: palette.chrome_bg,
                    bg: palette.chrome_bg,
                    ..Face::default()
                },
            );
            self.set_if_still_default(
                StyleToken::SPLIT_DIVIDER_FOCUSED,
                Face {
                    fg: Color::Default,
                    bg: palette.chrome_bg,
                    ..Face::default()
                },
            );
        }
    }

    /// Check if a token has been configured by the user (non-default colors).
    ///
    /// Returns `true` when the token exists in the theme map AND has at least
    /// one non-Default brush, indicating the user explicitly set it via config.
    pub fn is_user_configured(&self, token: &StyleToken) -> bool {
        self.map.get(token).is_some_and(|s| {
            !matches!(s.fg, crate::protocol::Brush::Default)
                || !matches!(s.bg, crate::protocol::Brush::Default)
        })
    }

    /// Resolve a face with protocol fallback: if the user configured the token
    /// in the theme, use that; otherwise use the protocol-provided face.
    pub fn resolve_with_protocol_fallback(&self, token: &StyleToken, protocol_face: Face) -> Face {
        if self.is_user_configured(token) {
            self.map.get(token).unwrap().to_face()
        } else {
            protocol_face
        }
    }

    fn set_if_still_default(&mut self, token: StyleToken, derived: Face) {
        let derived_style: PStyle = derived.into();
        match self.map.get(&token) {
            Some(existing)
                if !matches!(existing.fg, crate::protocol::Brush::Default)
                    || !matches!(existing.bg, crate::protocol::Brush::Default) =>
            {
                // User explicitly configured -- don't override
            }
            _ => {
                self.map.insert(token, derived_style);
            }
        }
    }
}

/// Normalize a config key to a StyleToken.
///
/// Underscore-separated config names (e.g., "menu_item_normal") are converted
/// to dot notation (e.g., "menu.item.normal") to match the canonical token names.
fn normalize_config_key(name: &str) -> StyleToken {
    StyleToken::new(name.replace('_', "."))
}

/// Resolve `@token` references iteratively.
///
/// Follows reference chains (A→B→C) up to 10 iterations.
/// Circular references are detected and fall back to default face.
fn resolve_token_refs(
    theme: &mut Theme,
    pending: &[(String, String)],
    config: &ThemeConfig,
    variant: Option<&str>,
) {
    use std::collections::HashSet;
    const MAX_ITER: usize = 10;

    for (name, target_name) in pending {
        let token = normalize_config_key(name);
        let mut current_ref = target_name.clone();
        let mut visited = HashSet::new();
        visited.insert(name.clone());
        let mut resolved = false;

        for _ in 0..MAX_ITER {
            if visited.contains(&current_ref) {
                // Circular reference detected
                tracing::warn!(
                    "theme: circular reference detected for token '{name}' → '@{current_ref}'"
                );
                break;
            }
            visited.insert(current_ref.clone());

            // Look up the target: first check variant, then base config, then existing theme
            let target_value = variant
                .and_then(|v| config.variants.get(v))
                .and_then(|vf| vf.get(&current_ref))
                .or_else(|| config.faces.get(&current_ref));

            match target_value {
                Some(ThemeValue::FaceSpec(spec)) => {
                    if let Some(face) = parse_face_spec(spec) {
                        theme.set(token.clone(), face);
                        resolved = true;
                    }
                    break;
                }
                Some(ThemeValue::TokenRef(next_ref)) => {
                    current_ref = next_ref.clone();
                    // Continue chain resolution
                }
                None => {
                    // Not in config; check if it's already in theme map
                    let ref_token = normalize_config_key(&current_ref);
                    if let Some(face) = theme.get(&ref_token) {
                        theme.set(token.clone(), face);
                        resolved = true;
                    }
                    break;
                }
            }
        }

        if !resolved {
            // Fallback to default face
            theme.set(token, Face::default());
        }
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
        let style = ElementStyle::from(face);
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
        let style = ElementStyle::Token(StyleToken::MENU_ITEM_NORMAL);
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
        let style = ElementStyle::Token(StyleToken::MENU_ITEM_NORMAL);
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
        config.faces.insert(
            "menu_item_normal".into(),
            ThemeValue::FaceSpec("cyan,black".into()),
        );
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
        config.faces.insert(
            "my_plugin_highlight".into(),
            ThemeValue::FaceSpec("green,default".into()),
        );
        let theme = Theme::from_config(&config);
        // Unknown keys are normalized and stored
        let token = StyleToken::new("my.plugin.highlight");
        assert!(theme.get(&token).is_some());
    }

    #[test]
    fn test_apply_color_context_k3() {
        use crate::render::color_context::{ChromePalette, ColorContext, ColorKnowledge};
        let mut theme = Theme::default_theme();
        let ctx = ColorContext {
            is_dark: true,
            knowledge: ColorKnowledge::K3,
            chrome: Some(ChromePalette {
                chrome_bg: Color::Rgb {
                    r: 50,
                    g: 50,
                    b: 50,
                },
                dim_fg: Color::Rgb {
                    r: 150,
                    g: 150,
                    b: 150,
                },
            }),
        };
        theme.apply_color_context(&ctx);
        let shadow = theme.get(&StyleToken::SHADOW).unwrap();
        assert_eq!(
            shadow.fg,
            Color::Rgb {
                r: 150,
                g: 150,
                b: 150
            }
        );
    }

    #[test]
    fn test_apply_color_context_preserves_user_config() {
        use crate::render::color_context::{ChromePalette, ColorContext, ColorKnowledge};
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("shadow".into(), ThemeValue::FaceSpec("cyan,default".into()));
        let mut theme = Theme::from_config(&config);

        let ctx = ColorContext {
            is_dark: true,
            knowledge: ColorKnowledge::K3,
            chrome: Some(ChromePalette {
                chrome_bg: Color::Rgb {
                    r: 50,
                    g: 50,
                    b: 50,
                },
                dim_fg: Color::Rgb {
                    r: 150,
                    g: 150,
                    b: 150,
                },
            }),
        };
        theme.apply_color_context(&ctx);
        // User's cyan should be preserved
        let shadow = theme.get(&StyleToken::SHADOW).unwrap();
        assert_eq!(shadow.fg, Color::Named(NamedColor::Cyan));
    }

    #[test]
    fn test_apply_color_context_k1_noop() {
        use crate::render::color_context::{ColorContext, ColorKnowledge};
        let mut theme = Theme::default_theme();
        let original_shadow = theme.get(&StyleToken::SHADOW).unwrap();
        let ctx = ColorContext {
            is_dark: true,
            knowledge: ColorKnowledge::K1,
            chrome: None,
        };
        theme.apply_color_context(&ctx);
        assert_eq!(theme.get(&StyleToken::SHADOW).unwrap(), original_shadow);
    }

    // ── Token reference tests ────────────────────────────────────────────

    #[test]
    fn test_theme_token_ref_simple() {
        let mut config = ThemeConfig::default();
        config.faces.insert(
            "accent".into(),
            ThemeValue::FaceSpec("green,default".into()),
        );
        config
            .faces
            .insert("status_mode".into(), ThemeValue::TokenRef("accent".into()));
        let theme = Theme::from_config(&config);
        let face = theme.get(&StyleToken::new("status.mode")).unwrap();
        assert_eq!(face.fg, Color::Named(NamedColor::Green));
    }

    #[test]
    fn test_theme_token_ref_chain() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("a".into(), ThemeValue::FaceSpec("cyan,default".into()));
        config
            .faces
            .insert("b".into(), ThemeValue::TokenRef("a".into()));
        config
            .faces
            .insert("c".into(), ThemeValue::TokenRef("b".into()));
        let theme = Theme::from_config(&config);
        let face = theme.get(&StyleToken::new("c")).unwrap();
        assert_eq!(face.fg, Color::Named(NamedColor::Cyan));
    }

    #[test]
    fn test_theme_token_ref_cycle_falls_back() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("a".into(), ThemeValue::TokenRef("b".into()));
        config
            .faces
            .insert("b".into(), ThemeValue::TokenRef("a".into()));
        let theme = Theme::from_config(&config);
        // Cycle → falls back to default face
        let face = theme.get(&StyleToken::new("a")).unwrap();
        assert_eq!(face.fg, Color::Default);
    }

    #[test]
    fn test_theme_variant_overlay() {
        let mut config = ThemeConfig::default();
        config.faces.insert(
            "accent".into(),
            ThemeValue::FaceSpec("green,default".into()),
        );
        let mut dark_variant = std::collections::HashMap::new();
        dark_variant.insert("accent".into(), ThemeValue::FaceSpec("cyan,default".into()));
        config.variants.insert("dark".into(), dark_variant);

        // Without variant
        let theme = Theme::from_config(&config);
        assert_eq!(
            theme.get(&StyleToken::new("accent")).unwrap().fg,
            Color::Named(NamedColor::Green)
        );

        // With dark variant
        let theme_dark = Theme::from_config_with_variant(&config, Some("dark"));
        assert_eq!(
            theme_dark.get(&StyleToken::new("accent")).unwrap().fg,
            Color::Named(NamedColor::Cyan)
        );
    }

    #[test]
    fn test_theme_variant_ref_resolves_with_variant_override() {
        let mut config = ThemeConfig::default();
        config.faces.insert(
            "accent".into(),
            ThemeValue::FaceSpec("green,default".into()),
        );
        config
            .faces
            .insert("status_mode".into(), ThemeValue::TokenRef("accent".into()));
        let mut dark = std::collections::HashMap::new();
        dark.insert("accent".into(), ThemeValue::FaceSpec("cyan,default".into()));
        config.variants.insert("dark".into(), dark);

        let theme = Theme::from_config_with_variant(&config, Some("dark"));
        // status_mode → @accent → "cyan,default" (from dark variant)
        let face = theme.get(&StyleToken::new("status.mode")).unwrap();
        assert_eq!(face.fg, Color::Named(NamedColor::Cyan));
    }
}
