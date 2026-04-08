//! Unified predicate algebra for widget conditions and element patch predicates.
//!
//! Merges the widget `CondExpr` and plugin `PatchPredicate` into a single
//! `Predicate` type that can express both variable-based conditions and
//! multi-pane context predicates (focus, surface, line range).

use std::ops::Range;
use std::sync::Arc;

use compact_str::CompactString;

use crate::surface::SurfaceId;

use super::types::{CmpOp, Value};
use super::variables::VariableResolver;

/// A unified predicate for conditional evaluation.
///
/// Combines variable-based conditions (from widget `when=` attributes) and
/// pane-context predicates (from `ElementPatch::When`) into a single algebra.
///
/// Implements `Clone`, `PartialEq`, `Debug` for Salsa compatibility.
#[derive(Debug, Clone)]
pub enum Predicate {
    // --- From PatchPredicate ---
    /// True when the pane is focused.
    HasFocus,
    /// True when the pane's surface ID matches.
    SurfaceIs(SurfaceId),
    /// True when the target line falls within the given range.
    LineRange(Range<usize>),

    // --- From CondExpr ---
    /// Variable is truthy (non-empty, non-zero).
    VariableTruthy(CompactString),
    /// Variable compared to a typed value.
    VariableCompare {
        variable: CompactString,
        op: CmpOp,
        value: Value,
    },
    /// Variable matches a regex pattern (`=~`).
    VariableMatches {
        variable: CompactString,
        pattern: Arc<regex_lite::Regex>,
    },
    /// Variable value is a member of a set (`in`).
    VariableIn {
        variable: CompactString,
        values: Vec<Value>,
    },

    // --- Logical connectives ---
    Not(Box<Predicate>),
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
}

/// Context for evaluating predicates.
///
/// Combines variable resolution (from widgets) with pane context (from transforms).
pub struct PredicateContext<'a> {
    /// Variable resolver for `VariableTruthy` / `VariableCompare`.
    pub resolver: &'a dyn VariableResolver,
    /// Whether the pane is focused (for `HasFocus`).
    pub pane_focused: bool,
    /// The pane's surface ID (for `SurfaceIs`).
    pub pane_surface_id: Option<SurfaceId>,
    /// The target line number (for `LineRange`).
    pub target_line: Option<usize>,
}

impl Predicate {
    /// Evaluate this predicate against a full context.
    pub fn evaluate(&self, ctx: &PredicateContext<'_>) -> bool {
        match self {
            Self::HasFocus => ctx.pane_focused,
            Self::SurfaceIs(id) => ctx.pane_surface_id == Some(*id),
            Self::LineRange(range) => ctx.target_line.is_some_and(|l| range.contains(&l)),
            Self::VariableTruthy(name) => ctx.resolver.resolve(name).is_truthy(),
            Self::VariableCompare {
                variable,
                op,
                value,
            } => ctx.resolver.resolve(variable).compare(*op, value),
            Self::VariableMatches { variable, pattern } => {
                let val = ctx.resolver.resolve(variable).to_display();
                pattern.is_match(&val)
            }
            Self::VariableIn { variable, values } => {
                let val = ctx.resolver.resolve(variable);
                values.iter().any(|v| val.compare(CmpOp::Eq, v))
            }
            Self::Not(inner) => !inner.evaluate(ctx),
            Self::And(a, b) => a.evaluate(ctx) && b.evaluate(ctx),
            Self::Or(a, b) => a.evaluate(ctx) || b.evaluate(ctx),
        }
    }

    /// Evaluate using only a variable resolver (convenience for widget conditions).
    ///
    /// Pane-context predicates (`HasFocus`, `SurfaceIs`, `LineRange`) evaluate
    /// using default values (unfocused, no surface, no line).
    pub fn evaluate_with_resolver(&self, resolver: &dyn VariableResolver) -> bool {
        let ctx = PredicateContext {
            resolver,
            pane_focused: false,
            pane_surface_id: None,
            target_line: None,
        };
        self.evaluate(&ctx)
    }

    /// Iterate over all variable names referenced in this predicate.
    pub fn referenced_variables(&self) -> Vec<&str> {
        let mut vars = Vec::new();
        collect_variables(self, &mut vars);
        vars
    }

    /// Append all referenced variable names to the given vector.
    pub fn collect_variables<'a>(&'a self, out: &mut Vec<&'a str>) {
        collect_variables(self, out);
    }
}

impl PartialEq for Predicate {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::HasFocus, Self::HasFocus) => true,
            (Self::SurfaceIs(a), Self::SurfaceIs(b)) => a == b,
            (Self::LineRange(a), Self::LineRange(b)) => a == b,
            (Self::VariableTruthy(a), Self::VariableTruthy(b)) => a == b,
            (
                Self::VariableCompare {
                    variable: v1,
                    op: o1,
                    value: val1,
                },
                Self::VariableCompare {
                    variable: v2,
                    op: o2,
                    value: val2,
                },
            ) => v1 == v2 && o1 == o2 && val1 == val2,
            (
                Self::VariableMatches {
                    variable: v1,
                    pattern: p1,
                },
                Self::VariableMatches {
                    variable: v2,
                    pattern: p2,
                },
            ) => v1 == v2 && p1.as_str() == p2.as_str(),
            (
                Self::VariableIn {
                    variable: v1,
                    values: vals1,
                },
                Self::VariableIn {
                    variable: v2,
                    values: vals2,
                },
            ) => v1 == v2 && vals1 == vals2,
            (Self::Not(a), Self::Not(b)) => a == b,
            (Self::And(a1, a2), Self::And(b1, b2)) => a1 == b1 && a2 == b2,
            (Self::Or(a1, a2), Self::Or(b1, b2)) => a1 == b1 && a2 == b2,
            _ => false,
        }
    }
}

fn collect_variables<'a>(pred: &'a Predicate, out: &mut Vec<&'a str>) {
    match pred {
        Predicate::VariableTruthy(name) => out.push(name),
        Predicate::VariableCompare { variable, .. }
        | Predicate::VariableMatches { variable, .. }
        | Predicate::VariableIn { variable, .. } => out.push(variable),
        Predicate::And(a, b) | Predicate::Or(a, b) => {
            collect_variables(a, out);
            collect_variables(b, out);
        }
        Predicate::Not(inner) => collect_variables(inner, out),
        Predicate::HasFocus | Predicate::SurfaceIs(_) | Predicate::LineRange(_) => {}
    }
}
