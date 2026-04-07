//! Unit tests for the widget system.

use compact_str::CompactString;

use crate::plugin::{
    AnnotationScope, AppView, ContribSizeHint, ContributeContext, GutterSide, PluginBackend,
    PluginCapabilities, SlotId, TransformContext,
};
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
// Phase 1A: Last-Known-Good Hot Reload
// =============================================================================

#[test]
fn backend_reload_keeps_previous_on_syntax_error() {
    let mut backend = WidgetBackend::from_source(r#"pos slot="status-right" text="ok""#);
    assert!(
        backend
            .capabilities()
            .contains(PluginCapabilities::CONTRIBUTOR)
    );
    let gen_before = backend.state_hash();

    // Reload with broken syntax → should keep previous
    let accepted = backend.reload_from_source("broken {{{ kdl");
    assert!(!accepted);
    assert!(
        backend
            .capabilities()
            .contains(PluginCapabilities::CONTRIBUTOR)
    );
    // Generation should NOT have incremented
    assert_eq!(backend.state_hash(), gen_before);
    // Should have a diagnostic about the rejected reload
    let diags = backend.drain_diagnostics();
    assert!(!diags.is_empty());
}

#[test]
fn backend_reload_replaces_on_valid_source() {
    let mut backend = WidgetBackend::from_source(r#"pos slot="status-right" text="old""#);
    let gen_before = backend.state_hash();

    let accepted =
        backend.reload_from_source(r#"new kind="background" line="cursor" face="default,red""#);
    assert!(accepted);
    // Capabilities should have changed
    assert!(
        !backend
            .capabilities()
            .contains(PluginCapabilities::CONTRIBUTOR)
    );
    assert!(
        backend
            .capabilities()
            .contains(PluginCapabilities::ANNOTATOR)
    );
    // Generation should have incremented
    assert!(backend.state_hash() > gen_before);
}

#[test]
fn backend_reload_generation_increments_only_on_success() {
    let mut backend = WidgetBackend::from_source(r#"a slot="status-left" text="x""#);
    let gen1 = backend.state_hash();

    // Failed reload → no increment
    backend.reload_from_source("invalid {{{ syntax");
    assert_eq!(backend.state_hash(), gen1);

    // Successful reload → increment
    backend.reload_from_source(r#"b slot="status-right" text="y""#);
    assert!(backend.state_hash() > gen1);
}

// =============================================================================
// Phase 1B: LineExpr::Selection
// =============================================================================

#[test]
fn parse_selection_background() {
    let source = r#"sel kind="background" line="selection" face="default,rgb:264f78""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    assert_eq!(file.widgets.len(), 1);
    if let super::types::WidgetKind::Background(ref b) = file.widgets[0].kind {
        assert!(matches!(b.line_expr, super::types::LineExpr::Selection));
    } else {
        panic!("expected background widget");
    }
}

#[test]
fn backend_selection_background_annotation() {
    let source = r#"sel kind="background" line="selection" face="default,rgb:264f78""#;
    let backend = WidgetBackend::from_source(source);

    let mut state = AppState::default();
    // Add a selection spanning lines 3-5
    state.selections = vec![crate::state::derived::Selection {
        anchor: crate::protocol::Coord { line: 3, column: 0 },
        cursor: crate::protocol::Coord {
            line: 5,
            column: 10,
        },
        is_primary: true,
    }];
    let view = AppView::new(&state);
    let ctx = crate::plugin::AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    // Lines within selection range should match
    assert!(backend.annotate_background(3, &view, &ctx).is_some());
    assert!(backend.annotate_background(4, &view, &ctx).is_some());
    assert!(backend.annotate_background(5, &view, &ctx).is_some());
    // Lines outside should not match
    assert!(backend.annotate_background(2, &view, &ctx).is_none());
    assert!(backend.annotate_background(6, &view, &ctx).is_none());
}

#[test]
fn backend_selection_empty_selections() {
    let source = r#"sel kind="background" line="selection" face="default,rgb:264f78""#;
    let backend = WidgetBackend::from_source(source);

    let state = AppState::default(); // no selections
    let view = AppView::new(&state);
    let ctx = crate::plugin::AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    assert!(backend.annotate_background(0, &view, &ctx).is_none());
}

#[test]
fn parse_selection_computed_deps() {
    let source = r#"sel kind="background" line="selection" face="default,red""#;
    let (file, _) = parse_widgets(source).unwrap();
    // Selection depends on BUFFER (= BUFFER_CONTENT | BUFFER_CURSOR)
    assert!(file.computed_deps.contains(DirtyFlags::BUFFER_CONTENT));
    assert!(file.computed_deps.contains(DirtyFlags::BUFFER_CURSOR));
}

// =============================================================================
// Phase 1C: ContribSizeHint
// =============================================================================

#[test]
fn parse_size_hint_auto() {
    let source = r#"a slot="status-left" text="x" size="auto""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert_eq!(c.size_hint, ContribSizeHint::Auto);
    } else {
        panic!("expected contribution");
    }
}

#[test]
fn parse_size_hint_absent_defaults_to_auto() {
    let source = r#"a slot="status-left" text="x""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert_eq!(c.size_hint, ContribSizeHint::Auto);
    } else {
        panic!("expected contribution");
    }
}

#[test]
fn parse_size_hint_fixed() {
    let source = r#"a slot="status-left" text="x" size="20col""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert_eq!(c.size_hint, ContribSizeHint::Fixed(20));
    } else {
        panic!("expected contribution");
    }
}

#[test]
fn parse_size_hint_flex() {
    let source = r#"a slot="status-left" text="x" size="1.5fr""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert_eq!(c.size_hint, ContribSizeHint::Flex(1.5));
    } else {
        panic!("expected contribution");
    }
}

#[test]
fn parse_size_hint_invalid() {
    let source = r#"a slot="status-left" text="x" size="garbage""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
}

#[test]
fn backend_size_hint_used_in_contribution() {
    let source = r#"a slot="status-left" text="x" size="20col""#;
    let backend = WidgetBackend::from_source(source);

    let state = AppState::default();
    let view = AppView::new(&state);
    let ctx = ContributeContext::new(&view, None);

    let result = backend.contribute_to(&SlotId::STATUS_LEFT, &view, &ctx);
    assert!(result.is_some());
    assert_eq!(result.unwrap().size_hint, ContribSizeHint::Fixed(20));
}

// =============================================================================
// Phase 1D: Additional variables
// =============================================================================

#[test]
fn variable_resolver_has_menu() {
    let state = AppState::default(); // no menu
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("has_menu"), "");
}

