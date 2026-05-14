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
    /// ADR-035 §2 — handle to the configured `HistoryBackend`.
    /// Synced from `AppState::history` each frame; backs the
    /// Time-aware Salsa queries (`text_at_time`, `selection_at_time`,
    /// `display_directives_at_time`).
    pub history: HistoryInput,
    contribution_cache: ContributionCache,
}

impl SalsaInputHandles {
    /// Create all Salsa input instances with default values.
    pub fn new(db: &mut KasaneDatabase) -> Self {
        Self {
            buffer: BufferInput::new(
                db,
                std::sync::Arc::new(vec![]),
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
            history: HistoryInput::new(
                db,
                std::sync::Arc::new(crate::history::InMemoryRing::new()),
                crate::history::VersionId::INITIAL,
            ),
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
    // ADR-035 §2 — history backend (point at AppState's ring) and
    // current_version (so Time::Now-resolving queries invalidate
    // when the auto-commit hook bumps the version).
    use crate::history::HistoryBackend;
    inputs.history.set_backend(db).to(state.history.clone());
    inputs
        .history
        .set_current_version(db)
        .to(state.history.current_version());

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
///
/// `dirty` is forwarded to `collect_contributions_cached` so per-slot
/// freshness can be evaluated against both the plugin's revision input and
/// the frame's `DirtyFlags ∩ view_deps()` — replacing the gate previously
/// pre-computed into `slot.needs_recollect`.
pub fn sync_plugin_contributions(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &mut SalsaInputHandles,
    dirty: crate::state::DirtyFlags,
) {
    use crate::display::DisplayMapRef;
    use crate::plugin::{
        AnnotateContext, ContribSizeHint, ContributeContext, Contribution, SlotId,
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
        dirty: crate::state::DirtyFlags,
    ) -> Vec<crate::element::FlexChild> {
        registry
            .collect_contributions_cached(slot, &AppView::new(state), ctx, cache, dirty)
            .into_iter()
            .map(contribution_to_flex_child)
            .collect()
    }

    // Slot contributions: only re-collect if any contributor is stale
    if registry.any_contributor_needs_recollect() {
        // Collect with a fresh inner scope so the contribution-cache mut
        // borrow ends before the surrounding Salsa input setter calls.
        let (
            buffer_left,
            buffer_right,
            above_buffer,
            below_buffer,
            status_left,
            status_right,
            above_status,
        ) = {
            let view = AppView::new(state);
            let ctx = ContributeContext::new(&view, None);
            let cache = &mut inputs.contribution_cache;
            (
                collect_slot_cached(&SlotId::BUFFER_LEFT, state, registry, &ctx, cache, dirty),
                collect_slot_cached(&SlotId::BUFFER_RIGHT, state, registry, &ctx, cache, dirty),
                collect_slot_cached(&SlotId::ABOVE_BUFFER, state, registry, &ctx, cache, dirty),
                collect_slot_cached(&SlotId::BELOW_BUFFER, state, registry, &ctx, cache, dirty),
                collect_slot_cached(&SlotId::STATUS_LEFT, state, registry, &ctx, cache, dirty),
                collect_slot_cached(&SlotId::STATUS_RIGHT, state, registry, &ctx, cache, dirty),
                collect_slot_cached(&SlotId::ABOVE_STATUS, state, registry, &ctx, cache, dirty),
            )
        };
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
        // Wrap per-line lists in `Arc` so the pipeline reader can share the
        // allocation across frames with `Arc::clone` instead of paying for a
        // fresh `Vec` deep-clone every frame (`pipeline_salsa.rs`).
        inputs
            .annotations
            .set_line_backgrounds(db)
            .to(result.line_backgrounds.map(Arc::new));
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
            .to(result.inline_decorations.map(Arc::new));
        inputs
            .annotations
            .set_virtual_text(db)
            .to(result.virtual_text.map(Arc::new));
    }

    // Plugin overlays: moved inline into pipeline_salsa.rs (θ-spike). The
    // Salsa input wrapper provided no structural value because no tracked
    // query depended on it.
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

/// Unified display synchronization: collects spatial directives and
/// annotations in a single coordinated pass.
///
/// For plugins that use `has_unified_display()`, this ensures `unified_display()`
/// is called only once (via lazy caching in `PluginView`), with the result
/// partitioned across the spatial and annotation Salsa inputs.
///
/// Content annotations are collected inline at render time (θ.3); no
/// sync step needed.
///
/// Call this after `prepare_plugin_cache()` instead of calling
/// `sync_display_directives()` and `sync_plugin_contributions()` separately.
pub fn sync_unified_display(
    db: &mut KasaneDatabase,
    state: &AppState,
    registry: &PluginView<'_>,
    inputs: &mut SalsaInputHandles,
    dirty: crate::state::DirtyFlags,
) {
    // Step 1: Spatial directives (display map depends on these)
    sync_display_directives(db, state, registry, inputs);

    // Step 2: Annotations (depends on display map from step 1)
    sync_plugin_contributions(db, state, registry, inputs, dirty);
}
