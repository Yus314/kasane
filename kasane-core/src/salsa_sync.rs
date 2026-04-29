//! Projection from `AppState` to Salsa inputs (Layer 1 → Layer 2 boundary).
//!
//! `sync_inputs_from_state()` is called once per frame, after all events
//! in the batch have been processed. It unconditionally sets all Salsa inputs
//! from the current AppState. Salsa's `set_*().to()` uses PartialEq to
//! detect unchanged values and skip downstream revalidation automatically.

use salsa::{Durability, Setter};

use crate::plugin::ContributionCache;
use crate::plugin::{AppView, PluginView};
use crate::salsa_db::KasaneDatabase;
use crate::salsa_inputs::*;
use crate::state::AppState;
use crate::state::snapshot::{InfoSnapshot, MenuSnapshot};

/// Handles to Salsa input instances, created once at startup and reused across frames.
///
/// Also owns the [`ContributionCache`] used by `sync_plugin_contributions()`
/// to avoid redundant `contribute_to()` calls for non-stale plugins.
pub struct SalsaInputHandles {
    pub buffer: BufferInput,
    pub cursor: CursorInput,
    pub status: StatusInput,
    pub menu: MenuInput,
    pub info: InfoInput,
    pub config: ConfigInput,
    pub display_directives: DisplayDirectivesInput,
    pub slot_contributions: SlotContributionsInput,
    pub annotations: AnnotationResultInput,
    pub plugin_overlays: PluginOverlaysInput,
    pub transform_patches: TransformPatchesInput,
    pub content_annotations: ContentAnnotationsInput,
    contribution_cache: ContributionCache,
}

impl SalsaInputHandles {
    /// Create all Salsa input instances with default values.
    pub fn new(db: &mut KasaneDatabase) -> Self {
        Self {
            buffer: BufferInput::new(
                db,
                vec![],
                crate::protocol::Style::default(),
                crate::protocol::Style::default(),
                crate::protocol::Coord::default(),
                0,
            ),
            cursor: CursorInput::new(db, crate::protocol::CursorMode::Buffer, 0, vec![]),
            status: StatusInput::new(
                db,
                vec![],
                vec![],
                -1,
                vec![],
                vec![],
                crate::protocol::Style::default(),
                crate::protocol::StatusStyle::default(),
            ),
            menu: MenuInput::new(db, None),
            info: InfoInput::new(db, vec![]),
            config: ConfigInput::new(
                db,
                80,
                24,
                true,
                true,
                false,
                0.4,
                crate::config::MenuPosition::Auto,
                false,
                "█".to_string(),
                "░".to_string(),
                None,
            ),
            display_directives: DisplayDirectivesInput::new(db, vec![], 0),
            slot_contributions: SlotContributionsInput::new(
                db,
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ),
            annotations: AnnotationResultInput::new(db, None, None, None, None, None),
            plugin_overlays: PluginOverlaysInput::new(db, vec![]),
            transform_patches: TransformPatchesInput::new(db, None, None),
            content_annotations: ContentAnnotationsInput::new(db, vec![]),
            contribution_cache: ContributionCache::default(),
        }
    }

    /// Remove cached contributions for a plugin (e.g., after unloading).
    pub fn remove_plugin_cache(&mut self, plugin_id: &crate::plugin::PluginId) {
        self.contribution_cache.remove_plugin(plugin_id);
    }
}

/// Drain unloaded plugin IDs from the registry and clean up their
/// contribution caches. Call this before `sync_plugin_contributions()`.
pub fn cleanup_unloaded_plugins(
    registry: &mut crate::plugin::PluginRuntime,
    inputs: &mut SalsaInputHandles,
) {
    for id in registry.drain_unloaded_ids() {
        inputs.remove_plugin_cache(&id);
    }
}