#[test]
fn variable_resolver_has_info() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("has_info"), "");
}

#[test]
fn variable_resolver_is_prompt() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("is_prompt"), "");
}

#[test]
fn variable_resolver_status_style() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("status_style"), "status");
}

#[test]
fn variable_resolver_cursor_mode() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("cursor_mode"), "buffer");
}

#[test]
fn variable_resolver_is_dark() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    // Default color_context.is_dark is true
    assert_eq!(resolver.resolve("is_dark"), "true");
}

#[test]
fn variable_resolver_session_count() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("session_count"), "0");
}

#[test]
fn variable_resolver_active_session() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("active_session"), "");
}

#[test]
fn variable_dirty_flag_new_variables() {
    assert_eq!(variable_dirty_flag("has_menu"), DirtyFlags::MENU_STRUCTURE);
    assert_eq!(variable_dirty_flag("has_info"), DirtyFlags::INFO);
    assert_eq!(variable_dirty_flag("is_prompt"), DirtyFlags::STATUS);
    assert_eq!(variable_dirty_flag("status_style"), DirtyFlags::STATUS);
    assert_eq!(
        variable_dirty_flag("cursor_mode"),
        DirtyFlags::BUFFER_CURSOR
    );
    assert_eq!(variable_dirty_flag("is_dark"), DirtyFlags::OPTIONS);
    assert_eq!(variable_dirty_flag("session_count"), DirtyFlags::SESSION);
    assert_eq!(variable_dirty_flag("active_session"), DirtyFlags::SESSION);
}

// =============================================================================
// Phase 1E: Variable aliases
// =============================================================================

#[test]
fn variable_alias_filetype() {
    let mut state = AppState::default();
    state
        .ui_options
        .insert("filetype".to_string(), "rust".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("filetype"), "rust");
}

#[test]
fn variable_alias_bufname() {
    let mut state = AppState::default();
    state
        .ui_options
        .insert("bufname".to_string(), "main.rs".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("bufname"), "main.rs");
}

#[test]
fn variable_dirty_flag_aliases() {
    assert_eq!(variable_dirty_flag("filetype"), DirtyFlags::OPTIONS);
    assert_eq!(variable_dirty_flag("bufname"), DirtyFlags::OPTIONS);
}

// =============================================================================
// Phase 2: Gutter widget
// =============================================================================

#[test]
fn parse_gutter_widget() {
    let source = r#"nums kind="gutter" side="left" text="{line_number:4} " face="rgb:888888""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    if let super::types::WidgetKind::Gutter(ref g) = file.widgets[0].kind {
        assert_eq!(g.side, GutterSide::Left);
        assert!(g.face.is_some());
        assert!(g.when.is_none());
        assert!(g.line_when.is_none());
    } else {
        panic!("expected gutter widget");
    }
}

#[test]
fn parse_gutter_minimal() {
    let source = r#"nums kind="gutter" text="{line_number} ""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Gutter(ref g) = file.widgets[0].kind {
        assert_eq!(g.side, GutterSide::Left); // default
        assert!(g.face.is_none());
    } else {
        panic!("expected gutter widget");
    }
}

