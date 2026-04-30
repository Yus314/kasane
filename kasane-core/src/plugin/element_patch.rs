//! Declarative element transform algebra.
//!
//! `ElementPatch` represents a declarative, composable transform that can be
//! applied to a [`TransformSubject`]. Unlike the imperative `fn transform()` approach,
//! patches form a monoid under `Compose` and can be inspected, normalized, and
//! (when [`is_pure()`](ElementPatch::is_pure)) memoized by Salsa.
//!
//! # Monoid Laws
//!
//! `ElementPatch` satisfies the monoid laws via [`Composable`](super::Composable):
//! - **Identity**: `Identity` (no-op)
//! - **Composition**: `Compose(vec![a, b])` applies `a` then `b`
//! - **Associativity**: guaranteed by sequential application
//!
//! # Algebraic Properties
//!
//! - `Replace` absorbs all prior patches (annihilator from the left)
//! - `Identity` is eliminated during normalization
//! - Nested `Compose` is flattened

use std::sync::Arc;

use crate::element::{Align, Direction, Element, ElementStyle, FlexChild, OverlayAnchor};
use crate::protocol::WireFace;
use crate::widget::predicate::Predicate;

use super::context::{FaceMerge, TransformContext, TransformScope, TransformSubject};

/// Backward-compatible alias for the unified `Predicate` type.
pub type PatchPredicate = Predicate;

impl Predicate {
    /// Evaluate this predicate against a transform context (pane-context only).
    ///
    /// Variable-based predicates (`VariableTruthy`, `VariableCompare`) always
    /// return `false` when evaluated without a resolver.
    pub fn evaluate_in_transform_context(&self, ctx: &TransformContext) -> bool {
        use crate::widget::variables::NullResolver;
        let pred_ctx = crate::widget::predicate::PredicateContext {
            resolver: &NullResolver,
            pane_focused: ctx.pane_focused,
            pane_surface_id: ctx.pane_surface_id,
            target_line: ctx.target_line,
        };
        self.evaluate(&pred_ctx)
    }
}

/// A declarative transform on a [`TransformSubject`].
///
/// Variants represent specific structural or stylistic modifications.
/// Forms a monoid with `Identity` as the identity element and
/// sequential composition via `Compose`.
pub enum ElementPatch {
    /// No-op transform. Identity element of the composition monoid.
    Identity,

    /// Wrap the subject element in a container with the given inline style.
    WrapContainer {
        style: Arc<crate::protocol::UnresolvedStyle>,
    },

    /// Prepend an element before the subject.
    Prepend { element: Element },

    /// Append an element after the subject.
    Append { element: Element },

    /// Replace the subject entirely. Absorbs all prior patches in a composition.
    Replace { element: Element },

    /// Overlay style attributes onto the subject element.
    ModifyStyle {
        overlay: Arc<crate::protocol::UnresolvedStyle>,
    },

    /// Sequence of patches applied left-to-right.
    Compose(Vec<ElementPatch>),

    /// Modify the overlay anchor (no-op for non-overlay subjects).
    ModifyAnchor {
        transform: Arc<dyn Fn(OverlayAnchor) -> OverlayAnchor + Send + Sync>,
    },

    /// Conditional patch: evaluates a pure predicate and applies one of two branches.
    ///
    /// Unlike `Custom`, predicates are data and can be memoized by Salsa.
    When {
        predicate: PatchPredicate,
        then: Box<ElementPatch>,
        otherwise: Box<ElementPatch>,
    },

    /// Escape hatch: arbitrary opaque transform function.
    /// Blocks Salsa memoization ([`is_pure()`](Self::is_pure) returns `false`).
    Custom(Arc<dyn Fn(TransformSubject) -> TransformSubject + Send + Sync>),
}

impl Clone for ElementPatch {
    fn clone(&self) -> Self {
        match self {
            Self::Identity => Self::Identity,
            Self::WrapContainer { style } => Self::WrapContainer {
                style: Arc::clone(style),
            },
            Self::Prepend { element } => Self::Prepend {
                element: element.clone(),
            },
            Self::Append { element } => Self::Append {
                element: element.clone(),
            },
            Self::Replace { element } => Self::Replace {
                element: element.clone(),
            },
            Self::ModifyStyle { overlay } => Self::ModifyStyle {
                overlay: Arc::clone(overlay),
            },
            Self::Compose(patches) => Self::Compose(patches.clone()),
            Self::ModifyAnchor { transform } => Self::ModifyAnchor {
                transform: Arc::clone(transform),
            },
            Self::When {
                predicate,
                then,
                otherwise,
            } => Self::When {
                predicate: predicate.clone(),
                then: then.clone(),
                otherwise: otherwise.clone(),
            },
            Self::Custom(f) => Self::Custom(Arc::clone(f)),
        }
    }
}

