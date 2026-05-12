use std::collections::HashMap;

use thiserror::Error;

use crate::config::{ThemeConfig, ThemeValue};
use crate::element::{ElementStyle, StyleToken};
use crate::protocol::Style as PStyle;
use crate::protocol::{Attributes, Color, NamedColor, WireFace};

/// Errors surfaced while building a [`Theme`] from a [`ThemeConfig`].
///
/// The runtime [`Theme::resolve`] path is infallible (missing tokens fall
/// back to the caller-supplied `fallback`); these errors describe
/// problems detected during construction that would otherwise resolve to
/// `Default` silently.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ThemeError {
    /// `@target` reference resolved to no defined token. The owning
    /// `token` ends up at its default style.
    #[error("theme token '{token}' references undefined '@{referenced}'")]
    UndefinedTokenReference { token: String, referenced: String },

    /// Reference cycle (e.g. A → B → A). The starting token ends up at
    /// its default style.
    #[error("theme token '{token}' is part of a reference cycle via '@{chain}'")]
    CircularReference { token: String, chain: String },

    /// Face spec did not parse cleanly. `reason` indicates the specific
    /// shape problem (currently: unknown colour name).
    #[error("theme token '{token}' has malformed face spec '{spec}': {reason}")]
    MalformedFaceSpec {
        token: String,
        spec: String,
        reason: String,
    },
}

/// Theme maps StyleTokens to styles for consistent visual styling.
///
/// Storage and the public API are both `Style`-native. Wire-format `WireFace`
/// values produced by [`parse_face_spec`] are converted to `Style` at the
/// theme boundary, so callers never see the legacy bitflag representation.
///
/// Construction via [`Theme::from_config`] may surface non-fatal issues
/// (undefined references, circular references, malformed face specs);
/// drain them with [`Theme::take_build_errors`] right after construction
/// to log or attribute them to the user's configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    map: HashMap<StyleToken, PStyle>,
    build_errors: Vec<ThemeError>,
}

impl Theme {
    /// Create an empty theme (all tokens resolve to fallback).
    pub fn new() -> Self {
        Theme {
            map: HashMap::new(),
            build_errors: Vec::new(),
        }
    }

    /// Drain the non-fatal errors collected during config-driven
    /// construction. The vector is replaced with an empty one, so a
    /// subsequent call returns no entries. Callers typically invoke this
    /// immediately after [`Theme::from_config`] / [`Theme::from_config_with_variant`]
    /// to log or surface the errors via the diagnostic overlay.
    pub fn take_build_errors(&mut self) -> Vec<ThemeError> {
        std::mem::take(&mut self.build_errors)
    }

    /// Create the default theme with Kakoune-compatible face values.
    pub fn default_theme() -> Self {
        let mut map = HashMap::new();

        // Menu: Default means "use protocol face from Kakoune".
        // User can override via config.toml [theme] section.
        map.insert(StyleToken::MENU_ITEM_NORMAL, PStyle::default());
        map.insert(StyleToken::MENU_ITEM_SELECTED, PStyle::default());
        map.insert(StyleToken::MENU_SCROLLBAR, PStyle::default());
        map.insert(StyleToken::MENU_SCROLLBAR_THUMB, PStyle::default());

        // Info / Status — semantically equivalent to default, kept explicit
        // so the user-configured detection (`is_user_configured`) reports
        // false for these tokens until config overrides them.
        map.insert(StyleToken::INFO_TEXT, PStyle::default());
        map.insert(StyleToken::INFO_BORDER, PStyle::default());
        map.insert(StyleToken::STATUS_LINE, PStyle::default());
        map.insert(StyleToken::STATUS_MODE, PStyle::default());

        // Workspace split divider
        map.insert(
            StyleToken::SPLIT_DIVIDER,
            (WireFace {
                fg: Color::Named(NamedColor::BrightBlack),
                bg: Color::Named(NamedColor::BrightBlack),
                ..WireFace::default()
            })
            .into(),
        );
        map.insert(
            StyleToken::SPLIT_DIVIDER_FOCUSED,
            (WireFace {
                fg: Color::Default,
                bg: Color::Named(NamedColor::BrightBlack),
                ..WireFace::default()
            })
            .into(),
        );

        // Gutter line numbers (TextPanel)
        map.insert(
            StyleToken::GUTTER_LINE_NUMBER,
            WireFace {
                fg: Color::Rgb {
                    r: 120,
                    g: 120,
                    b: 120,
                },
                ..WireFace::default()
            }
            .into(),
        );

        // TextPanel cursor highlight
        map.insert(
            StyleToken::TEXT_PANEL_CURSOR,
            WireFace {
                bg: Color::Rgb {
                    r: 40,
                    g: 40,
                    b: 60,
                },
                ..WireFace::default()
            }
            .into(),
        );

        // Shadow
        map.insert(
            StyleToken::SHADOW,
            (WireFace {
                fg: Color::Default,
                bg: Color::Default,
                underline: Color::Default,
                attributes: Attributes::DIM,
            })
            .into(),
        );

        Theme {
            map,
            build_errors: Vec::new(),
        }
    }

