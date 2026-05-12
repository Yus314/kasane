//! Shared widget evaluation helpers used by [`WidgetPlugin`](super::plugin::WidgetPlugin).
//!
//! Tests drive widgets through `PluginRuntime` via a thin shim in
//! `widget/tests.rs` that aggregates per-widget plugin registrations.

use crate::element::{Element, ElementStyle};
use crate::plugin::{AppView, PluginDiagnostic, PluginId};
use crate::protocol::{Atom, Style};

use super::parse::WidgetNodeError;
use super::types::{ContributionWidget, FaceOrToken, FaceRule};
use super::variables::VariableResolver;

const PLUGIN_ID: &str = "kasane.widgets";

/// Convert a `FaceOrToken` to a `Style` without resolving tokens against the theme.
pub(super) fn to_style(face_or_token: &FaceOrToken) -> ElementStyle {
    match face_or_token {
        FaceOrToken::Direct(face) => ElementStyle::from(*face),
        FaceOrToken::Token(token) => ElementStyle::Token(token.clone()),
    }
}

/// Resolve a `FaceOrToken` to a concrete `Style` using the current theme.
pub(super) fn resolve_face(face_or_token: &FaceOrToken, state: &AppView<'_>) -> Style {
    match face_or_token {
        FaceOrToken::Direct(face) => Style::from(*face),
        FaceOrToken::Token(token) => state.theme_style(token).cloned().unwrap_or_default(),
    }
}

/// Evaluate face rules and return the style for the first matching rule.
pub(super) fn resolve_face_rules(
    rules: &[FaceRule],
    resolver: &dyn VariableResolver,
    state: &AppView<'_>,
) -> Style {
    for rule in rules {
        if rule
            .when
            .as_ref()
            .is_none_or(|c| c.evaluate_with_resolver(resolver))
        {
            return resolve_face(&rule.face, state);
        }
    }
    Style::default()
}

/// Try to resolve face rules to a `Style` without eagerly resolving tokens.
///
/// Returns `Some(style)` if the rules have exactly one unconditional entry,
/// preserving `Style::Token` for deferred theme resolution. Returns `None`
/// if the rules require conditional evaluation (caller should fall back to
/// `resolve_face_rules`).
fn try_resolve_style(rules: &[FaceRule]) -> Option<ElementStyle> {
    match rules {
        [single] if single.when.is_none() => Some(to_style(&single.face)),
        _ => None,
    }
}

/// Build an Element from a contribution widget's parts.
pub(super) fn build_contribution_element(
    contrib: &ContributionWidget,
    resolver: &dyn VariableResolver,
    state: &AppView<'_>,
) -> Option<Element> {
    let mut atoms: Vec<Atom> = Vec::new();

    for part in &contrib.parts {
        // Check per-part when condition
        if let Some(ref cond) = part.when
            && !cond.evaluate_with_resolver(resolver)
        {
            continue;
        }

        let text = part.template.expand(resolver);
        let style = resolve_face_rules(&part.face_rules, resolver, state);
        atoms.push(Atom::with_style(text, style));
    }

    if atoms.is_empty() {
        return None;
    }

    // Single-atom optimization: if the sole part has an unconditional Token face,
    // emit Element::Text with Style::Token so the paint phase resolves it via the
    // theme. This avoids eagerly resolving the token here and allows theme changes
    // to take effect without re-evaluating the widget.
    if atoms.len() == 1 {
        let active_parts: Vec<_> = contrib
            .parts
            .iter()
            .filter(|p| {
                p.when
                    .as_ref()
                    .is_none_or(|c| c.evaluate_with_resolver(resolver))
            })
            .collect();
        if active_parts.len() == 1
            && let Some(style) = try_resolve_style(&active_parts[0].face_rules)
        {
            let atom = atoms.into_iter().next().unwrap();
            return Some(Element::Text(atom.contents, style));
        }
    }

    Some(Element::styled_line(atoms))
}

pub fn node_error_to_diagnostic(error: &WidgetNodeError) -> PluginDiagnostic {
    PluginDiagnostic::config_error(PluginId(PLUGIN_ID.to_string()), &error.name, &error.message)
}