/// Project all `AppState` fields onto Salsa inputs.
///
/// Salsa's `set_*().to()` compares via PartialEq and only bumps the
/// revision when the value actually changed, so unconditional sync
/// is safe and avoids manual DirtyFlags tracking.
pub fn sync_inputs_from_state(
    db: &mut KasaneDatabase,
    state: &AppState,
    inputs: &SalsaInputHandles,
) {
    // Buffer content
    inputs.buffer.set_lines(db).to(state.observed.lines.clone());
    inputs
        .buffer
        .set_default_style(db)
        .to(state.observed.default_style.clone());
    inputs
        .buffer
        .set_padding_style(db)
        .to(state.observed.padding_style.clone());
    inputs
        .buffer
        .set_widget_columns(db)
        .to(state.observed.widget_columns);

    // Cursor
    inputs
        .buffer
        .set_cursor_pos(db)
        .to(state.observed.cursor_pos);
    inputs
        .cursor
        .set_cursor_mode(db)
        .to(state.inference.cursor_mode);
    inputs
        .cursor
        .set_cursor_count(db)
        .to(state.inference.cursor_count);
    inputs
        .cursor
        .set_secondary_cursors(db)
        .to(state.inference.secondary_cursors.clone());

    // Status: observed components first, then the derived concatenation.
    inputs
        .status
        .set_status_prompt(db)
        .to(state.observed.status_prompt.clone());
    inputs
        .status
        .set_status_content(db)
        .to(state.observed.status_content.clone());
    inputs
        .status
        .set_status_content_cursor_pos(db)
        .to(state.observed.status_content_cursor_pos);
    inputs
        .status
        .set_status_line(db)
        .to(state.inference.status_line.clone());
    inputs
        .status
        .set_status_mode_line(db)
        .to(state.observed.status_mode_line.clone());
    inputs
        .status
        .set_status_default_style(db)
        .to(state.observed.status_default_style.clone());
    inputs
        .status
        .set_status_style(db)
        .to(state.observed.status_style);

    // Menu
    let snapshot = state
        .observed
        .menu
        .as_ref()
        .map(MenuSnapshot::from_menu_state);
    inputs.menu.set_menu(db).to(snapshot);

    // Info
    let snapshots: Vec<_> = state
        .observed
        .infos
        .iter()
        .map(InfoSnapshot::from_info_state)
        .collect();
    inputs.info.set_infos(db).to(snapshots);

    // Config / options
    inputs
        .config
        .set_cols(db)
        .with_durability(Durability::HIGH)
        .to(state.runtime.cols);
    inputs
        .config
        .set_rows(db)
        .with_durability(Durability::HIGH)
        .to(state.runtime.rows);
    inputs.config.set_focused(db).to(state.runtime.focused);
    inputs
        .config
        .set_shadow_enabled(db)
        .with_durability(Durability::HIGH)
        .to(state.config.shadow_enabled);
    inputs
        .config
        .set_status_at_top(db)
        .with_durability(Durability::HIGH)
        .to(state.config.status_at_top);
    inputs
        .config
        .set_secondary_blend_ratio(db)
        .with_durability(Durability::HIGH)
        .to(state.config.secondary_blend_ratio);
    inputs
        .config
        .set_menu_position(db)
        .with_durability(Durability::HIGH)
        .to(state.config.menu_position);
    inputs
        .config
        .set_search_dropdown(db)
        .with_durability(Durability::HIGH)
        .to(state.config.search_dropdown);
    inputs
        .config
        .set_scrollbar_thumb(db)
        .with_durability(Durability::HIGH)
        .to(state.config.scrollbar_thumb.clone());
    inputs
        .config
        .set_scrollbar_track(db)
        .with_durability(Durability::HIGH)
        .to(state.config.scrollbar_track.clone());
    inputs
        .config
        .set_assistant_art(db)
        .with_durability(Durability::HIGH)
        .to(state.config.assistant_art.clone());
}