#[test]
fn parse_gutter_invalid_side() {
    let source = r#"nums kind="gutter" side="top" text="{line_number}""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown gutter side"));
}

#[test]
fn parse_gutter_with_line_when() {
    let source =
        r#"abs kind="gutter" side="left" text="{line_number:3} " line-when="is_cursor_line""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Gutter(ref g) = file.widgets[0].kind {
        assert!(g.line_when.is_some());
    } else {
        panic!("expected gutter widget");
    }
}

#[test]
fn parse_gutter_missing_text() {
    let source = r#"nums kind="gutter" side="left""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
}

#[test]
fn backend_gutter_annotation() {
    let source = r#"nums kind="gutter" side="left" text="{line_number:4} ""#;
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

    // Left gutter should return element
    let result = backend.annotate_gutter(GutterSide::Left, 9, &view, &ctx);
    assert!(result.is_some());
    // Right gutter should not (widget is left)
    let result = backend.annotate_gutter(GutterSide::Right, 9, &view, &ctx);
    assert!(result.is_none());
}

#[test]
fn backend_gutter_line_when_cursor_line() {
    let source = r#"
abs kind="gutter" side="left" text="{line_number:3} " face="rgb:ffffff+b" line-when="is_cursor_line"
"#;
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

    // Cursor line (5) should match
    assert!(
        backend
            .annotate_gutter(GutterSide::Left, 5, &view, &ctx)
            .is_some()
    );
    // Other lines should not
    assert!(
        backend
            .annotate_gutter(GutterSide::Left, 4, &view, &ctx)
            .is_none()
    );
}

#[test]
fn backend_gutter_capabilities() {
    let source = r#"nums kind="gutter" side="left" text="{line_number}""#;
    let backend = WidgetBackend::from_source(source);
    assert!(
        backend
            .capabilities()
            .contains(PluginCapabilities::ANNOTATOR)
    );
    let desc = backend.capability_descriptor().unwrap();
    assert!(
        desc.annotation_scopes
            .contains(&AnnotationScope::LeftGutter)
    );
}

#[test]
fn backend_gutter_global_when_disabled() {
    let source = r#"nums kind="gutter" side="left" text="{line_number}" when="cursor_count > 1""#;
    let backend = WidgetBackend::from_source(source);

    let state = AppState::default(); // cursor_count = 0
    let view = AppView::new(&state);
    let ctx = crate::plugin::AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    // Global when disabled → None for all lines
    assert!(
        backend
            .annotate_gutter(GutterSide::Left, 0, &view, &ctx)
            .is_none()
    );
}

#[test]
fn line_context_resolver_variables() {
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = super::variables::LineContextResolver::new(&view, 9, 5);

    assert_eq!(resolver.resolve("line_number"), "10"); // 1-indexed
    assert_eq!(resolver.resolve("relative_line"), "4"); // |9 - 5|
    assert_eq!(resolver.resolve("is_cursor_line"), ""); // 9 != 5

    let resolver_cursor = super::variables::LineContextResolver::new(&view, 5, 5);
    assert_eq!(resolver_cursor.resolve("is_cursor_line"), "true");
    assert_eq!(resolver_cursor.resolve("relative_line"), "0");
}

#[test]
fn parse_gutter_computed_deps() {
    let source = r#"nums kind="gutter" side="left" text="{line_number}""#;
    let (file, _) = parse_widgets(source).unwrap();
    assert!(file.computed_deps.contains(DirtyFlags::BUFFER_CURSOR));
}

// =============================================================================
// Phase 3A: WrapContainer
// =============================================================================

#[test]
fn parse_transform_default_patch() {
    let source = r#"t kind="transform" target="status" face="default,blue""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert!(matches!(t.patch, super::types::WidgetPatch::ModifyFace(_)));
    } else {
        panic!("expected transform");
    }
}

#[test]
fn parse_transform_explicit_modify_face() {
    let source = r#"t kind="transform" target="status" face="default,blue" patch="modify-face""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert!(matches!(t.patch, super::types::WidgetPatch::ModifyFace(_)));
    } else {
        panic!("expected transform");
    }
}

#[test]
fn parse_transform_wrap_container() {
    let source = r#"wrap kind="transform" target="status" face="default,rgb:1a1a1a" patch="wrap""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert!(matches!(
            t.patch,
            super::types::WidgetPatch::WrapContainer(_)
        ));
    } else {
        panic!("expected transform");
    }
}

#[test]
fn backend_wrap_container_patch() {
    let source = r#"wrap kind="transform" target="status" face="default,rgb:1a1a1a" patch="wrap""#;
    let backend = WidgetBackend::from_source(source);

    let state = AppState::default();
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
    assert!(matches!(
        result.unwrap(),
        crate::plugin::ElementPatch::WrapContainer { .. }
    ));
}