impl PartialEq for ElementPatch {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Identity, Self::Identity) => true,
            (Self::WrapContainer { style: a }, Self::WrapContainer { style: b }) => {
                a.as_ref() == b.as_ref()
            }
            (Self::Prepend { element: a }, Self::Prepend { element: b }) => a == b,
            (Self::Append { element: a }, Self::Append { element: b }) => a == b,
            (Self::Replace { element: a }, Self::Replace { element: b }) => a == b,
            (Self::ModifyStyle { overlay: a }, Self::ModifyStyle { overlay: b }) => {
                a.as_ref() == b.as_ref()
            }
            (Self::Compose(a), Self::Compose(b)) => a == b,
            (
                Self::When {
                    predicate: pa,
                    then: ta,
                    otherwise: oa,
                },
                Self::When {
                    predicate: pb,
                    then: tb,
                    otherwise: ob,
                },
            ) => pa == pb && ta == tb && oa == ob,
            // Impure variants: always unequal (opaque closures cannot be compared).
            // This blocks Salsa memoization for chains containing these variants.
            (Self::ModifyAnchor { .. }, _) | (Self::Custom(_), _) => false,
            _ => false,
        }
    }
}

impl std::fmt::Debug for ElementPatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity => write!(f, "Identity"),
            Self::WrapContainer { style } => f
                .debug_struct("WrapContainer")
                .field("style", style)
                .finish(),
            Self::Prepend { element } => {
                f.debug_struct("Prepend").field("element", element).finish()
            }
            Self::Append { element } => f.debug_struct("Append").field("element", element).finish(),
            Self::Replace { element } => {
                f.debug_struct("Replace").field("element", element).finish()
            }
            Self::ModifyStyle { overlay } => f
                .debug_struct("ModifyStyle")
                .field("overlay", overlay)
                .finish(),
            Self::Compose(patches) => f.debug_tuple("Compose").field(patches).finish(),
            Self::ModifyAnchor { .. } => write!(f, "ModifyAnchor(<fn>)"),
            Self::When {
                predicate,
                then,
                otherwise,
            } => f
                .debug_struct("When")
                .field("predicate", predicate)
                .field("then", then)
                .field("otherwise", otherwise)
                .finish(),
            Self::Custom(_) => write!(f, "Custom(<fn>)"),
        }
    }
}

impl ElementPatch {
    /// Algebraic simplification:
    /// - Removes `Identity` elements from `Compose`
    /// - Flattens nested `Compose`
    /// - `Replace` absorbs all prior patches
    /// - Single-element `Compose` is unwrapped
    /// - Empty `Compose` becomes `Identity`
    /// - `When` branches are recursively normalized; collapsed to `Identity` if both are `Identity`
    pub fn normalize(self) -> Self {
        match self {
            Self::Compose(patches) => {
                let mut normalized: Vec<ElementPatch> = Vec::new();
                for patch in patches {
                    let patch = patch.normalize();
                    match patch {
                        Self::Identity => {}                              // remove identity
                        Self::Compose(inner) => normalized.extend(inner), // flatten
                        other => normalized.push(other),
                    }
                }
                // Replace absorbs all prior patches
                if let Some(last_replace) = normalized
                    .iter()
                    .rposition(|p| matches!(p, Self::Replace { .. }))
                {
                    normalized.drain(..last_replace);
                }
                match normalized.len() {
                    0 => Self::Identity,
                    1 => normalized.into_iter().next().unwrap(),
                    _ => Self::Compose(normalized),
                }
            }
            Self::When {
                predicate,
                then,
                otherwise,
            } => {
                let then = then.normalize();
                let otherwise = otherwise.normalize();
                if matches!((&then, &otherwise), (Self::Identity, Self::Identity)) {
                    Self::Identity
                } else {
                    Self::When {
                        predicate,
                        then: Box::new(then),
                        otherwise: Box::new(otherwise),
                    }
                }
            }
            other => other,
        }
    }

