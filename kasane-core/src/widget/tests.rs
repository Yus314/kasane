//! Unit tests for the widget system.

use compact_str::CompactString;

use crate::plugin::{AppView, ContributeContext, PluginBackend, SlotId, TransformContext};
use crate::state::AppState;

use super::backend::WidgetBackend;
use super::condition::parse_condition;
use super::parse::parse_widgets;
use super::types::Template;
use super::variables::{AppViewResolver, VariableResolver, variable_dirty_flag};
use crate::state::DirtyFlags;

// =============================================================================
// Template tests
// =============================================================================

#[test]
fn template_literal_only() {
    let t = Template::parse("hello world").unwrap();
    let resolver = StaticResolver::new(&[]);
    assert_eq!(t.expand(&resolver), "hello world");
}

#[test]
fn template_single_variable() {
    let t = Template::parse("{cursor_line}").unwrap();
    let resolver = StaticResolver::new(&[("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver), "42");
}

#[test]
fn template_mixed() {
    let t = Template::parse(" {cursor_line}:{cursor_col} ").unwrap();
    let resolver = StaticResolver::new(&[("cursor_line", "10"), ("cursor_col", "5")]);
    assert_eq!(t.expand(&resolver), " 10:5 ");
}

#[test]
fn template_padding() {
    let t = Template::parse("{cursor_line:4}").unwrap();
    let resolver = StaticResolver::new(&[("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver), "  42");
}

#[test]
fn template_padding_no_pad_needed() {
    let t = Template::parse("{cursor_line:2}").unwrap();
    let resolver = StaticResolver::new(&[("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver), "42");
}

#[test]
fn template_unclosed_brace() {
    assert!(Template::parse("{cursor_line").is_err());
}

#[test]
fn template_empty_variable() {
    assert!(Template::parse("{}").is_err());
}

#[test]
fn template_referenced_variables() {
    let t = Template::parse("a {x} b {y} c").unwrap();
    let vars: Vec<&str> = t.referenced_variables().collect();
    assert_eq!(vars, &["x", "y"]);
}

#[test]
fn template_no_variables() {
    let t = Template::parse("just text").unwrap();
    let vars: Vec<&str> = t.referenced_variables().collect();
    assert!(vars.is_empty());
}

// =============================================================================
// Condition tests
// =============================================================================

#[test]
fn cond_truthy() {
    let expr = parse_condition("is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "true")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_falsy_empty() {
    let expr = parse_condition("is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "")]);
    assert!(!expr.evaluate(&resolver));
}

#[test]
fn cond_falsy_zero() {
    let expr = parse_condition("is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "0")]);
    assert!(!expr.evaluate(&resolver));
}

#[test]
fn cond_eq() {
    let expr = parse_condition("editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("editor_mode", "insert")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_ne() {
    let expr = parse_condition("editor_mode != 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("editor_mode", "normal")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_gt_numeric() {
    let expr = parse_condition("cursor_count > 1").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "3")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_lt_numeric() {
    let expr = parse_condition("cursor_count < 2").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "1")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_and() {
    let expr = parse_condition("cursor_count > 1 && editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "3"), ("editor_mode", "insert")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_and_false() {
    let expr = parse_condition("cursor_count > 1 && editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "1"), ("editor_mode", "insert")]);
    assert!(!expr.evaluate(&resolver));
}

#[test]
fn cond_or() {
    let expr = parse_condition("cursor_count > 1 || editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "1"), ("editor_mode", "insert")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_not() {
    let expr = parse_condition("!is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_not_truthy() {
    let expr = parse_condition("!is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "true")]);
    assert!(!expr.evaluate(&resolver));
}

#[test]
fn cond_referenced_variables() {
    let expr = parse_condition("cursor_count > 1 && editor_mode == 'insert'").unwrap();
    let vars = expr.referenced_variables();
    assert_eq!(vars, &["cursor_count", "editor_mode"]);
}

#[test]
fn cond_unexpected_end() {
    assert!(parse_condition("cursor_count >").is_err());
}

#[test]
fn cond_ge() {
    let expr = parse_condition("cursor_count >= 2").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "2")]);
    assert!(expr.evaluate(&resolver));
}

#[test]
fn cond_le() {
    let expr = parse_condition("cursor_count <= 2").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "3")]);
    assert!(!expr.evaluate(&resolver));
}

// =============================================================================
// KDL parse tests
// =============================================================================

#[test]
fn parse_simple_contribution() {
    let source = r#"position slot="status-right" text=" {cursor_line}:{cursor_col} ""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    assert_eq!(file.widgets.len(), 1);
    assert!(matches!(
        file.widgets[0].kind,
        super::types::WidgetKind::Contribution(_)
    ));
}

#[test]
fn parse_contribution_with_parts() {
    let source = r#"
status-info slot="status-right" {
    part text=" {editor_mode} " face="default,blue+b"
    part text=" {cursor_line}:{cursor_col} "
}
"#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert_eq!(c.parts.len(), 2);
        assert!(c.parts[0].face.is_some());
        assert!(c.parts[1].face.is_none());
    } else {
        panic!("expected contribution widget");
    }
}

#[test]
fn parse_background_widget() {
    let source = r#"cursorline kind="background" line="cursor" face="default,rgb:303030""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    assert_eq!(file.widgets.len(), 1);
    assert!(matches!(
        file.widgets[0].kind,
        super::types::WidgetKind::Background(_)
    ));
}

#[test]
fn parse_transform_widget() {
    let source = r#"insert-status kind="transform" target="status" face="default,blue" when="editor_mode == 'insert'""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    assert_eq!(file.widgets.len(), 1);
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert!(t.when.is_some());
    } else {
        panic!("expected transform widget");
    }
}

#[test]
fn parse_syntax_error_rejects_entire_file() {
    let result = parse_widgets("this is not { valid kdl }}}");
    assert!(result.is_err());
}

#[test]
fn parse_invalid_node_skipped() {
    let source = r#"
good slot="status-left" text="ok"
bad kind="unknown_kind" slot="status-left" text="nope"
"#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown widget kind"));
}

#[test]
fn parse_missing_slot() {
    let source = r#"no-slot text="hello""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("slot"));
}

#[test]
fn parse_multiple_widgets() {
    let source = r#"
a slot="status-left" text="A"
b slot="status-right" text="B"
c kind="background" line="cursor" face="default,rgb:333333"
"#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    assert_eq!(file.widgets.len(), 3);
}

#[test]
fn parse_computed_deps() {
    let source = r#"pos slot="status-right" text=" {cursor_line}:{cursor_col} ""#;
    let (file, _) = parse_widgets(source).unwrap();
    assert!(file.computed_deps.contains(DirtyFlags::BUFFER_CURSOR));
}

// =============================================================================
// Backend tests
// =============================================================================

#[test]
fn backend_empty_has_no_capabilities() {
    let backend = WidgetBackend::empty();
    assert!(backend.capabilities().is_empty());
}

#[test]
fn backend_from_source_with_contribution() {
    let source = r#"pos slot="status-right" text=" {cursor_line} ""#;
    let backend = WidgetBackend::from_source(source);
    assert!(
        backend
            .capabilities()
            .contains(crate::plugin::PluginCapabilities::CONTRIBUTOR)
    );
}

#[test]
fn backend_contribute_to_matching_slot() {
    let source = r#"pos slot="status-right" text=" {cursor_line}:{cursor_col} ""#;
    let backend = WidgetBackend::from_source(source);

    let mut state = AppState::default();
    state.cursor_pos = crate::protocol::Coord { line: 9, column: 4 };
    let view = AppView::new(&state);
    let ctx = ContributeContext::new(&view, None);

    let result = backend.contribute_to(&SlotId::STATUS_RIGHT, &view, &ctx);
    assert!(result.is_some());
}

#[test]
fn backend_contribute_to_non_matching_slot() {
    let source = r#"pos slot="status-right" text="test""#;
    let backend = WidgetBackend::from_source(source);

    let state = AppState::default();
    let view = AppView::new(&state);
    let ctx = ContributeContext::new(&view, None);

    let result = backend.contribute_to(&SlotId::STATUS_LEFT, &view, &ctx);
    assert!(result.is_none());
}

#[test]
fn backend_contribute_with_when_condition() {
    let source = r#"pos slot="status-right" text=" multi " when="cursor_count > 1""#;
    let backend = WidgetBackend::from_source(source);

    // cursor_count = 0 (default) → condition false
    let state = AppState::default();
    let view = AppView::new(&state);
    let ctx = ContributeContext::new(&view, None);
    let result = backend.contribute_to(&SlotId::STATUS_RIGHT, &view, &ctx);
    assert!(result.is_none());

    // cursor_count = 3 → condition true
    let mut state = AppState::default();
    state.cursor_count = 3;
    let view = AppView::new(&state);
    let ctx = ContributeContext::new(&view, None);
    let result = backend.contribute_to(&SlotId::STATUS_RIGHT, &view, &ctx);
    assert!(result.is_some());
}

#[test]
fn backend_multiple_contributions_same_slot() {
    let source = r#"
a slot="status-right" text="A"
b slot="status-right" text="B"
"#;
    let backend = WidgetBackend::from_source(source);

    let state = AppState::default();
    let view = AppView::new(&state);
    let ctx = ContributeContext::new(&view, None);

    let result = backend.contribute_to(&SlotId::STATUS_RIGHT, &view, &ctx);
    assert!(result.is_some());
    // Should be an Element::row combining both
    let contrib = result.unwrap();
    match contrib.element {
        crate::element::Element::Flex { ref children, .. } => {
            assert_eq!(children.len(), 2);
        }
        _ => panic!("expected Flex element for multi-widget slot"),
    }
}

#[test]
fn backend_background_annotation() {
    let source = r#"hl kind="background" line="cursor" face="default,rgb:333333""#;
    let backend = WidgetBackend::from_source(source);

    let mut state = AppState::default();
    state.cursor_pos = crate::protocol::Coord { line: 5, column: 0 };
    let view = AppView::new(&state);
    let ctx = crate::plugin::AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    // Line 5 should match cursor line
    assert!(backend.annotate_background(5, &view, &ctx).is_some());
    // Line 3 should not match
    assert!(backend.annotate_background(3, &view, &ctx).is_none());
}

#[test]
fn backend_transform_patch() {
    let source = r#"ins kind="transform" target="status" face="default,blue" when="editor_mode == 'insert'""#;
    let backend = WidgetBackend::from_source(source);

    let mut state = AppState::default();
    state.editor_mode = crate::state::derived::EditorMode::Insert;
    let view = AppView::new(&state);
    let ctx = TransformContext {
        is_default: true,
        chain_position: 0,
        pane_surface_id: None,
        pane_focused: true,
        target_line: None,
    };

    let result = backend.transform_patch(
        &kasane_plugin_model::TransformTarget::STATUS_BAR,
        &view,
        &ctx,
    );
    assert!(result.is_some());

    // Normal mode → should not match
    let state = AppState::default(); // default is Normal
    let view = AppView::new(&state);
    let result = backend.transform_patch(
        &kasane_plugin_model::TransformTarget::STATUS_BAR,
        &view,
        &ctx,
    );
    assert!(result.is_none());
}

#[test]
fn backend_parse_error_produces_diagnostic() {
    let mut backend = WidgetBackend::from_source("this is not { valid kdl }}}");
    let diags = backend.drain_diagnostics();
    assert!(!diags.is_empty());
}

#[test]
fn backend_id() {
    let backend = WidgetBackend::empty();
    assert_eq!(backend.id().0, "kasane.widgets");
}

#[test]
fn backend_view_deps_tracks_variables() {
    let source = r#"
pos slot="status-right" text=" {cursor_line}:{cursor_col} "
mode slot="status-left" text=" {editor_mode} "
"#;
    let backend = WidgetBackend::from_source(source);
    let deps = backend.view_deps();
    assert!(deps.contains(DirtyFlags::BUFFER_CURSOR));
    assert!(deps.contains(DirtyFlags::STATUS));
}

// =============================================================================
// Variable resolution tests
// =============================================================================

#[test]
fn variable_resolver_cursor() {
    let mut state = AppState::default();
    state.cursor_pos = crate::protocol::Coord { line: 9, column: 4 };
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("cursor_line"), "10"); // 1-indexed
    assert_eq!(resolver.resolve("cursor_col"), "5"); // 1-indexed
}

#[test]
fn variable_resolver_editor_mode() {
    let mut state = AppState::default();
    state.editor_mode = crate::state::derived::EditorMode::Insert;
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("editor_mode"), "insert");
}

#[test]
fn variable_resolver_unknown() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("nonexistent"), "");
}

#[test]
fn variable_resolver_opt() {
    let mut state = AppState::default();
    state
        .ui_options
        .insert("filetype".to_string(), "rust".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.filetype"), "rust");
}

#[test]
fn variable_dirty_flag_cursor() {
    assert_eq!(
        variable_dirty_flag("cursor_line"),
        DirtyFlags::BUFFER_CURSOR
    );
    assert_eq!(variable_dirty_flag("cursor_col"), DirtyFlags::BUFFER_CURSOR);
}

#[test]
fn variable_dirty_flag_mode() {
    assert_eq!(variable_dirty_flag("editor_mode"), DirtyFlags::STATUS);
}

#[test]
fn variable_dirty_flag_opt() {
    assert_eq!(variable_dirty_flag("opt.filetype"), DirtyFlags::OPTIONS);
}

// =============================================================================
// Helpers
// =============================================================================

struct StaticResolver<'a> {
    vars: &'a [(&'a str, &'a str)],
}

impl<'a> StaticResolver<'a> {
    fn new(vars: &'a [(&'a str, &'a str)]) -> Self {
        Self { vars }
    }
}

impl VariableResolver for StaticResolver<'_> {
    fn resolve(&self, name: &str) -> CompactString {
        for (k, v) in self.vars {
            if *k == name {
                return CompactString::from(*v);
            }
        }
        CompactString::default()
    }
}
