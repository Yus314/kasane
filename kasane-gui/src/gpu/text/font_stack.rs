//! `FontConfig` → Parley font family conversion.
//!
//! Bridges Kasane's user-facing [`FontConfig`] to the
//! [`parley::FontFamily`] container that
//! `RangedBuilder::push_default(StyleProperty::FontFamily(...))`
//! consumes. [`super::ParleyText::new`] caches the resolved stack so
//! the production hot path does not re-build it per shape call.
//!
//! Naming convention: Kakoune (and CSS) use the generic family names
//! `"monospace"`, `"serif"`, `"sans-serif"`, `"cursive"`, `"fantasy"`. Parley
//! understands these names directly through [`parley::GenericFamily`], so the
//! conversion is a string match for the generics and a wrapped name for
//! everything else.
//!
//! Fallback handling: `FontConfig.fallback_list` is concatenated after the
//! primary family. Fontique resolves the chain in order at shape time, so
//! emoji / CJK / niche-script fonts can be supplied as fallbacks without any
//! Kasane-side scoring.

use std::borrow::Cow;

use kasane_core::config::FontConfig;
use parley::{FontFamily, FontFamilyName, GenericFamily};

/// Build a Parley [`FontFamily`] list from the user's [`FontConfig`].
///
/// The returned value is owned (`'static` lifetime via `Cow::Owned`), so it
/// can be cached on `ParleyText` without borrowing from `FontConfig`.
pub fn resolve_stack(font_config: &FontConfig) -> FontFamily<'static> {
    let mut families: Vec<FontFamilyName<'static>> =
        Vec::with_capacity(1 + font_config.fallback_list.len());
    families.push(family_from_name(&font_config.family));
    for fallback in &font_config.fallback_list {
        families.push(family_from_name(fallback));
    }
    if families.len() == 1 {
        FontFamily::Single(families.pop().unwrap())
    } else {
        FontFamily::List(Cow::Owned(families))
    }
}

/// Convert a single CSS-style family name to a [`FontFamilyName`].
///
/// Generic family keywords are mapped to their Parley equivalents; everything
/// else is wrapped as a `FontFamilyName::Named` with an owned string copy.
pub fn family_from_name(name: &str) -> FontFamilyName<'static> {
    if let Some(g) = generic_from_str(name) {
        FontFamilyName::Generic(g)
    } else {
        FontFamilyName::Named(Cow::Owned(name.to_string()))
    }
}

fn generic_from_str(name: &str) -> Option<GenericFamily> {
    Some(match name.to_ascii_lowercase().as_str() {
        "monospace" => GenericFamily::Monospace,
        "serif" => GenericFamily::Serif,
        "sans-serif" | "sans serif" => GenericFamily::SansSerif,
        "cursive" => GenericFamily::Cursive,
        "fantasy" => GenericFamily::Fantasy,
        "system-ui" | "system ui" => GenericFamily::SystemUi,
        "ui-monospace" | "ui monospace" => GenericFamily::UiMonospace,
        "ui-serif" => GenericFamily::UiSerif,
        "ui-sans-serif" => GenericFamily::UiSansSerif,
        "emoji" => GenericFamily::Emoji,
        "math" => GenericFamily::Math,
        "fangsong" => GenericFamily::FangSong,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monospace_resolves_to_generic() {
        match family_from_name("monospace") {
            FontFamilyName::Generic(GenericFamily::Monospace) => {}
            other => panic!("expected Generic(Monospace), got {other:?}"),
        }
    }

    #[test]
    fn arbitrary_name_becomes_named() {
        match family_from_name("Fira Code") {
            FontFamilyName::Named(name) => assert_eq!(name.as_ref(), "Fira Code"),
            other => panic!("expected Named, got {other:?}"),
        }
    }

    #[test]
    fn case_insensitive_generic() {
        // CSS generic family keywords are case-insensitive.
        match family_from_name("Monospace") {
            FontFamilyName::Generic(GenericFamily::Monospace) => {}
            other => panic!("expected Generic(Monospace), got {other:?}"),
        }
        match family_from_name("SANS-SERIF") {
            FontFamilyName::Generic(GenericFamily::SansSerif) => {}
            other => panic!("expected Generic(SansSerif), got {other:?}"),
        }
    }

    #[test]
    fn font_config_default_to_single() {
        // Default FontConfig has no fallbacks → Single (not List).
        let cfg = FontConfig::default();
        let stack = resolve_stack(&cfg);
        match stack {
            FontFamily::Single(FontFamilyName::Generic(GenericFamily::Monospace)) => {}
            other => panic!("expected Single(Monospace), got {other:?}"),
        }
    }

    #[test]
    fn font_config_with_fallbacks_to_list() {
        let cfg = FontConfig {
            family: "JetBrains Mono".into(),
            fallback_list: vec!["Noto Color Emoji".into(), "monospace".into()],
            ..FontConfig::default()
        };
        let stack = resolve_stack(&cfg);
        match stack {
            FontFamily::List(list) => {
                assert_eq!(list.len(), 3);
                match &list[0] {
                    FontFamilyName::Named(name) => assert_eq!(name.as_ref(), "JetBrains Mono"),
                    other => panic!("expected primary Named, got {other:?}"),
                }
                match &list[1] {
                    FontFamilyName::Named(name) => assert_eq!(name.as_ref(), "Noto Color Emoji"),
                    other => panic!("expected fallback Named, got {other:?}"),
                }
                match &list[2] {
                    FontFamilyName::Generic(GenericFamily::Monospace) => {}
                    other => panic!("expected fallback Generic(Monospace), got {other:?}"),
                }
            }
            other => panic!("expected List, got {other:?}"),
        }
    }
}