    /// Apply this patch to a [`TransformSubject`].
    pub fn apply(self, subject: TransformSubject) -> TransformSubject {
        match self {
            Self::Identity => subject,

            Self::WrapContainer { style: _ } => {
                // Wrap element in a Flex container.
                // WireFace styling is carried in the patch for the rendering pipeline;
                // structural wrapping is applied here.
                subject.map_element(|el| Element::Flex {
                    direction: Direction::Column,
                    children: vec![FlexChild::fixed(el)],
                    gap: 0,
                    align: Align::Start,
                    cross_align: Align::Start,
                })
            }

            Self::Prepend { element: prepend } => subject.map_element(|el| Element::Flex {
                direction: Direction::Column,
                children: vec![FlexChild::fixed(prepend), FlexChild::fixed(el)],
                gap: 0,
                align: Align::Start,
                cross_align: Align::Start,
            }),

            Self::Append { element: append } => subject.map_element(|el| Element::Flex {
                direction: Direction::Column,
                children: vec![FlexChild::fixed(el), FlexChild::fixed(append)],
                gap: 0,
                align: Align::Start,
                cross_align: Align::Start,
            }),

            Self::Replace { element } => subject.map_element(|_| element),

            Self::ModifyStyle { overlay } => {
                subject.map_element(|el| overlay_face_on_element(el, &overlay.to_face()))
            }

            Self::Compose(patches) => patches
                .into_iter()
                .fold(subject, |subj, patch| patch.apply(subj)),

            Self::ModifyAnchor { transform } => subject.map_anchor(|a| transform(a)),

            Self::When { then, .. } => {
                // Without context, When always takes the `then` branch.
                then.apply(subject)
            }

            Self::Custom(f) => f(subject),
        }
    }

    /// Apply this patch to a [`TransformSubject`] with a [`TransformContext`].
    ///
    /// `When` predicates are evaluated against the context. Other variants
    /// delegate to [`apply()`](Self::apply).
    pub fn apply_with_context(
        self,
        subject: TransformSubject,
        ctx: &TransformContext,
    ) -> TransformSubject {
        match self {
            Self::When {
                predicate,
                then,
                otherwise,
            } => {
                if predicate.evaluate_in_transform_context(ctx) {
                    then.apply_with_context(subject, ctx)
                } else {
                    otherwise.apply_with_context(subject, ctx)
                }
            }
            Self::Compose(patches) => patches
                .into_iter()
                .fold(subject, |subj, patch| patch.apply_with_context(subj, ctx)),
            other => other.apply(subject),
        }
    }

    /// Returns `true` if this patch contains no `Custom` or `ModifyAnchor` variants.
    ///
    /// Pure patches can potentially be stored as Salsa inputs for memoization.
    /// `When` is pure if both branches are pure (predicates are always pure).
    pub fn is_pure(&self) -> bool {
        match self {
            Self::Custom(_) | Self::ModifyAnchor { .. } => false,
            Self::Compose(patches) => patches.iter().all(Self::is_pure),
            Self::When {
                then, otherwise, ..
            } => then.is_pure() && otherwise.is_pure(),
            _ => true,
        }
    }

    /// Infer the [`TransformScope`] from this patch variant.
    ///
    /// For `Compose`, returns the most impactful scope among children.
    /// For `When`, returns the most impactful scope across both branches.
    pub fn scope(&self) -> TransformScope {
        match self {
            Self::Identity => TransformScope::Identity,
            Self::WrapContainer { .. } => TransformScope::Wrapper,
            Self::Prepend { .. } => TransformScope::Prepend,
            Self::Append { .. } => TransformScope::Append,
            Self::Replace { .. } => TransformScope::Replacement,
            Self::ModifyStyle { .. } => TransformScope::Attribute,
            Self::Compose(patches) => patches
                .iter()
                .map(Self::scope)
                .max_by_key(scope_impact)
                .unwrap_or(TransformScope::Identity),
            Self::ModifyAnchor { .. } => TransformScope::Structural,
            Self::When {
                then, otherwise, ..
            } => {
                let t = then.scope();
                let o = otherwise.scope();
                if scope_impact(&t) >= scope_impact(&o) {
                    t
                } else {
                    o
                }
            }
            Self::Custom(_) => TransformScope::Structural,
        }
    }
}