/// Synchronize plugin contributions (slots, annotations, overlays) into Salsa inputs.
///
/// Call this after `prepare_plugin_cache()` and `sync_display_directives()`.
/// Uses per-extension-point granularity: only re-collects extension points
/// where at least one participating plugin needs recollection. The
/// [`ContributionCache`] inside `inputs` provides per-plugin caching across frames.
pub fn sync_plugin_contributions(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &mut SalsaInputHandles,
) {
    use crate::display::DisplayMapRef;
    use crate::element::Overlay;
    use crate::plugin::{
        AnnotateContext, ContribSizeHint, ContributeContext, Contribution, OverlayContext, SlotId,
    };
    use std::sync::Arc;

    fn contribution_to_flex_child(c: Contribution) -> crate::element::FlexChild {
        match c.size_hint {
            ContribSizeHint::Auto => crate::element::FlexChild::fixed(c.element),
            ContribSizeHint::Fixed(n) => crate::element::FlexChild {
                element: c.element,
                flex: 0.0,
                min_size: Some(n),
                max_size: Some(n),
            },
            ContribSizeHint::Flex(flex) => crate::element::FlexChild::flexible(c.element, flex),
        }
    }

    fn collect_slot_cached(
        slot: &SlotId,
        state: &AppState,
        registry: &PluginView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
    ) -> Vec<crate::element::FlexChild> {
        registry
            .collect_contributions_cached(slot, &AppView::new(state), ctx, cache)
            .into_iter()
            .map(contribution_to_flex_child)
            .collect()
    }

    // Slot contributions: only re-collect if any contributor is stale
    if registry.any_contributor_needs_recollect() {
        let view = AppView::new(state);
        let ctx = ContributeContext::new(&view, None);
        let cache = &mut inputs.contribution_cache;
        let buffer_left = collect_slot_cached(&SlotId::BUFFER_LEFT, state, registry, &ctx, cache);
        let buffer_right = collect_slot_cached(&SlotId::BUFFER_RIGHT, state, registry, &ctx, cache);
        let above_buffer = collect_slot_cached(&SlotId::ABOVE_BUFFER, state, registry, &ctx, cache);
        let below_buffer = collect_slot_cached(&SlotId::BELOW_BUFFER, state, registry, &ctx, cache);
        let status_left = collect_slot_cached(&SlotId::STATUS_LEFT, state, registry, &ctx, cache);
        let status_right = collect_slot_cached(&SlotId::STATUS_RIGHT, state, registry, &ctx, cache);
        let above_status = collect_slot_cached(&SlotId::ABOVE_STATUS, state, registry, &ctx, cache);
        inputs
            .slot_contributions
            .set_buffer_left(db)
            .to(buffer_left);
        inputs
            .slot_contributions
            .set_buffer_right(db)
            .to(buffer_right);
        inputs
            .slot_contributions
            .set_above_buffer(db)
            .to(above_buffer);
        inputs
            .slot_contributions
            .set_below_buffer(db)
            .to(below_buffer);
        inputs
            .slot_contributions
            .set_status_left(db)
            .to(status_left);
        inputs
            .slot_contributions
            .set_status_right(db)
            .to(status_right);
        inputs
            .slot_contributions
            .set_above_status(db)
            .to(above_status);
    }

    // Annotations: only re-collect if any annotator is stale
    if registry.any_annotator_needs_recollect() {
        let display_map: DisplayMapRef =
            crate::salsa_views::display_map_query(db, inputs.display_directives);
        let annotate_ctx = AnnotateContext {
            line_width: state.runtime.cols,
            gutter_width: 0,
            display_map: Some(Arc::clone(&display_map)),
            pane_surface_id: None,
            pane_focused: true,
        };
        let result = registry.collect_annotations(&AppView::new(state), &annotate_ctx);
        inputs
            .annotations
            .set_line_backgrounds(db)
            .to(result.line_backgrounds);
        inputs
            .annotations
            .set_left_gutter(db)
            .to(result.left_gutter);
        inputs
            .annotations
            .set_right_gutter(db)
            .to(result.right_gutter);
        inputs
            .annotations
            .set_inline_decorations(db)
            .to(result.inline_decorations);
        inputs
            .annotations
            .set_virtual_text(db)
            .to(result.virtual_text);
    }

    // Plugin overlays: only re-collect if any overlay provider is stale
    if registry.any_overlay_needs_recollect() {
        let overlay_ctx = OverlayContext {
            screen_cols: state.runtime.cols,
            screen_rows: state.runtime.rows,
            menu_rect: crate::layout::get_menu_rect(state),
            existing_overlays: vec![],
            focused_surface_id: None,
        };
        let overlays: Vec<Overlay> = registry
            .collect_overlays_with_ctx(&AppView::new(state), &overlay_ctx)
            .into_iter()
            .map(|oc| Overlay {
                element: oc.element,
                anchor: oc.anchor,
            })
            .collect();
        inputs.plugin_overlays.set_overlays(db).to(overlays);
    }
}