    /// Set a token's style.
    pub fn set_style(&mut self, token: StyleToken, style: PStyle) {
        self.map.insert(token, style);
    }

    /// Resolve an [`ElementStyle`] to a concrete [`Style`] using this theme.
    /// - `Inline(arc)` projects the inline style directly
    /// - `Token(token)` looks up the theme map; missing entries yield `fallback`
    pub fn resolve(&self, style: &ElementStyle, fallback: &PStyle) -> PStyle {
        match style {
            ElementStyle::Inline(arc) => arc.style.clone(),
            ElementStyle::Token(token) => self
                .map
                .get(token)
                .cloned()
                .unwrap_or_else(|| fallback.clone()),
        }
    }

    /// Look up a token's style directly (without fallback).
    pub fn get_style(&self, token: &StyleToken) -> Option<&PStyle> {
        self.map.get(token)
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
                    apply_face_spec(&mut theme, name, spec);
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
                        apply_face_spec(&mut theme, name, spec);
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
                WireFace {
                    fg: palette.dim_fg,
                    bg: Color::Default,
                    underline: Color::Default,
                    attributes: Attributes::DIM,
                }
                .into(),
            );
            self.set_if_still_default(
                StyleToken::SPLIT_DIVIDER,
                WireFace {
                    fg: palette.chrome_bg,
                    bg: palette.chrome_bg,
                    ..WireFace::default()
                }
                .into(),
            );
            self.set_if_still_default(
                StyleToken::SPLIT_DIVIDER_FOCUSED,
                WireFace {
                    fg: Color::Default,
                    bg: palette.chrome_bg,
                    ..WireFace::default()
                }
                .into(),
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

    /// Resolve a token with protocol fallback: if the user configured the token
    /// in the theme, use that; otherwise return `protocol_style`.
    pub fn resolve_with_protocol_fallback(
        &self,
        token: &StyleToken,
        protocol_style: PStyle,
    ) -> PStyle {
        if self.is_user_configured(token) {
            self.map.get(token).unwrap().clone()
        } else {
            protocol_style
        }
    }

    fn set_if_still_default(&mut self, token: StyleToken, derived: PStyle) {
        match self.map.get(&token) {
            Some(existing)
                if !matches!(existing.fg, crate::protocol::Brush::Default)
                    || !matches!(existing.bg, crate::protocol::Brush::Default) =>
            {
                // User explicitly configured -- don't override
            }
            _ => {
                self.map.insert(token, derived);
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

/// Parse a face spec and store it under `name`, surfacing any malformed
/// spec as a [`ThemeError::MalformedFaceSpec`] on the theme's
/// `build_errors`. The best-effort partial face is still applied so the
/// user sees as much of their configured colour as parses cleanly.
fn apply_face_spec(theme: &mut Theme, name: &str, spec: &str) {
    let token = normalize_config_key(name);
    match parse_face_spec_strict(spec) {
        Ok(face) => {
            theme.set_style(token, face.into());
        }
        Err((partial, reason)) => {
            theme.set_style(token, partial.into());
            theme.build_errors.push(ThemeError::MalformedFaceSpec {
                token: name.to_string(),
                spec: spec.to_string(),
                reason,
            });
        }
    }
}

/// Resolve `@token` references iteratively.
///
/// Follows reference chains (A→B→C) up to 10 iterations. Circular references
/// and undefined targets are reported via [`ThemeError`] on
/// `theme.build_errors`, and the originating token falls back to the
/// default style so the rest of the theme remains usable.
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
        let mut chain: Vec<String> = vec![target_name.clone()];

        for _ in 0..MAX_ITER {
            if visited.contains(&current_ref) {
                // Circular reference detected.
                theme.build_errors.push(ThemeError::CircularReference {
                    token: name.clone(),
                    chain: chain.join(" → @"),
                });
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
                    match parse_face_spec_strict(spec) {
                        Ok(face) => {
                            theme.set_style(token.clone(), face.into());
                        }
                        Err((partial, reason)) => {
                            theme.set_style(token.clone(), partial.into());
                            theme.build_errors.push(ThemeError::MalformedFaceSpec {
                                token: name.clone(),
                                spec: spec.clone(),
                                reason,
                            });
                        }
                    }
                    resolved = true;
                    break;
                }
                Some(ThemeValue::TokenRef(next_ref)) => {
                    chain.push(next_ref.clone());
                    current_ref = next_ref.clone();
                    // Continue chain resolution
                }
                None => {
                    // Not in config; check if it's already in theme map
                    let ref_token = normalize_config_key(&current_ref);
                    if let Some(style) = theme.get_style(&ref_token).cloned() {
                        theme.set_style(token.clone(), style);
                        resolved = true;
                    } else {
                        theme
                            .build_errors
                            .push(ThemeError::UndefinedTokenReference {
                                token: name.clone(),
                                referenced: current_ref.clone(),
                            });
                    }
                    break;
                }
            }
        }

        if !resolved {
            // Fallback to default style
            theme.set_style(token, PStyle::default());
        }
    }
}

/// Parse a simple face spec like "red,blue+bi" into a WireFace.
/// Format: "fg,bg+attrs" where fg/bg are color names or "default",
/// and attrs is a combination of b(old), i(talic), u(nderline), r(everse), d(im).
///
/// Lossy variant: unknown colour names silently degrade to `Color::Default`.
/// New code that needs to surface malformed specs should call
/// [`parse_face_spec_strict`] instead.
pub(crate) fn parse_face_spec(spec: &str) -> Option<WireFace> {
    Some(parse_face_spec_strict(spec).unwrap_or_else(|(face, _)| face))
}

/// Strict variant of [`parse_face_spec`] that returns the partial face
/// alongside a description of the first encountered problem (currently
/// always: unknown colour name). Used by [`Theme::from_config_with_variant`]
/// to surface malformed specs through [`ThemeError::MalformedFaceSpec`].
///
/// On error the returned `WireFace` still contains the best-effort parse
/// (unknown components fall back to `Color::Default`), so callers can
/// choose to either reject the spec entirely or accept the degraded face.
pub(crate) fn parse_face_spec_strict(spec: &str) -> Result<WireFace, (WireFace, String)> {
    let (colors_part, attrs_part) = if let Some(pos) = spec.find('+') {
        (&spec[..pos], Some(&spec[pos + 1..]))
    } else {
        (spec, None)
    };

    let mut parts = colors_part.splitn(2, ',');
    let fg_name = parts.next().unwrap_or("default").trim();
    let bg_name = parts.next().unwrap_or("default").trim();
    let (fg, fg_unknown) = parse_color_name_strict(fg_name);
    let (bg, bg_unknown) = parse_color_name_strict(bg_name);

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

    let face = WireFace {
        fg,
        bg,
        underline: Color::Default,
        attributes,
    };

    match (fg_unknown, bg_unknown) {
        (Some(name), _) => Err((face, format!("unknown foreground colour '{name}'"))),
        (None, Some(name)) => Err((face, format!("unknown background colour '{name}'"))),
        (None, None) => Ok(face),
    }
}

/// Parse a colour name, returning the colour and the original unknown
/// name (if the input did not match any known palette entry / RGB form
/// / explicit `default`).
fn parse_color_name_strict(name: &str) -> (Color, Option<String>) {
    let color = parse_color_name_inner(name);
    let unknown = matches!(color, Color::Default) && !name.is_empty() && name != "default";
    let unknown_name = if unknown {
        Some(name.to_string())
    } else {
        None
    };
    (color, unknown_name)
}

fn parse_color_name_inner(name: &str) -> Color {
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
    use crate::protocol::Brush;

    #[test]
    fn test_resolve_direct() {
        let theme = Theme::new();
        let face = WireFace {
            fg: Color::Named(NamedColor::Red),
            ..WireFace::default()
        };
        let style = ElementStyle::from(face);
        let result = theme.resolve(&style, &PStyle::default());
        assert_eq!(result.fg, Brush::Named(NamedColor::Red));
    }

    #[test]
    fn test_resolve_token_found() {
        let mut theme = Theme::new();
        let style = PStyle {
            fg: Brush::Named(NamedColor::Green),
            ..PStyle::default()
        };
        theme.set_style(StyleToken::MENU_ITEM_NORMAL, style);
        let element_style = ElementStyle::Token(StyleToken::MENU_ITEM_NORMAL);
        let result = theme.resolve(&element_style, &PStyle::default());
        assert_eq!(result.fg, Brush::Named(NamedColor::Green));
    }

    #[test]
    fn test_resolve_token_fallback() {
        let theme = Theme::new();
        let fallback = PStyle {
            fg: Brush::Named(NamedColor::Yellow),
            ..PStyle::default()
        };
        let element_style = ElementStyle::Token(StyleToken::MENU_ITEM_NORMAL);
        let result = theme.resolve(&element_style, &fallback);
        assert_eq!(result.fg, Brush::Named(NamedColor::Yellow));
    }

    #[test]
    fn test_default_theme_has_menu_faces() {
        let theme = Theme::default_theme();
        assert!(theme.get_style(&StyleToken::MENU_ITEM_NORMAL).is_some());
        assert!(theme.get_style(&StyleToken::MENU_ITEM_SELECTED).is_some());
        assert!(theme.get_style(&StyleToken::SHADOW).is_some());
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
        let style = theme.get_style(&StyleToken::MENU_ITEM_NORMAL).unwrap();
        assert_eq!(style.fg, Brush::Named(NamedColor::Cyan));
        assert_eq!(style.bg, Brush::Named(NamedColor::Black));
    }

    #[test]
    fn test_theme_merge() {
        let mut base = Theme::default_theme();
        let mut overlay = Theme::new();
        let custom = PStyle {
            fg: Brush::Named(NamedColor::Red),
            ..PStyle::default()
        };
        overlay.set_style(StyleToken::MENU_ITEM_NORMAL, custom.clone());
        overlay.set_style(StyleToken::new("myplugin.highlight"), custom);

        base.merge(&overlay);

        // Overlay wins for existing token
        assert_eq!(
            base.get_style(&StyleToken::MENU_ITEM_NORMAL).unwrap().fg,
            Brush::Named(NamedColor::Red)
        );
        // Custom token was added
        assert!(
            base.get_style(&StyleToken::new("myplugin.highlight"))
                .is_some()
        );
        // Unmodified token preserved
        assert!(base.get_style(&StyleToken::SHADOW).is_some());
    }

    #[test]
    fn test_custom_token_registration() {
        let mut theme = Theme::default_theme();
        let style = PStyle {
            fg: Brush::Named(NamedColor::Magenta),
            ..PStyle::default()
        };
        let token = StyleToken::new("color-preview.swatch");
        theme.set_style(token.clone(), style);
        let resolved = theme.get_style(&token).unwrap();
        assert_eq!(resolved.fg, Brush::Named(NamedColor::Magenta));
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
        assert!(theme.get_style(&token).is_some());
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
        let shadow = theme.get_style(&StyleToken::SHADOW).unwrap();
        assert_eq!(shadow.fg, Brush::rgb(150, 150, 150),);
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
        let shadow = theme.get_style(&StyleToken::SHADOW).unwrap();
        assert_eq!(shadow.fg, Brush::Named(NamedColor::Cyan));
    }

    #[test]
    fn test_apply_color_context_k1_noop() {
        use crate::render::color_context::{ColorContext, ColorKnowledge};
        let mut theme = Theme::default_theme();
        let original_shadow = theme.get_style(&StyleToken::SHADOW).cloned().unwrap();
        let ctx = ColorContext {
            is_dark: true,
            knowledge: ColorKnowledge::K1,
            chrome: None,
        };
        theme.apply_color_context(&ctx);
        assert_eq!(
            theme.get_style(&StyleToken::SHADOW).unwrap(),
            &original_shadow
        );
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
        let style = theme.get_style(&StyleToken::new("status.mode")).unwrap();
        assert_eq!(style.fg, Brush::Named(NamedColor::Green));
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
        let style = theme.get_style(&StyleToken::new("c")).unwrap();
        assert_eq!(style.fg, Brush::Named(NamedColor::Cyan));
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
        // Cycle → falls back to default style
        let style = theme.get_style(&StyleToken::new("a")).unwrap();
        assert_eq!(style.fg, Brush::Default);
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
            theme.get_style(&StyleToken::new("accent")).unwrap().fg,
            Brush::Named(NamedColor::Green)
        );

        // With dark variant
        let theme_dark = Theme::from_config_with_variant(&config, Some("dark"));
        assert_eq!(
            theme_dark.get_style(&StyleToken::new("accent")).unwrap().fg,
            Brush::Named(NamedColor::Cyan)
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
        let style = theme.get_style(&StyleToken::new("status.mode")).unwrap();
        assert_eq!(style.fg, Brush::Named(NamedColor::Cyan));
    }

    #[test]
    fn build_error_surfaces_undefined_reference() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("a".into(), ThemeValue::TokenRef("nonexistent".into()));
        let mut theme = Theme::from_config(&config);
        let errors = theme.take_build_errors();
        assert!(
            errors.iter().any(
                |e| matches!(e, ThemeError::UndefinedTokenReference { token, referenced }
                    if token == "a" && referenced == "nonexistent")
            ),
            "expected UndefinedTokenReference for token 'a', got {errors:?}"
        );
    }

    #[test]
    fn build_error_surfaces_circular_reference() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("a".into(), ThemeValue::TokenRef("b".into()));
        config
            .faces
            .insert("b".into(), ThemeValue::TokenRef("a".into()));
        let mut theme = Theme::from_config(&config);
        let errors = theme.take_build_errors();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ThemeError::CircularReference { .. })),
            "expected CircularReference error, got {errors:?}"
        );
    }

    #[test]
    fn build_error_surfaces_malformed_face_spec() {
        let mut config = ThemeConfig::default();
        config.faces.insert(
            "a".into(),
            ThemeValue::FaceSpec("plum,default".into()), // "plum" is not a known colour
        );
        let mut theme = Theme::from_config(&config);
        let errors = theme.take_build_errors();
        assert!(
            errors.iter().any(|e| matches!(
                e,
                ThemeError::MalformedFaceSpec { token, .. } if token == "a"
            )),
            "expected MalformedFaceSpec for token 'a', got {errors:?}"
        );
        // Partial parse still produces a usable face (fg falls back to Default).
        let style = theme.get_style(&StyleToken::new("a")).unwrap();
        assert_eq!(style.fg, Brush::Default);
        assert_eq!(style.bg, Brush::Default);
    }

    #[test]
    fn take_build_errors_drains_the_queue() {
        let mut config = ThemeConfig::default();
        config
            .faces
            .insert("a".into(), ThemeValue::FaceSpec("plum,default".into()));
        let mut theme = Theme::from_config(&config);
        assert!(!theme.take_build_errors().is_empty());
        assert!(
            theme.take_build_errors().is_empty(),
            "drain must be idempotent after the first call"
        );
    }
}