/// Ordering of scope impact for `Compose` scope inference.
fn scope_impact(scope: &TransformScope) -> u8 {
    match scope {
        TransformScope::Identity => 0,
        TransformScope::Attribute => 1,
        TransformScope::Prepend | TransformScope::Append => 2,
        TransformScope::Wrapper => 3,
        TransformScope::Structural => 4,
        TransformScope::Replacement => 5,
    }
}

/// Best-effort face overlay on an element's styled content.
fn overlay_face_on_element(el: Element, face: &WireFace) -> Element {
    match el {
        Element::Text(text, style) => {
            let new_style = match style {
                ElementStyle::Inline(arc) => {
                    let mut base = arc.to_face();
                    FaceMerge::Overlay.apply(&mut base, face);
                    ElementStyle::from(base)
                }
                token @ ElementStyle::Token(_) => token, // Cannot modify token-based styles
            };
            Element::Text(text, new_style)
        }
        Element::StyledLine(atoms) => Element::StyledLine(
            atoms
                .into_iter()
                .map(|atom| {
                    let mut merged = atom.unresolved_style().to_face();
                    FaceMerge::Overlay.apply(&mut merged, face);
                    crate::protocol::Atom::with_style(
                        atom.contents,
                        crate::protocol::Style::from_face(&merged),
                    )
                })
                .collect(),
        ),
        // For complex element types (Flex, Stack, etc.), face overlay cannot be
        // applied structurally. The patch is preserved as metadata for the
        // rendering pipeline to interpret.
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::protocol::{Color, NamedColor, WireFace};

    fn sample_element() -> Element {
        Element::plain_text("hello")
    }

    fn sample_subject() -> TransformSubject {
        TransformSubject::Element(sample_element())
    }

    // --- is_pure ---

    #[test]
    fn identity_is_pure() {
        assert!(ElementPatch::Identity.is_pure());
    }

    #[test]
    fn replace_is_pure() {
        assert!(
            ElementPatch::Replace {
                element: Element::Empty
            }
            .is_pure()
        );
    }

    #[test]
    fn modify_face_is_pure() {
        assert!(
            ElementPatch::ModifyStyle {
                overlay: crate::protocol::default_unresolved_style()
            }
            .is_pure()
        );
    }

    #[test]
    fn custom_is_not_pure() {
        let patch = ElementPatch::Custom(Arc::new(|s| s));
        assert!(!patch.is_pure());
    }

    #[test]
    fn modify_anchor_is_not_pure() {
        let patch = ElementPatch::ModifyAnchor {
            transform: Arc::new(|a| a),
        };
        assert!(!patch.is_pure());
    }

    #[test]
    fn compose_pure_if_all_children_pure() {
        let patch = ElementPatch::Compose(vec![
            ElementPatch::Identity,
            ElementPatch::Replace {
                element: Element::Empty,
            },
        ]);
        assert!(patch.is_pure());
    }

    #[test]
    fn compose_impure_if_any_child_impure() {
        let patch = ElementPatch::Compose(vec![
            ElementPatch::Identity,
            ElementPatch::Custom(Arc::new(|s| s)),
        ]);
        assert!(!patch.is_pure());
    }

    // --- scope ---

    #[test]
    fn scope_identity() {
        assert_eq!(ElementPatch::Identity.scope(), TransformScope::Identity);
    }

    #[test]
    fn scope_replace() {
        assert_eq!(
            ElementPatch::Replace {
                element: Element::Empty
            }
            .scope(),
            TransformScope::Replacement
        );
    }

    #[test]
    fn scope_wrap() {
        assert_eq!(
            ElementPatch::WrapContainer {
                style: crate::protocol::default_unresolved_style()
            }
            .scope(),
            TransformScope::Wrapper
        );
    }

    #[test]
    fn scope_compose_picks_most_impactful() {
        let patch = ElementPatch::Compose(vec![
            ElementPatch::Prepend {
                element: Element::Empty,
            },
            ElementPatch::Replace {
                element: Element::Empty,
            },
        ]);
        assert_eq!(patch.scope(), TransformScope::Replacement);
    }

    #[test]
    fn scope_empty_compose() {
        assert_eq!(
            ElementPatch::Compose(vec![]).scope(),
            TransformScope::Identity
        );
    }

    // --- normalize ---

    #[test]
    fn normalize_identity_removal() {
        let patch = ElementPatch::Compose(vec![
            ElementPatch::Identity,
            ElementPatch::Prepend {
                element: Element::Empty,
            },
            ElementPatch::Identity,
        ]);
        let normalized = patch.normalize();
        // Should be just the Prepend (single element unwrapped)
        assert!(matches!(normalized, ElementPatch::Prepend { .. }));
    }

    #[test]
    fn normalize_empty_compose_to_identity() {
        let patch = ElementPatch::Compose(vec![ElementPatch::Identity, ElementPatch::Identity]);
        let normalized = patch.normalize();
        assert!(matches!(normalized, ElementPatch::Identity));
    }

    #[test]
    fn normalize_flatten_nested_compose() {
        let inner = ElementPatch::Compose(vec![
            ElementPatch::Prepend {
                element: Element::Empty,
            },
            ElementPatch::Append {
                element: Element::Empty,
            },
        ]);
        let outer = ElementPatch::Compose(vec![
            inner,
            ElementPatch::ModifyStyle {
                overlay: crate::protocol::default_unresolved_style(),
            },
        ]);
        let normalized = outer.normalize();
        // Should be a flat Compose with 3 elements
        match normalized {
            ElementPatch::Compose(patches) => assert_eq!(patches.len(), 3),
            _ => panic!("expected Compose"),
        }
    }

    #[test]
    fn normalize_replace_absorbs_prior() {
        let patch = ElementPatch::Compose(vec![
            ElementPatch::Prepend {
                element: Element::Empty,
            },
            ElementPatch::ModifyStyle {
                overlay: crate::protocol::default_unresolved_style(),
            },
            ElementPatch::Replace {
                element: sample_element(),
            },
            ElementPatch::Append {
                element: Element::Empty,
            },
        ]);
        let normalized = patch.normalize();
        // Prepend and ModifyFace should be absorbed by Replace
        match normalized {
            ElementPatch::Compose(patches) => {
                assert_eq!(patches.len(), 2);
                assert!(matches!(patches[0], ElementPatch::Replace { .. }));
                assert!(matches!(patches[1], ElementPatch::Append { .. }));
            }
            _ => panic!("expected Compose with 2 elements"),
        }
    }

    #[test]
    fn normalize_single_element_unwrap() {
        let patch = ElementPatch::Compose(vec![ElementPatch::Replace {
            element: Element::Empty,
        }]);
        let normalized = patch.normalize();
        assert!(matches!(normalized, ElementPatch::Replace { .. }));
    }

    #[test]
    fn normalize_non_compose_unchanged() {
        let patch = ElementPatch::Prepend {
            element: Element::Empty,
        };
        let normalized = patch.normalize();
        assert!(matches!(normalized, ElementPatch::Prepend { .. }));
    }

    // --- apply ---

    #[test]
    fn apply_identity() {
        let subject = sample_subject();
        let result = ElementPatch::Identity.apply(subject.clone());
        assert_eq!(result, subject);
    }

    #[test]
    fn apply_replace() {
        let replacement = Element::plain_text("replaced");
        let result = ElementPatch::Replace {
            element: replacement.clone(),
        }
        .apply(sample_subject());
        assert_eq!(result.into_element(), replacement);
    }

    #[test]
    fn apply_custom() {
        let result = ElementPatch::Custom(Arc::new(|s| {
            s.map_element(|_| Element::plain_text("custom"))
        }))
        .apply(sample_subject());
        assert_eq!(result.into_element(), Element::plain_text("custom"));
    }

    #[test]
    fn apply_compose_sequential() {
        // Replace then Append
        let patch = ElementPatch::Compose(vec![
            ElementPatch::Replace {
                element: Element::plain_text("base"),
            },
            ElementPatch::Append {
                element: Element::plain_text("after"),
            },
        ]);
        let result = patch.apply(sample_subject());
        // Should be a Flex with base and after
        match result.into_element() {
            Element::Flex { children, .. } => {
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected Flex, got {other:?}"),
        }
    }

    #[test]
    fn apply_modify_face_on_styled_line() {
        use crate::protocol::Atom;
        let atoms = vec![Atom::plain("test")];
        let subject = TransformSubject::Element(Element::StyledLine(atoms));
        let overlay = Arc::new(crate::protocol::UnresolvedStyle::from_face(&WireFace {
            fg: Color::Named(NamedColor::Red),
            ..WireFace::default()
        }));
        let result = ElementPatch::ModifyStyle { overlay }.apply(subject);
        match result.into_element() {
            Element::StyledLine(atoms) => {
                assert_eq!(
                    atoms[0].unresolved_style().to_face().fg,
                    Color::Named(NamedColor::Red)
                );
            }
            other => panic!("expected StyledLine, got {other:?}"),
        }
    }

    #[test]
    fn apply_modify_anchor_on_overlay() {
        use crate::element::Overlay;
        let overlay = Overlay {
            element: Element::plain_text("menu"),
            anchor: OverlayAnchor::Absolute {
                x: 1,
                y: 2,
                w: 10,
                h: 5,
            },
        };
        let subject = TransformSubject::Overlay(overlay);
        let result = ElementPatch::ModifyAnchor {
            transform: Arc::new(|_| OverlayAnchor::Fill),
        }
        .apply(subject);
        match result {
            TransformSubject::Overlay(o) => assert_eq!(o.anchor, OverlayAnchor::Fill),
            _ => panic!("expected Overlay"),
        }
    }

    #[test]
    fn apply_modify_anchor_noop_for_element() {
        let subject = sample_subject();
        let result = ElementPatch::ModifyAnchor {
            transform: Arc::new(|_| OverlayAnchor::Fill),
        }
        .apply(subject.clone());
        assert_eq!(result, subject);
    }

    // --- PartialEq ---

    #[test]
    fn eq_identity() {
        assert_eq!(ElementPatch::Identity, ElementPatch::Identity);
    }

    #[test]
    fn eq_replace_same() {
        let a = ElementPatch::Replace {
            element: Element::Empty,
        };
        let b = ElementPatch::Replace {
            element: Element::Empty,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn ne_replace_different() {
        let a = ElementPatch::Replace {
            element: Element::Empty,
        };
        let b = ElementPatch::Replace {
            element: sample_element(),
        };
        assert_ne!(a, b);
    }

    #[test]
    fn ne_different_variants() {
        assert_ne!(
            ElementPatch::Identity,
            ElementPatch::Replace {
                element: Element::Empty,
            }
        );
    }

    #[test]
    fn ne_custom_always_unequal() {
        let a = ElementPatch::Custom(Arc::new(|s| s));
        let b = ElementPatch::Custom(Arc::new(|s| s));
        assert_ne!(a, b);
        // Even the same instance cloned compares as unequal
        assert_ne!(a, a.clone());
    }

    #[test]
    fn ne_modify_anchor_always_unequal() {
        let a = ElementPatch::ModifyAnchor {
            transform: Arc::new(|a| a),
        };
        assert_ne!(a, a.clone());
    }

    #[test]
    fn eq_compose_structural() {
        let a = ElementPatch::Compose(vec![
            ElementPatch::Identity,
            ElementPatch::Replace {
                element: Element::Empty,
            },
        ]);
        let b = ElementPatch::Compose(vec![
            ElementPatch::Identity,
            ElementPatch::Replace {
                element: Element::Empty,
            },
        ]);
        assert_eq!(a, b);
    }

    // --- Composable monoid laws (via proptest in tests/compose.rs) ---

    #[test]
    fn composable_identity_element() {
        use super::super::compose::Composable;
        let empty = ElementPatch::empty();
        assert!(matches!(empty, ElementPatch::Identity));
    }

    #[test]
    fn composable_left_identity() {
        use super::super::compose::Composable;
        let patch = ElementPatch::Replace {
            element: Element::Empty,
        };
        // Identity.compose(x) applied to a subject should equal x applied to the subject
        let subject = sample_subject();
        let composed = ElementPatch::empty().compose(patch.clone());
        assert_eq!(
            composed.apply(subject.clone()).into_element(),
            patch.apply(subject).into_element()
        );
    }

    #[test]
    fn composable_right_identity() {
        use super::super::compose::Composable;
        let patch = ElementPatch::Replace {
            element: Element::Empty,
        };
        let subject = sample_subject();
        let composed = patch.clone().compose(ElementPatch::empty());
        assert_eq!(
            composed.apply(subject.clone()).into_element(),
            patch.apply(subject).into_element()
        );
    }

    #[test]
    fn composable_associativity() {
        use super::super::compose::Composable;
        let a = ElementPatch::Replace {
            element: Element::plain_text("a"),
        };
        let b = ElementPatch::Append {
            element: Element::plain_text("b"),
        };
        let c = ElementPatch::Append {
            element: Element::plain_text("c"),
        };

        let subject = sample_subject();
        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        assert_eq!(
            left.apply(subject.clone()).into_element(),
            right.apply(subject).into_element()
        );
    }
}