/// Synchronize display directives from plugins into Salsa.
///
/// Call this after `prepare_plugin_cache()`.
/// Only re-collects if any DISPLAY_TRANSFORM plugin needs recollection.
pub fn sync_display_directives(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &SalsaInputHandles,
) {
    if !registry.any_display_transform_needs_recollect() {
        return;
    }

    let directives = registry.collect_display_directives(&AppView::new(state));
    let line_count = state.visible_line_range().len();

    inputs.display_directives.set_directives(db).to(directives);
    inputs
        .display_directives
        .set_buffer_line_count(db)
        .to(line_count);
}

/// Synchronize content annotations from plugins into Salsa.
///
/// Call this after `sync_plugin_contributions()` (which sets up the display map
/// and annotations). Only re-collects if any CONTENT_ANNOTATOR plugin needs
/// recollection.
pub fn sync_content_annotations(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &SalsaInputHandles,
) {
    use crate::display::DisplayMapRef;
    use crate::plugin::AnnotateContext;
    use std::sync::Arc;

    if !registry.has_capability(crate::plugin::PluginCapabilities::CONTENT_ANNOTATOR) {
        return;
    }

    let display_map: DisplayMapRef =
        crate::salsa_views::display_map_query(db, inputs.display_directives);
    let annotate_ctx = AnnotateContext {
        line_width: state.runtime.cols,
        gutter_width: 0,
        display_map: Some(Arc::clone(&display_map)),
        pane_surface_id: None,
        pane_focused: true,
    };
    let annotations = registry.collect_content_annotations(&AppView::new(state), &annotate_ctx);
    inputs
        .content_annotations
        .set_annotations(db)
        .to(annotations);
}

/// Unified display synchronization: collects spatial directives, annotations,
/// and content annotations in a single coordinated pass.
///
/// For plugins that use `has_unified_display()`, this ensures `unified_display()`
/// is called only once (via lazy caching in `PluginView`), with the result
/// partitioned across the spatial, annotation, and content annotation Salsa inputs.
///
/// Call this after `prepare_plugin_cache()` instead of calling
/// `sync_display_directives()`, `sync_plugin_contributions()`, and
/// `sync_content_annotations()` separately.
pub fn sync_unified_display(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &mut SalsaInputHandles,
) {
    // Step 1: Spatial directives (display map depends on these)
    sync_display_directives(db, state, registry, inputs);

    // Step 2: Annotations (depends on display map from step 1)
    sync_plugin_contributions(db, state, registry, inputs);

    // Step 3: Content annotations (depends on display map)
    sync_content_annotations(db, state, registry, inputs);
}

/// Synchronize transform patches from TRANSFORMER plugins into Salsa.
///
/// Collects patches for Buffer and StatusBar targets. When all patches for a
/// target are pure, stores the composed result as a Salsa input (enabling
/// PartialEq-based memoization). When any patch is impure or legacy, stores
/// `None` to signal that the render pipeline should fall back to imperative
/// `apply_transform_chain()`.
pub fn sync_transform_patches(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &SalsaInputHandles,
) {
    use crate::plugin::TransformTarget;

    let app_view = AppView::new(state);

    let buffer = registry.collect_transform_patches(TransformTarget::BUFFER, &app_view);
    inputs.transform_patches.set_buffer(db).to(buffer);

    let status_bar = registry.collect_transform_patches(TransformTarget::STATUS_BAR, &app_view);
    inputs.transform_patches.set_status_bar(db).to(status_bar);
}
