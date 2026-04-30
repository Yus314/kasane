//! Unit tests for the widget system.

use compact_str::CompactString;

use crate::plugin::{
    AnnotationScope, AppView, ContribSizeHint, ContributeContext, GutterSide, PluginBackend,
    PluginCapabilities, PluginDiagnosticKind, PluginDiagnosticSeverity, SlotId, TransformContext,
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
fn template_padding_default_left_align() {
    let t = Template::parse("{cursor_line:4}").unwrap();
    let resolver = StaticResolver::new(&[("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver), "42  ");
}

#[test]
fn template_padding_right_align() {
    let t = Template::parse("{cursor_line:>4}").unwrap();
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
    let vars = t.referenced_variables();
    assert_eq!(vars, &["x", "y"]);
}

#[test]
fn template_no_variables() {
    let t = Template::parse("just text").unwrap();
    let vars = t.referenced_variables();
    assert!(vars.is_empty());
}

// =============================================================================
// Template: nested conditionals (Phase 1 — depth-aware brace parser)
// =============================================================================

#[test]
fn template_conditional_with_variable_in_then() {
    // {?is_focused => {cursor_line} => N/A}
    let t = Template::parse("{?is_focused => {cursor_line} => N/A}").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "true"), ("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver), "42");
    let resolver2 = StaticResolver::new(&[("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver2), "N/A");
}

#[test]
fn template_conditional_with_formatted_variable_in_then() {
    // {?is_focused => {cursor_line:>4} => ---}
    let t = Template::parse("{?is_focused => {cursor_line:>4} => ---}").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "true"), ("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver), "  42");
    let resolver2 = StaticResolver::new(&[("cursor_line", "42")]);
    assert_eq!(t.expand(&resolver2), "---");
}

#[test]
fn template_nested_conditional() {
    // {?is_focused => active => {?has_menu => menu => buffer}}
    let t = Template::parse("{?is_focused => active => {?has_menu => menu => buffer}}").unwrap();
    // is_focused=true → "active"
    let r1 = StaticResolver::new(&[("is_focused", "true")]);
    assert_eq!(t.expand(&r1), "active");
    // is_focused=false, has_menu=true → "menu"
    let r2 = StaticResolver::new(&[("has_menu", "true")]);
    assert_eq!(t.expand(&r2), "menu");
    // is_focused=false, has_menu=false → "buffer"
    let r3 = StaticResolver::new(&[]);
    assert_eq!(t.expand(&r3), "buffer");
}

#[test]
fn template_conditional_comparison_with_variable_branches() {
    // {?editor_mode == 'insert' => {cursor_col} => {cursor_line}}
    let t = Template::parse("{?editor_mode == 'insert' => {cursor_col} => {cursor_line}}").unwrap();
    let r1 = StaticResolver::new(&[
        ("editor_mode", "insert"),
        ("cursor_col", "5"),
        ("cursor_line", "10"),
    ]);
    assert_eq!(t.expand(&r1), "5");
    let r2 = StaticResolver::new(&[
        ("editor_mode", "normal"),
        ("cursor_col", "5"),
        ("cursor_line", "10"),
    ]);
    assert_eq!(t.expand(&r2), "10");
}

#[test]
fn template_nested_conditional_referenced_variables() {
    let t = Template::parse("{?is_focused => {cursor_line} => N/A}").unwrap();
    let mut vars = t.referenced_variables();
    vars.sort();
    assert_eq!(vars, &["cursor_line", "is_focused"]);
}

#[test]
fn template_conditional_with_colon_in_else() {
    // Colons in branches are now allowed since => is the separator
    let t = Template::parse("{?is_focused => active => 12:34}").unwrap();
    let r_false = StaticResolver::new(&[]);
    assert_eq!(t.expand(&r_false), "12:34");
    let r_true = StaticResolver::new(&[("is_focused", "true")]);
    assert_eq!(t.expand(&r_true), "active");
}

#[test]
fn template_conditional_with_url_in_branch() {
    let t = Template::parse("{?is_focused => https://example.com => N/A}").unwrap();
    let r = StaticResolver::new(&[("is_focused", "true")]);
    assert_eq!(t.expand(&r), "https://example.com");
}

#[test]
fn template_conditional_missing_arrow_is_error() {
    // No => separator should fail
    assert!(Template::parse("{?is_focused}").is_err());
}

// =============================================================================
// Condition tests
// =============================================================================

#[test]
fn cond_truthy() {
    let expr = parse_condition("is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "true")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_falsy_empty() {
    let expr = parse_condition("is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_falsy_zero() {
    let expr = parse_condition("is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "0")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_eq() {
    let expr = parse_condition("editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("editor_mode", "insert")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_ne() {
    let expr = parse_condition("editor_mode != 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("editor_mode", "normal")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_gt_numeric() {
    let expr = parse_condition("cursor_count > 1").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "3")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_lt_numeric() {
    let expr = parse_condition("cursor_count < 2").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "1")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_and() {
    let expr = parse_condition("cursor_count > 1 && editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "3"), ("editor_mode", "insert")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_and_false() {
    let expr = parse_condition("cursor_count > 1 && editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "1"), ("editor_mode", "insert")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_or() {
    let expr = parse_condition("cursor_count > 1 || editor_mode == 'insert'").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "1"), ("editor_mode", "insert")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_not() {
    let expr = parse_condition("!is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_not_truthy() {
    let expr = parse_condition("!is_focused").unwrap();
    let resolver = StaticResolver::new(&[("is_focused", "true")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
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
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_le() {
    let expr = parse_condition("cursor_count <= 2").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "3")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

// =============================================================================
// Condition: =~ and in operators (Phase 3b)
// =============================================================================

#[test]
fn cond_regex_match() {
    let expr = parse_condition("filetype =~ 'rs|go|py'").unwrap();
    let resolver = StaticResolver::new(&[("filetype", "rs")]);
    assert!(expr.evaluate_with_resolver(&resolver));
    let resolver2 = StaticResolver::new(&[("filetype", "java")]);
    assert!(!expr.evaluate_with_resolver(&resolver2));
}

#[test]
fn cond_regex_no_match() {
    let expr = parse_condition("filetype =~ '^rust$'").unwrap();
    let resolver = StaticResolver::new(&[("filetype", "rs")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_regex_invalid() {
    let result = parse_condition("filetype =~ '(invalid['");
    assert!(result.is_err());
}

#[test]
fn cond_in_set() {
    let expr = parse_condition("filetype in ('rust', 'go', 'python')").unwrap();
    let resolver = StaticResolver::new(&[("filetype", "rust")]);
    assert!(expr.evaluate_with_resolver(&resolver));
    let resolver2 = StaticResolver::new(&[("filetype", "java")]);
    assert!(!expr.evaluate_with_resolver(&resolver2));
}

#[test]
fn cond_in_empty_set() {
    let expr = parse_condition("filetype in ()").unwrap();
    let resolver = StaticResolver::new(&[("filetype", "rust")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_in_numeric() {
    let expr = parse_condition("cursor_count in (1, 2, 3)").unwrap();
    let resolver = StaticResolver::new(&[("cursor_count", "2")]);
    assert!(expr.evaluate_with_resolver(&resolver));
    let resolver2 = StaticResolver::new(&[("cursor_count", "5")]);
    assert!(!expr.evaluate_with_resolver(&resolver2));
}

#[test]
fn cond_regex_referenced_variables() {
    let expr = parse_condition("filetype =~ 'rs|go'").unwrap();
    let vars = expr.referenced_variables();
    assert_eq!(vars, &["filetype"]);
}

#[test]
fn cond_in_referenced_variables() {
    let expr = parse_condition("filetype in ('rust', 'go')").unwrap();
    let vars = expr.referenced_variables();
    assert_eq!(vars, &["filetype"]);
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
        file.widgets[0].effects[0].kind,
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
        assert_eq!(c.parts.len(), 2);
        assert!(!c.parts[0].face_rules.is_empty());
        assert!(c.parts[1].face_rules.is_empty());
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
        file.widgets[0].effects[0].kind,
        super::types::WidgetKind::Background(_)
    ));
}

#[test]
fn parse_transform_widget() {
    let source = r#"insert-status kind="transform" target="status" face="default,blue" when="editor_mode == 'insert'""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    assert_eq!(file.widgets.len(), 1);
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    state.observed.cursor_pos = crate::protocol::Coord { line: 9, column: 4 };
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
    state.inference.cursor_count = 3;
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
    state.observed.cursor_pos = crate::protocol::Coord { line: 5, column: 0 };
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
    state.inference.editor_mode = crate::state::derived::EditorMode::Insert;
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
    use super::types::Value;
    let mut state = AppState::default();
    state.observed.cursor_pos = crate::protocol::Coord { line: 9, column: 4 };
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("cursor_line"), Value::Int(10)); // 1-indexed
    assert_eq!(resolver.resolve("cursor_col"), Value::Int(5)); // 1-indexed
}

#[test]
fn variable_resolver_editor_mode() {
    use super::types::Value;
    let mut state = AppState::default();
    state.inference.editor_mode = crate::state::derived::EditorMode::Insert;
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("editor_mode"), Value::Str("insert".into()));
}

#[test]
fn variable_resolver_unknown() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("nonexistent"), Value::Empty);
}

#[test]
fn variable_resolver_opt() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("filetype".to_string(), "rust".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.filetype"), Value::Str("rust".into()));
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
    if let super::types::WidgetKind::Background(ref b) = file.widgets[0].effects[0].kind {
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
    state.inference.selections = vec![crate::state::derived::Selection {
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
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
    use super::types::Value;
    let state = AppState::default(); // no menu
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("has_menu"), Value::Bool(false));
}

#[test]
fn variable_resolver_has_info() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("has_info"), Value::Bool(false));
}

#[test]
fn variable_resolver_is_prompt() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("is_prompt"), Value::Bool(false));
}

#[test]
fn variable_resolver_status_style() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(
        resolver.resolve("status_style"),
        Value::Str("status".into())
    );
}

#[test]
fn variable_resolver_cursor_mode() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("cursor_mode"), Value::Str("buffer".into()));
}

#[test]
fn variable_resolver_is_dark() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    // Default color_context.is_dark is true
    assert_eq!(resolver.resolve("is_dark"), Value::Bool(true));
}

#[test]
fn variable_resolver_session_count() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("session_count"), Value::Int(0));
}

#[test]
fn variable_resolver_active_session() {
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("active_session"), Value::Empty);
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
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("filetype".to_string(), "rust".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("filetype"), Value::Str("rust".into()));
}

#[test]
fn variable_alias_bufname() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("bufname".to_string(), "main.rs".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("bufname"), Value::Str("main.rs".into()));
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
    if let super::types::WidgetKind::Gutter(ref g) = file.widgets[0].effects[0].kind {
        assert_eq!(g.side, GutterSide::Left);
        assert_eq!(g.branches.len(), 1);
        assert!(!g.branches[0].face_rules.is_empty());
        assert!(g.when.is_none());
        assert!(g.branches[0].line_when.is_none());
    } else {
        panic!("expected gutter widget");
    }
}

#[test]
fn parse_gutter_minimal() {
    let source = r#"nums kind="gutter" text="{line_number} ""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Gutter(ref g) = file.widgets[0].effects[0].kind {
        assert_eq!(g.side, GutterSide::Left); // default
        assert_eq!(g.branches.len(), 1);
        assert!(g.branches[0].face_rules.is_empty());
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
    if let super::types::WidgetKind::Gutter(g) = &file.widgets[0].effects[0].kind {
        assert_eq!(g.branches.len(), 1);
        assert!(g.branches[0].line_when.is_some());
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
    state.observed.cursor_pos = crate::protocol::Coord { line: 5, column: 0 };
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
    state.observed.cursor_pos = crate::protocol::Coord { line: 5, column: 0 };
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
    use super::types::Value;
    let state = AppState::default();
    let view = AppView::new(&state);
    let resolver = super::variables::LineContextResolver::new(&view, 9, 5);

    assert_eq!(resolver.resolve("line_number"), Value::Int(10)); // 1-indexed
    assert_eq!(resolver.resolve("relative_line"), Value::Int(4)); // |9 - 5|
    assert_eq!(resolver.resolve("is_cursor_line"), Value::Bool(false)); // 9 != 5

    let resolver_cursor = super::variables::LineContextResolver::new(&view, 5, 5);
    assert_eq!(resolver_cursor.resolve("is_cursor_line"), Value::Bool(true));
    assert_eq!(resolver_cursor.resolve("relative_line"), Value::Int(0));
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
        assert_eq!(c.parts[0].face_rules.len(), 1);
        assert!(matches!(
            c.parts[0].face_rules[0].face,
            super::types::FaceOrToken::Token(_)
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
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
        assert_eq!(c.parts[0].face_rules.len(), 1);
        assert!(matches!(
            c.parts[0].face_rules[0].face,
            super::types::FaceOrToken::Direct(_)
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
    if let super::types::WidgetKind::Background(ref b) = file.widgets[0].effects[0].kind {
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
    if let super::types::WidgetKind::Transform(ref t) = file.widgets[0].effects[0].kind {
        if let super::types::WidgetPatch::ModifyFace(ref rules) = t.patch {
            assert_eq!(rules.len(), 1);
            assert!(matches!(rules[0].face, super::types::FaceOrToken::Token(_)));
        } else {
            panic!("expected ModifyFace patch");
        }
    } else {
        panic!("expected transform widget");
    }
}

#[test]
fn parse_face_token_in_gutter() {
    let source = r#"nums kind="gutter" side="left" text="{line_number}" face="@status_line""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty());
    if let super::types::WidgetKind::Gutter(g) = &file.widgets[0].effects[0].kind {
        assert_eq!(g.branches.len(), 1);
        assert!(matches!(
            g.branches[0].face_rules.first(),
            Some(super::types::FaceRule {
                face: super::types::FaceOrToken::Token(_),
                ..
            })
        ));
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
    use crate::protocol::WireFace;

    let mut state = AppState::default();
    // Set a theme face for "status.line"
    let token = StyleToken::new("status.line");
    let expected_face = WireFace {
        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
        bg: crate::protocol::Color::Named(crate::protocol::NamedColor::Blue),
        ..WireFace::default()
    };
    state
        .config
        .theme
        .set_style(token.clone(), expected_face.into());
    let view = AppView::new(&state);

    let fot = super::types::FaceOrToken::Token(token);
    let resolved = super::backend::resolve_face(&fot, &view);
    assert_eq!(resolved, expected_face);
}

#[test]
fn resolve_face_token_missing_returns_default() {
    use crate::element::StyleToken;
    use crate::protocol::WireFace;

    let state = AppState::default();
    let view = AppView::new(&state);

    let fot = super::types::FaceOrToken::Token(StyleToken::new("nonexistent.token"));
    let resolved = super::backend::resolve_face(&fot, &view);
    assert_eq!(resolved, WireFace::default());
}

#[test]
fn resolve_face_direct_passthrough() {
    use crate::protocol::WireFace;

    let state = AppState::default();
    let view = AppView::new(&state);

    let expected = WireFace {
        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Green),
        ..WireFace::default()
    };
    let fot = super::types::FaceOrToken::Direct(expected);
    let resolved = super::backend::resolve_face(&fot, &view);
    assert_eq!(resolved, expected);
}

// =============================================================================
// Condition parentheses tests
// =============================================================================

#[test]
fn cond_paren_simple_group() {
    // (a || b) && c — without parens, would be a || (b && c)
    let expr = parse_condition("(a || b) && c").unwrap();
    let resolver = StaticResolver::new(&[("a", ""), ("b", "true"), ("c", "true")]);
    assert!(expr.evaluate_with_resolver(&resolver));
    // With c false, should be false
    let resolver = StaticResolver::new(&[("a", ""), ("b", "true"), ("c", "")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_paren_nested() {
    let expr = parse_condition("((a || b) && (c || d))").unwrap();
    let resolver = StaticResolver::new(&[("a", "true"), ("b", ""), ("c", ""), ("d", "true")]);
    assert!(expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_paren_not_group() {
    let expr = parse_condition("!(a || b)").unwrap();
    let resolver = StaticResolver::new(&[("a", ""), ("b", "")]);
    assert!(expr.evaluate_with_resolver(&resolver));
    let resolver = StaticResolver::new(&[("a", "true"), ("b", "")]);
    assert!(!expr.evaluate_with_resolver(&resolver));
}

#[test]
fn cond_paren_unclosed() {
    let result = parse_condition("(a || b");
    assert!(result.is_err());
    assert!(matches!(
        result,
        Err(super::condition::CondParseError::UnclosedParen)
    ));
}

// =============================================================================
// Template format extension tests
// =============================================================================

#[test]
fn template_left_align() {
    let t = Template::parse("{x:<10}").unwrap();
    let resolver = StaticResolver::new(&[("x", "hi")]);
    assert_eq!(t.expand(&resolver), "hi        ");
}

#[test]
fn template_truncation() {
    let t = Template::parse("{x:.5}").unwrap();
    let resolver = StaticResolver::new(&[("x", "hello world")]);
    assert_eq!(t.expand(&resolver), "hell\u{2026}");
}

#[test]
fn template_truncation_no_op_short() {
    let t = Template::parse("{x:.10}").unwrap();
    let resolver = StaticResolver::new(&[("x", "hi")]);
    assert_eq!(t.expand(&resolver), "hi");
}

#[test]
fn template_left_align_with_truncation() {
    let t = Template::parse("{x:<10.5}").unwrap();
    let resolver = StaticResolver::new(&[("x", "hello world")]);
    // Truncated to 5 chars (4 + ellipsis), then left-aligned to 10
    let result = t.expand(&resolver);
    assert_eq!(result, "hell\u{2026}     ");
}

#[test]
fn template_default_align_is_left() {
    // Default: {name:width} = left-align
    let t = Template::parse("{x:6}").unwrap();
    let resolver = StaticResolver::new(&[("x", "hi")]);
    assert_eq!(t.expand(&resolver), "hi    ");
}

#[test]
fn template_explicit_right_align() {
    // {name:>width} = right-align
    let t = Template::parse("{x:>6}").unwrap();
    let resolver = StaticResolver::new(&[("x", "hi")]);
    assert_eq!(t.expand(&resolver), "    hi");
}

// =============================================================================
// Unknown variable detection tests
// =============================================================================

#[test]
fn validate_known_variable() {
    use super::variables::validate_variable;
    assert!(validate_variable("cursor_line", false).is_none());
    assert!(validate_variable("editor_mode", false).is_none());
    assert!(validate_variable("is_focused", false).is_none());
}

#[test]
fn validate_unknown_with_suggestion() {
    use super::variables::validate_variable;
    let result = validate_variable("cursor_lint", false);
    assert!(result.is_some());
    let msg = result.unwrap();
    assert!(msg.contains("did you mean"));
    assert!(msg.contains("cursor_line"));
}

#[test]
fn validate_opt_prefix_always_valid() {
    use super::variables::validate_variable;
    assert!(validate_variable("opt.filetype", false).is_none());
    assert!(validate_variable("opt.some_custom_thing", false).is_none());
}

#[test]
fn validate_line_var_in_gutter_context() {
    use super::variables::validate_variable;
    assert!(validate_variable("line_number", true).is_none());
    assert!(validate_variable("relative_line", true).is_none());
    assert!(validate_variable("is_cursor_line", true).is_none());
}

#[test]
fn validate_line_var_outside_gutter_context() {
    use super::variables::validate_variable;
    let result = validate_variable("line_number", false);
    assert!(result.is_some());
    assert!(result.unwrap().contains("only available in gutter"));
}

#[test]
fn parse_unknown_variable_warning() {
    let source = r#"mode slot="status-left" text=" {cursor_lint} ""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 1);
    assert!(
        !errors.is_empty(),
        "expected variable warning for cursor_lint"
    );
    assert!(errors[0].message.contains("cursor_lin"));
}

// =============================================================================
// widgets {} block tests
// =============================================================================

#[test]
fn unified_widgets_block() {
    let source = r#"
ui { shadow #false }
widgets {
    mode slot="status-left" text=" {editor_mode} "
    cursorline kind="background" line="cursor" face="default,rgb:303030"
}
"#;
    let (config, _, file, errors) = crate::config::unified::parse_unified(source).unwrap();
    assert!(!config.ui.shadow);
    assert_eq!(file.widgets.len(), 2);
    // No errors — widgets are inside the block
    assert!(errors.is_empty());
}

#[test]
fn unified_flat_widgets_rejected() {
    let source = r#"
ui { shadow #false }
mode slot="status-left" text=" {editor_mode} "
"#;
    let result = crate::config::unified::parse_unified(source);
    let err = match result {
        Err(e) => e.to_string(),
        Ok(_) => panic!("flat top-level widgets should be rejected"),
    };
    assert!(
        err.contains("mode"),
        "error should mention the offending node name"
    );
}

#[test]
fn unified_mixed_widgets_block_and_flat_rejected() {
    let source = r#"
widgets {
    a slot="status-left" text="A"
}
b slot="status-right" text="B"
"#;
    let result = crate::config::unified::parse_unified(source);
    let err = match result {
        Err(e) => e.to_string(),
        Ok(_) => panic!("flat top-level widgets mixed with block should be rejected"),
    };
    assert!(
        err.contains("b"),
        "error should mention the offending node name"
    );
}

#[test]
fn unified_unknown_top_level_suggests_typo() {
    let source = r#"
widgts {
    a slot="status-left" text="A"
}
"#;
    let result = crate::config::unified::parse_unified(source);
    let err = match result {
        Err(e) => e.to_string(),
        Ok(_) => panic!("typo section should be rejected"),
    };
    assert!(
        err.contains("did you mean 'widgets'"),
        "should suggest closest match, got: {err}"
    );
}

// =============================================================================
// Phase 3a: Unicode width in template formatting
// =============================================================================

#[test]
fn template_padding_cjk_characters() {
    // "日本語" is 6 display columns wide, format to 10 → 4 spaces left-padded
    let t = Template::parse("{val:10}").unwrap();
    let resolver = StaticResolver::new(&[("val", "日本語")]);
    assert_eq!(t.expand(&resolver), "日本語    ");
}

#[test]
fn template_padding_ascii_unchanged() {
    // "hello" is 5 display columns, format to 10 → 5 spaces left-padded
    let t = Template::parse("{val:10}").unwrap();
    let resolver = StaticResolver::new(&[("val", "hello")]);
    assert_eq!(t.expand(&resolver), "hello     ");
}

#[test]
fn template_truncation_cjk() {
    // "日本語テスト" is 12 columns, truncate to 7 → "日本語…" (6+1=7)
    let t = Template::parse("{val:.7}").unwrap();
    let resolver = StaticResolver::new(&[("val", "日本語テスト")]);
    assert_eq!(t.expand(&resolver), "日本語…");
}

#[test]
fn template_left_align_cjk() {
    // "日本" is 4 columns, format to 8 left-aligned → "日本    "
    let t = Template::parse("{val:<8}").unwrap();
    let resolver = StaticResolver::new(&[("val", "日本")]);
    assert_eq!(t.expand(&resolver), "日本    ");
}

// =============================================================================
// Phase 1c: CondParseError::TooLong
// =============================================================================

#[test]
fn cond_too_long_returns_too_long_error() {
    use super::condition::CondParseError;
    let long_expr = "a".repeat(257);
    assert_eq!(parse_condition(&long_expr), Err(CondParseError::TooLong));
}

#[test]
fn cond_at_max_length_succeeds() {
    // 256 chars should parse (if valid expression)
    let expr = format!("{:<256}", "x");
    assert!(parse_condition(expr.trim()).is_ok());
}

// =============================================================================
// Phase 1b: duplicate widget name warning
// =============================================================================

#[test]
fn parse_duplicate_widget_name_produces_warning() {
    let source = r#"
a slot="status-left" text="first"
a slot="status-right" text="second"
"#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 2);
    let dup_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("duplicate widget name"))
        .collect();
    assert_eq!(dup_errors.len(), 1);
    assert!(dup_errors[0].message.contains("'a'"));
}

// =============================================================================
// Phase 1a: opt.* typed resolution
// =============================================================================

#[test]
fn variable_resolver_opt_numeric_string_becomes_int() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("tabstop".to_string(), "0".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.tabstop"), Value::Int(0));
}

#[test]
fn variable_resolver_opt_positive_int() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("tabstop".to_string(), "42".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.tabstop"), Value::Int(42));
}

#[test]
fn variable_resolver_opt_true_becomes_bool() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("autoreload".to_string(), "true".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.autoreload"), Value::Bool(true));
}

#[test]
fn variable_resolver_opt_false_becomes_bool() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("autoreload".to_string(), "false".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.autoreload"), Value::Bool(false));
}

#[test]
fn variable_resolver_opt_string_stays_str() {
    use super::types::Value;
    let mut state = AppState::default();
    state
        .observed
        .ui_options
        .insert("filetype".to_string(), "rust".to_string());
    let view = AppView::new(&state);
    let resolver = AppViewResolver::new(&view);
    assert_eq!(resolver.resolve("opt.filetype"), Value::Str("rust".into()));
}

// =============================================================================
// Fuzzy suggestion for slots and targets
// =============================================================================

#[test]
fn parse_unknown_slot_suggests_close_match() {
    let source = r#"x slot="status_left" text="hi""#;
    let (_, errors) = parse_widgets(source).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.contains("did you mean 'status-left'"),
        "got: {}",
        errors[0].message
    );
}

#[test]
fn parse_unknown_slot_no_suggestion_for_unrelated() {
    let source = r#"x slot="foobar" text="hi""#;
    let (_, errors) = parse_widgets(source).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.contains("unknown slot: 'foobar'"),
        "got: {}",
        errors[0].message
    );
    assert!(
        !errors[0].message.contains("did you mean"),
        "should not suggest for unrelated input: {}",
        errors[0].message
    );
}

#[test]
fn parse_unknown_target_suggests_close_match() {
    let source = r#"x kind="transform" target="statusbar" face="red""#;
    let (_, errors) = parse_widgets(source).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.contains("did you mean 'status-bar'"),
        "got: {}",
        errors[0].message
    );
}

#[test]
fn parse_unknown_target_no_suggestion_for_unrelated() {
    let source = r#"x kind="transform" target="zzzzz" face="red""#;
    let (_, errors) = parse_widgets(source).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(
        !errors[0].message.contains("did you mean"),
        "should not suggest for unrelated input: {}",
        errors[0].message
    );
}

// =============================================================================
// Bool(false) display
// =============================================================================

#[test]
fn bool_false_displays_as_false_string() {
    use super::types::Value;
    assert_eq!(Value::Bool(false).to_display().as_str(), "false");
}

// =============================================================================
// Group syntax
// =============================================================================

#[test]
fn group_inherits_when_condition() {
    let source = r#"
        group when="is_focused" {
            a slot="status-left" text="A"
            b slot="status-right" text="B"
        }
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 2);
    assert_eq!(file.widgets[0].name.as_str(), "a");
    assert_eq!(file.widgets[1].name.as_str(), "b");
    // Both should have the group's when condition
    assert!(file.widgets[0].when.is_some());
    assert!(file.widgets[1].when.is_some());
}

#[test]
fn group_merges_with_widget_when() {
    // Single-effect widget: widget's own when is in the effect, group when is in WidgetDef.when.
    let source = r#"
        group when="is_focused" {
            a slot="status-left" text="A" when="has_menu"
        }
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    // Group condition becomes WidgetDef.when
    assert!(file.widgets[0].when.is_some());
    // Widget's own when="has_menu" is inside the contribution effect
    if let super::types::WidgetKind::Contribution(ref c) = file.widgets[0].effects[0].kind {
        assert!(c.when.is_some(), "contribution should have its own when");
    } else {
        panic!("expected contribution");
    }
}

#[test]
fn group_merges_with_multi_effect_widget_when() {
    // Multi-effect widget: both group and widget have when= → AND merge.
    let source = r#"
        group when="is_focused" {
            a when="has_menu" {
                contribution slot="status-left" text="A"
                background line="cursor" face="default,rgb:303030"
            }
        }
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    // Should be And(is_focused, has_menu)
    let when = file.widgets[0].when.as_ref().unwrap();
    assert!(
        matches!(when, super::predicate::Predicate::And(_, _)),
        "expected And, got: {when:?}"
    );
}

#[test]
fn group_without_when_is_passthrough() {
    let source = r#"
        group {
            a slot="status-left" text="A"
        }
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    assert!(file.widgets[0].when.is_none());
}

#[test]
fn nested_groups() {
    let source = r#"
        group when="is_focused" {
            group when="has_menu" {
                a slot="status-left" text="A"
            }
        }
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    // Should be And(is_focused, has_menu)
    let when = file.widgets[0].when.as_ref().unwrap();
    assert!(
        matches!(when, super::predicate::Predicate::And(_, _)),
        "expected And, got: {when:?}"
    );
}

#[test]
fn group_empty_children_error() {
    let source = r#"group when="is_focused""#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("no children"));
}

#[test]
fn group_preserves_widget_ordering() {
    let source = r#"
        before slot="status-left" text="1"
        group when="is_focused" {
            middle slot="status-left" text="2"
        }
        after slot="status-left" text="3"
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 3);
    assert_eq!(file.widgets[0].name.as_str(), "before");
    assert_eq!(file.widgets[0].index, 0);
    assert_eq!(file.widgets[1].name.as_str(), "middle");
    assert_eq!(file.widgets[1].index, 1);
    assert_eq!(file.widgets[2].name.as_str(), "after");
    assert_eq!(file.widgets[2].index, 2);
}

// =============================================================================
// Widget order= attribute
// =============================================================================

#[test]
fn bare_bool_when_true_is_noop() {
    // KDL v2 uses #true for booleans
    let source = "a slot=\"status-left\" text=\"A\" when=#true";
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets.len(), 1);
    // when=#true is a no-op — widget is always active, no condition needed
    assert!(file.widgets[0].when.is_none());
}

#[test]
fn bare_bool_when_false_is_error() {
    let source = "a slot=\"status-left\" text=\"A\" when=#false";
    let (file, errors) = parse_widgets(source).unwrap();
    assert_eq!(file.widgets.len(), 0);
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].message.contains("permanently disabled"),
        "got: {}",
        errors[0].message
    );
}

#[test]
fn order_attribute_overrides_file_order() {
    let source = r#"
        a slot="status-left" text="A" order=10
        b slot="status-left" text="B" order=-5
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert_eq!(file.widgets[0].priority(), 10);
    assert_eq!(file.widgets[1].priority(), -5);
}

#[test]
fn order_attribute_absent_uses_index() {
    let source = r#"
        a slot="status-left" text="A"
        b slot="status-left" text="B"
    "#;
    let (file, errors) = parse_widgets(source).unwrap();
    assert!(errors.is_empty(), "errors: {errors:?}");
    assert!(file.widgets[0].order.is_none());
    assert_eq!(file.widgets[0].priority(), 0);
    assert_eq!(file.widgets[1].priority(), 1);
}

#[test]
fn bool_true_displays_as_true_string() {
    use super::types::Value;
    assert_eq!(Value::Bool(true).to_display().as_str(), "true");
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
    fn resolve(&self, name: &str) -> super::types::Value {
        for (k, v) in self.vars {
            if *k == name {
                // Try to interpret as typed value for richer testing:
                // integers are returned as Value::Int, "true"/"" as Bool, rest as Str
                if let Ok(n) = v.parse::<i64>() {
                    return super::types::Value::Int(n);
                }
                if *v == "true" {
                    return super::types::Value::Bool(true);
                }
                if v.is_empty() {
                    return super::types::Value::Empty;
                }
                return super::types::Value::Str(CompactString::from(*v));
            }
        }
        super::types::Value::Empty
    }
}

#[test]
fn node_error_to_diagnostic_uses_config_error_kind() {
    let error = super::parse::WidgetNodeError {
        name: "my-widget".to_string(),
        message: "unknown slot 'foo'".to_string(),
    };
    let diag = super::backend::node_error_to_diagnostic(&error);
    assert_eq!(diag.severity(), PluginDiagnosticSeverity::Warning);
    assert!(
        matches!(diag.kind, PluginDiagnosticKind::ConfigError { ref key } if key == "my-widget")
    );
    assert_eq!(diag.message, "unknown slot 'foo'");
}