// =============================================================================
// Phase 3B: Additional transform targets
// =============================================================================

#[test]
fn parse_transform_target_menu_prompt() {
    let source = r#"t kind="transform" target="menu-prompt" face="default,blue""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert_eq!(t.target, kasane_plugin_model::TransformTarget::MENU_PROMPT);
    } else {
        panic!("expected transform");
    }
}

#[test]
fn parse_transform_target_menu_inline() {
    let source = r#"t kind="transform" target="menu-inline" face="default,blue""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert_eq!(t.target, kasane_plugin_model::TransformTarget::MENU_INLINE);
    } else {
        panic!("expected transform");
    }
}

#[test]
fn parse_transform_target_menu_search() {
    let source = r#"t kind="transform" target="menu-search" face="default,blue""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert_eq!(t.target, kasane_plugin_model::TransformTarget::MENU_SEARCH);
    } else {
        panic!("expected transform");
    }
}

#[test]
fn parse_transform_target_info_prompt() {
    let source = r#"t kind="transform" target="info-prompt" face="default,blue""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert_eq!(t.target, kasane_plugin_model::TransformTarget::INFO_PROMPT);
    } else {
        panic!("expected transform");
    }
}

#[test]
fn parse_transform_target_info_modal() {
    let source = r#"t kind="transform" target="info-modal" face="default,blue""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert_eq!(t.target, kasane_plugin_model::TransformTarget::INFO_MODAL);
    } else {
        panic!("expected transform");
    }
}

// =============================================================================
// Theme token reference (@token syntax)
// =============================================================================

#[test]
fn parse_face_token_reference() {
    let source = r#"mode slot="status-left" text=" {editor_mode} " face="@status_line""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert!(matches!(
            c.parts[0].face,
            Some(super::types::FaceOrToken::Token(_))
        ));
    } else {
        panic!("expected contribution widget");
    }
}

#[test]
fn parse_face_direct_backward_compat() {
    let source = r#"mode slot="status-left" text=" test " face="red,blue+b""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].kind {
        assert!(matches!(
            c.parts[0].face,
            Some(super::types::FaceOrToken::Direct(_))
        ));
    } else {
        panic!("expected contribution widget");
    }
}

#[test]
fn parse_face_token_in_background() {
    let source = r#"hl kind="background" line="cursor" face="@menu_item_selected""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Background(ref b) = file.widgets[0].kind {
        assert!(matches!(b.face, super::types::FaceOrToken::Token(_)));
    } else {
        panic!("expected background widget");
    }
}

#[test]
fn parse_face_token_in_transform() {
    let source = r#"t kind="transform" target="status" face="@status_line""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].kind {
        assert!(matches!(
            t.patch,
            super::types::WidgetPatch::ModifyFace(super::types::FaceOrToken::Token(_))
        ));
    } else {
        panic!("expected transform widget");
    }
}

#[test]
fn parse_face_token_in_gutter() {
    let source = r#"nums kind="gutter" side="left" text="{line_number}" face="@status_line""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Gutter(ref g) = file.widgets[0].kind {
        assert!(matches!(g.face, Some(super::types::FaceOrToken::Token(_))));
    } else {
        panic!("expected gutter widget");
    }
}

#[test]
fn parse_face_token_empty_name_error() {
    let source = r#"mode slot="status-left" text="test" face="@""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
}

#[test]
fn resolve_face_token_from_theme() {
    use crate::element::StyleToken;
    use crate::protocol::Face;

    let mut state = AppState::default();
    // Set a theme face for "status.line"
    let token = StyleToken::new("status.line");
    let expected_face = Face {
        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
        bg: crate::protocol::Color::Named(crate::protocol::NamedColor::Blue),
        ..Face::default()
    };
    state.theme.set(token.clone(), expected_face);
    let view = AppView::new(&state);

    let fot = super::types::FaceOrToken::Token(token);
    let resolved = super::backend::resolve_face(&fot, &view);
    assert_eq!(resolved, expected_face);
}

#[test]
fn resolve_face_token_missing_returns_default() {
    use crate::element::StyleToken;
    use crate::protocol::Face;

    let state = AppState::default();
    let view = AppView::new(&state);

    let fot = super::types::FaceOrToken::Token(StyleToken::new("nonexistent.token"));
    let resolved = super::backend::resolve_face(&fot, &view);
    assert_eq!(resolved, Face::default());
}

#[test]
fn resolve_face_direct_passthrough() {
    use crate::protocol::Face;

    let state = AppState::default();
    let view = AppView::new(&state);

    let expected = Face {
        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Green),
        ..Face::default()
    };
    let fot = super::types::FaceOrToken::Direct(expected);
    let resolved = super::backend::resolve_face(&fot, &view);
    assert_eq!(resolved, expected);
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
