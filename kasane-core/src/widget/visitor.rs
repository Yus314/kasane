//! Visitor pattern for traversing widget definitions.
//!
//! Provides a single walk function that eliminates the 6-way match duplication
//! in `compute_widget_deps` and `collect_widget_variables`.

use super::predicate::Predicate;
use super::types::{FaceRule, LineExpr, Template, WidgetKind, WidgetPatch};

/// Trait for visiting the constituents of a widget kind.
pub trait WidgetVisitor {
    fn visit_template(&mut self, template: &Template);
    fn visit_predicate(&mut self, predicate: &Predicate);
    fn visit_face_rules(&mut self, rules: &[FaceRule]);
    fn visit_line_expr(&mut self, _expr: &LineExpr) {}
}

/// Walk a widget kind, calling visitor methods for each constituent.
pub fn walk_widget_kind(kind: &WidgetKind, visitor: &mut dyn WidgetVisitor) {
    match kind {
        WidgetKind::Contribution(c) => {
            for part in &c.parts {
                visitor.visit_template(&part.template);
                if let Some(ref cond) = part.when {
                    visitor.visit_predicate(cond);
                }
                visitor.visit_face_rules(&part.face_rules);
            }
            if let Some(ref cond) = c.when {
                visitor.visit_predicate(cond);
            }
        }
        WidgetKind::Background(b) => {
            visitor.visit_line_expr(&b.line_expr);
            if let Some(ref cond) = b.when {
                visitor.visit_predicate(cond);
            }
        }
        WidgetKind::Transform(t) => {
            if let Some(ref cond) = t.when {
                visitor.visit_predicate(cond);
            }
            match &t.patch {
                WidgetPatch::ModifyFace(rules) | WidgetPatch::WrapContainer(rules) => {
                    visitor.visit_face_rules(rules);
                }
            }
        }
        WidgetKind::Gutter(g) => {
            for branch in &g.branches {
                visitor.visit_template(&branch.template);
                visitor.visit_face_rules(&branch.face_rules);
                if let Some(ref cond) = branch.line_when {
                    visitor.visit_predicate(cond);
                }
            }
            if let Some(ref cond) = g.when {
                visitor.visit_predicate(cond);
            }
        }
        WidgetKind::Inline(i) => {
            if let Some(ref cond) = i.when {
                visitor.visit_predicate(cond);
            }
        }
        WidgetKind::VirtualText(vt) => {
            visitor.visit_template(&vt.template);
            visitor.visit_face_rules(&vt.face_rules);
            if let Some(ref cond) = vt.when {
                visitor.visit_predicate(cond);
            }
        }
    }
}
