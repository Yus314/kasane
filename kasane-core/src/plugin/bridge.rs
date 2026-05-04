//! `PluginBridge` — adapts `Plugin` to the internal `PluginBackend` trait.
//!
//! Dispatches `PluginBackend` methods through a [`HandlerTable`] built from
//! `Plugin::register()`. State changes are tracked via a generation counter
//! for L1 cache invalidation.

use std::any::Any;

use crate::element::{Element, InteractiveId, Overlay, PluginTag};
use crate::input::{CompiledKeyMap, DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::extension_point::{ExtensionDefinition, ExtensionOutput, ExtensionPointId};
use super::handler_registry::HandlerRegistry;
use super::handler_table::HandlerTable;
use super::io::ProcessEvent;
use super::process_task::{
    ProcessTaskFeedResult, ProcessTaskHandle, collect_fallbacks, spawn_command,
};
use super::pubsub::TopicBus;
use super::state::{Plugin, PluginState};
use super::{
    AnnotateContext, AppView, BackgroundLayer, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, ElementPatch, GutterSide, IoEvent, KeyHandleResult, LineAnnotation,
    OverlayContext, OverlayContribution, PluginAuthorities, PluginBackend, PluginCapabilities,
    PluginDiagnostic, PluginId, SlotId, TransformContext, TransformDescriptor, TransformSubject,
    TransformTarget, VirtualTextItem,
};

/// Adapts a [`Plugin`] to the internal [`PluginBackend`] trait via data-driven dispatch.
///
/// Construction calls `P::register()` to capture a [`HandlerTable`], then all
/// `PluginBackend` methods dispatch through the table's erased handlers.
/// State changes are tracked via a generation counter for L1 cache invalidation.
pub struct PluginBridge {
    id: PluginId,
    table: HandlerTable,
    state: Box<dyn PluginState>,
    generation: u64,
    /// Snapshot of the last state observed by `check_state_change`.
    /// Cloned via `dyn_clone` whenever a change is detected. Pays one
    /// state-clone per real mutation in exchange for not requiring
    /// `Hash` on plugin-state types — `HashMap` and other non-`Hash`
    /// collections become legal as plugin state without boilerplate.
    prev_state: Box<dyn PluginState>,
    plugin_tag: PluginTag,
    /// Active process tasks managed by the framework.
    active_process_tasks: Vec<ProcessTaskHandle>,
    /// Job ID counter for process tasks (framework-managed, avoids collisions).
    next_task_job_id: u64,
    /// Pending diagnostics to be drained on the next `drain_diagnostics()` call.
    pending_diagnostics: Vec<PluginDiagnostic>,
    /// Cached projection descriptors from the handler table.
    cached_projection_descriptors: Vec<crate::display::ProjectionDescriptor>,
}

impl PluginBridge {
    /// Create a new bridge from a `Plugin`, calling `register()` to build the handler table.
    pub fn new<P: Plugin>(plugin: P) -> Self {
        let id = plugin.id();
        let mut registry = HandlerRegistry::<P::State>::new();
        plugin.register(&mut registry);
        let table = registry.into_table();
        let cached_projection_descriptors: Vec<_> = table
            .projection_entries
            .iter()
            .map(|e| e.descriptor.clone())
            .collect();
        let state: Box<dyn PluginState> = Box::new(P::State::default());
        let prev_state = state.clone();
        PluginBridge {
            id,
            table,
            state,
            generation: 0,
            prev_state,
            plugin_tag: PluginTag::UNASSIGNED,
            active_process_tasks: Vec::new(),
            // Start at a high offset to avoid collisions with manually managed job IDs.
            next_task_job_id: 0x8000_0000_0000_0000,
            pending_diagnostics: Vec::new(),
            cached_projection_descriptors,
        }
    }

    /// Attach initial diagnostics to be drained on the next `drain_diagnostics()` call.
    pub fn with_diagnostics(mut self, diagnostics: Vec<PluginDiagnostic>) -> Self {
        self.pending_diagnostics = diagnostics;
        self
    }

    /// Compare current state with the previous snapshot; bump generation if
    /// changed and refresh the snapshot.
    ///
    /// Uses [`PluginState::dyn_eq`] for an exact compare — no false negatives
    /// from hash collisions. Pays one state-clone per real mutation.
    fn check_state_change(&mut self) {
        // Deref through the trait object so dispatch goes via the inner
        // type's vtable. `self.state.dyn_eq(...)` would otherwise resolve
        // to the blanket `impl<T> PluginState for T` on `Box<dyn
        // PluginState>` itself (which now satisfies the relaxed bound),
        // causing the downcast to fail and every comparison to report
        // "not equal".
        if !(*self.state).dyn_eq(&*self.prev_state) {
            self.generation += 1;
            self.prev_state = self.state.clone();
        }
    }

    /// Try to route a process event through active task handles.
    ///
    /// Returns `Some(effects)` if a task handle matched the event (including
    /// fallback respawn). Returns `None` if no task handle matched, so the
    /// caller should fall through to the manual `io_event_handler`.
    fn try_process_task_event(
        &mut self,
        proc_event: &ProcessEvent,
        app: &AppView<'_>,
    ) -> Option<Effects> {
        // Find the matching active task handle.
        let idx = self.active_process_tasks.iter().position(|h| {
            let job_id = match proc_event {
                ProcessEvent::Stdout { job_id, .. }
                | ProcessEvent::Stderr { job_id, .. }
                | ProcessEvent::Exited { job_id, .. }
                | ProcessEvent::SpawnFailed { job_id, .. } => *job_id,
            };
            h.job_id == job_id
        })?;

        // Determine if this task is streaming.
        let task_name = self.active_process_tasks[idx].name;
        let streaming = self
            .table
            .process_tasks
            .iter()
            .find(|e| e.name == task_name)
            .is_some_and(|e| e.streaming);

        let feed_result = self.active_process_tasks[idx].feed(proc_event, streaming);

        match feed_result {
            ProcessTaskFeedResult::Pending => Some(Effects::default()),
            ProcessTaskFeedResult::Deliver(result) => {
                // Terminal events (Completed, Failed) remove the handle.
                let is_terminal = matches!(
                    result,
                    super::process_task::ProcessTaskResult::Completed { .. }
                        | super::process_task::ProcessTaskResult::Failed(_)
                );

                // Look up the handler and invoke it.
                let effects = if let Some(entry) = self
                    .table
                    .process_tasks
                    .iter()
                    .find(|e| e.name == task_name)
                {
                    let (new_state, effects) = (entry.handler)(&*self.state, &result, app);
                    self.state = new_state;
                    self.check_state_change();
                    effects
                } else {
                    Effects::default()
                };

                if is_terminal {
                    self.active_process_tasks.remove(idx);
                }

                Some(effects)
            }
            ProcessTaskFeedResult::TryFallback(fallback_spec) => {
                // Respawn with the fallback. Allocate a new job ID.
                let new_job_id = self.next_task_job_id;
                self.next_task_job_id += 1;
                self.active_process_tasks[idx].job_id = new_job_id;
                self.active_process_tasks[idx].stdout_buf.clear();

                let cmd = spawn_command(&fallback_spec, new_job_id);
                Some(Effects::with(vec![cmd]))
            }
            ProcessTaskFeedResult::Ignored => None,
        }
    }
}

/// Recursively walk an Element tree, replacing `InteractiveId` with `owner == UNASSIGNED`
/// to the given `plugin_tag`.
fn inject_owner(element: &mut Element, tag: PluginTag) {
    match element {
        Element::Interactive { id, child } => {
            if id.owner == PluginTag::UNASSIGNED {
                id.owner = tag;
            }
            inject_owner(child, tag);
        }
        Element::Flex { children, .. } | Element::ResolvedSlot { children, .. } => {
            for c in children {
                inject_owner(&mut c.element, tag);
            }
        }
        Element::Stack { base, overlays } => {
            inject_owner(base, tag);
            for o in overlays {
                inject_owner(&mut o.element, tag);
            }
        }
        Element::Container { child, .. } | Element::Scrollable { child, .. } => {
            inject_owner(child, tag);
        }
        Element::Grid { children, .. } => {
            for c in children {
                inject_owner(c, tag);
            }
        }
        _ => {} // Text, StyledLine, Empty, Image, BufferRef, SlotPlaceholder
    }
}

fn inject_owner_in_patch(patch: &mut super::ElementPatch, tag: PluginTag) {
    match patch {
        super::ElementPatch::Prepend { element }
        | super::ElementPatch::Append { element }
        | super::ElementPatch::Replace { element } => {
            inject_owner(element, tag);
        }
        super::ElementPatch::Compose(patches) => {
            for p in patches {
                inject_owner_in_patch(p, tag);
            }
        }
        _ => {} // Identity, WrapContainer, ModifyFace, ModifyAnchor, Custom
    }
}

/// Dispatch a state+effects handler: call handler, update state, check change, return effects.
macro_rules! dispatch_state_effect {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            let (new_state, effects) = handler(&*$self.state, $($arg),*);
            $self.state = new_state;
            $self.check_state_change();
            effects
        } else {
            Effects::default()
        }
    };
}

/// Dispatch a state-only handler: call handler, update state, check change.
macro_rules! dispatch_state_only {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            $self.state = handler(&*$self.state, $($arg),*);
            $self.check_state_change();
        }
    };
}

/// Dispatch an optional-consume handler: call handler, update state if Some, return mapped result.
macro_rules! dispatch_optional_consume {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            handler(&*$self.state, $($arg),*).map(|(new_state, result)| {
                $self.state = new_state;
                $self.check_state_change();
                result
            })
        } else {
            None
        }
    };
}

/// Dispatch an immutable view handler, returning the default if not registered.
macro_rules! dispatch_view_or {
    ($self:expr, $field:ident, $default:expr $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            handler(&*$self.state, $($arg),*)
        } else {
            $default
        }
    };
}

/// Dispatch an immutable view handler, returning Option via `and_then`.
macro_rules! dispatch_view_option {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        $self.table.$field.as_ref().and_then(|h| h(&*$self.state, $($arg),*))
    };
}

impl PluginBackend for PluginBridge {
    fn id(&self) -> PluginId {
        self.id.clone()
    }

    fn set_plugin_tag(&mut self, tag: PluginTag) {
        self.plugin_tag = tag;
    }

    fn capabilities(&self) -> PluginCapabilities {
        self.table.capabilities()
    }

    fn authorities(&self) -> PluginAuthorities {
        PluginAuthorities::empty()
    }

    fn suppressed_builtins(&self) -> &std::collections::HashSet<super::BuiltinTarget> {
        &self.table.suppressed_builtins
    }

    fn allows_process_spawn(&self) -> bool {
        true
    }

    fn state_hash(&self) -> u64 {
        self.generation
    }

    fn view_deps(&self) -> DirtyFlags {
        self.table.interests()
    }

    fn transform_priority(&self) -> i16 {
        self.table
            .transform_handler
            .as_ref()
            .map_or(0, |e| e.priority)
    }

    fn transform_descriptor(&self) -> Option<TransformDescriptor> {
        self.table.transform_handler.as_ref().and_then(|entry| {
            if entry.targets.is_empty() {
                None
            } else {
                Some(TransformDescriptor {
                    targets: entry.targets.clone(),
                    scope: super::TransformScope::Structural,
                })
            }
        })
    }

    fn display_directive_priority(&self) -> i16 {
        0
    }

    // === Lifecycle ===

    fn on_init_effects(&mut self, app: &AppView<'_>) -> Effects {
        dispatch_state_effect!(self, init_handler, app)
    }

    fn on_active_session_ready_effects(&mut self, app: &AppView<'_>) -> Effects {
        dispatch_state_effect!(self, session_ready_handler, app)
    }

    fn on_shutdown(&mut self) {
        if let Some(handler) = &self.table.shutdown_handler {
            handler(&*self.state);
        }
    }

    fn on_state_changed_effects(&mut self, app: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        dispatch_state_effect!(self, state_changed_handler, app, dirty)
    }

    fn on_io_event_effects(&mut self, event: &IoEvent, app: &AppView<'_>) -> Effects {
        // Route process events through active task handles first.
        if let IoEvent::Process(proc_event) = event
            && let Some(effects) = self.try_process_task_event(proc_event, app)
        {
            return effects;
        }

        // Fall through to the manual io_event handler.
        dispatch_state_effect!(self, io_event_handler, event, app)
    }

    fn intercept_buffer_edit(
        &mut self,
        edit: &crate::state::shadow_cursor::BufferEdit,
        app: &AppView<'_>,
    ) -> crate::state::shadow_cursor::BufferEditVerdict {
        if let Some(handler) = &self.table.buffer_edit_intercept_handler {
            let (new_state, verdict) = handler(&*self.state, edit, app);
            self.state = new_state;
            self.check_state_change();
            verdict
        } else {
            crate::state::shadow_cursor::BufferEditVerdict::PassThrough
        }
    }

    fn on_workspace_changed(&mut self, query: &WorkspaceQuery<'_>) {
        dispatch_state_only!(self, workspace_changed_handler, query);
    }

    fn workspace_save(&self) -> Option<serde_json::Value> {
        self.table
            .workspace_save_handler
            .as_ref()
            .and_then(|h| h(&*self.state))
    }

    fn workspace_restore(&mut self, data: &serde_json::Value) {
        dispatch_state_only!(self, workspace_restore_handler, data);
    }

    fn start_process_task(&mut self, name: &str) -> Vec<Command> {
        let Some(entry) = self.table.process_tasks.iter().find(|e| e.name == name) else {
            tracing::warn!(plugin = self.id.0.as_str(), name, "unknown process task");
            return vec![];
        };

        let job_id = self.next_task_job_id;
        self.next_task_job_id += 1;

        let fallbacks = collect_fallbacks(&entry.spec);
        let cmd = spawn_command(&entry.spec, job_id);
        self.active_process_tasks
            .push(ProcessTaskHandle::new(entry.name, job_id, fallbacks));

        vec![cmd]
    }

    // === Input ===

    fn observe_key(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_key_handler, key, app);
    }

    fn observe_text_input(&mut self, text: &str, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_text_input_handler, text, app);
    }

    fn observe_mouse(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_mouse_handler, event, app);
    }

    fn observe_drop(&mut self, event: &DropEvent, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_drop_handler, event, app);
    }

    fn handle_key(&mut self, key: &KeyEvent, app: &AppView<'_>) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, key_handler, key, app)
    }

    fn handle_text_input(&mut self, text: &str, app: &AppView<'_>) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, text_input_handler, text, app)
    }

    fn handle_key_middleware(&mut self, key: &KeyEvent, app: &AppView<'_>) -> KeyHandleResult {
        if let Some(handler) = &self.table.key_middleware_handler {
            let (new_state, result) = handler(&*self.state, key, app);
            self.state = new_state;
            self.check_state_change();
            result
        } else {
            // Fall back to handle_key (mirrors PluginBackend default)
            match self.handle_key(key, app) {
                Some(commands) => KeyHandleResult::Consumed(commands),
                None => KeyHandleResult::Passthrough,
            }
        }
    }

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, handle_mouse_handler, event, id, app)
    }

    fn handle_drop(
        &mut self,
        event: &DropEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, handle_drop_handler, event, id, app)
    }

    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        dispatch_optional_consume!(self, default_scroll_handler, candidate, app)
    }

    fn compiled_key_map(&self) -> Option<&CompiledKeyMap> {
        self.table.key_map.as_ref()
    }

    fn invoke_action(&mut self, action_id: &str, key: &KeyEvent, app: &AppView<'_>) -> KeyResponse {
        if let Some(handler) = &self.table.action_handler {
            let (new_state, response) = handler(&*self.state, action_id, key, app);
            self.state = new_state;
            self.check_state_change();
            response
        } else {
            KeyResponse::Pass
        }
    }

    fn refresh_key_groups(&mut self, app: &AppView<'_>) {
        if let Some(handler) = &self.table.group_refresh_handler
            && let Some(map) = &mut self.table.key_map
        {
            handler(&*self.state, app, map);
        }
    }

    fn update_effects(&mut self, msg: &mut dyn Any, app: &AppView<'_>) -> Effects {
        dispatch_state_effect!(self, update_handler, msg, app)
    }

    // === View contributions ===

    fn contribute_to(
        &self,
        region: &SlotId,
        app: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        for entry in &self.table.contribute_handlers {
            if entry.slot == *region {
                return (entry.handler)(&*self.state, app, ctx).map(|mut c| {
                    inject_owner(&mut c.element, self.plugin_tag);
                    c
                });
            }
        }
        None
    }

    fn transform_patch(
        &self,
        target: &TransformTarget,
        app: &AppView<'_>,
        ctx: &TransformContext,
    ) -> Option<ElementPatch> {
        self.table.transform_handler.as_ref().map(|entry| {
            let mut patch = (entry.handler)(&*self.state, target, app, ctx);
            inject_owner_in_patch(&mut patch, self.plugin_tag);
            patch
        })
    }

    fn transform(
        &self,
        target: &TransformTarget,
        subject: TransformSubject,
        app: &AppView<'_>,
        ctx: &TransformContext,
    ) -> TransformSubject {
        if let Some(patch) = self.transform_patch(target, app, ctx) {
            patch.apply(subject)
        } else {
            subject
        }
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        if !self.table.has_annotation_handlers() {
            return None;
        }

        let background = self
            .table
            .background_handler
            .as_ref()
            .and_then(|h| h(&*self.state, line, app, ctx));

        let inline = self
            .table
            .inline_handler
            .as_ref()
            .and_then(|h| h(&*self.state, line, app, ctx));

        let virtual_text: Vec<VirtualTextItem> = self
            .table
            .virtual_text_handler
            .as_ref()
            .map(|h| h(&*self.state, line, app, ctx))
            .unwrap_or_default();

        let mut left_gutter = None;
        let mut right_gutter = None;
        let mut priority = 0i16;

        for entry in &self.table.gutter_handlers {
            if let Some(el) = (entry.handler)(&*self.state, line, app, ctx) {
                match entry.side {
                    super::handler_table::GutterSide::Left => left_gutter = Some(el),
                    super::handler_table::GutterSide::Right => right_gutter = Some(el),
                }
                priority = entry.priority;
            }
        }

        if background.is_some()
            || inline.is_some()
            || !virtual_text.is_empty()
            || left_gutter.is_some()
            || right_gutter.is_some()
        {
            Some(LineAnnotation {
                left_gutter,
                right_gutter,
                background,
                priority,
                inline,
                virtual_text,
            })
        } else {
            None
        }
    }

    fn has_decomposed_annotations(&self) -> bool {
        true
    }

    fn decorate_gutter(
        &self,
        side: GutterSide,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<(i16, crate::element::Element)> {
        for entry in &self.table.gutter_handlers {
            if entry.side == side
                && let Some(mut el) = (entry.handler)(&*self.state, line, app, ctx)
            {
                inject_owner(&mut el, self.plugin_tag);
                return Some((entry.priority, el));
            }
        }
        None
    }

    fn decorate_background(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<BackgroundLayer> {
        dispatch_view_option!(self, background_handler, line, app, ctx)
    }

    fn decorate_inline(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<crate::render::InlineDecoration> {
        dispatch_view_option!(self, inline_handler, line, app, ctx)
    }

    fn annotate_virtual_text(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Vec<VirtualTextItem> {
        dispatch_view_or!(self, virtual_text_handler, vec![], line, app, ctx)
    }

    fn compute_display_scroll_offset(
        &self,
        cursor_display_y: usize,
        viewport_height: usize,
        default_offset: usize,
        state: &AppView<'_>,
    ) -> Option<usize> {
        dispatch_view_option!(
            self,
            display_scroll_offset_handler,
            cursor_display_y,
            viewport_height,
            default_offset,
            state
        )
    }

    fn render_menu_overlay(
        &self,
        state: &AppView<'_>,
        _view: &super::PluginView<'_>,
    ) -> Option<Overlay> {
        dispatch_view_option!(self, menu_renderer_handler, state)
    }

    fn render_info_overlays(
        &self,
        state: &AppView<'_>,
        avoid: &[crate::layout::Rect],
        _view: &super::PluginView<'_>,
    ) -> Option<Vec<Overlay>> {
        dispatch_view_option!(self, info_renderer_handler, state, avoid)
    }

    fn contribute_overlay_with_ctx(
        &self,
        app: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        if let Some(handler) = &self.table.overlay_handler {
            handler(&*self.state, app, ctx).map(|mut c| {
                inject_owner(&mut c.element, self.plugin_tag);
                c
            })
        } else {
            None
        }
    }

    fn render_ornaments(
        &self,
        app: &AppView<'_>,
        ctx: &super::RenderOrnamentContext,
    ) -> super::OrnamentBatch {
        dispatch_view_or!(
            self,
            render_ornament_handler,
            super::OrnamentBatch::default(),
            app,
            ctx
        )
    }

    fn paint_inline_box(&self, box_id: u64, app: &AppView<'_>) -> Option<Element> {
        dispatch_view_or!(self, inline_box_paint_handler, None, box_id, app)
    }

    fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        app: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        dispatch_view_or!(
            self,
            menu_transform_handler,
            None,
            item,
            index,
            selected,
            app
        )
    }

    fn content_annotations(
        &self,
        state: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Vec<crate::display::ContentAnnotation> {
        dispatch_view_or!(self, content_annotation_handler, vec![], state, ctx)
    }

    fn display_directives(&self, app: &AppView<'_>) -> Vec<DisplayDirective> {
        dispatch_view_or!(self, display_handler, vec![], app)
    }

    fn has_unified_display(&self) -> bool {
        self.table.unified_display_handler.is_some()
    }

    fn unified_display(&self, app: &AppView<'_>) -> Vec<DisplayDirective> {
        dispatch_view_or!(self, unified_display_handler, vec![], app)
    }

    fn projection_descriptors(&self) -> &[crate::display::ProjectionDescriptor] {
        &self.cached_projection_descriptors
    }

    fn projection_directives(
        &self,
        id: &crate::display::ProjectionId,
        state: &AppView<'_>,
    ) -> Vec<DisplayDirective> {
        for entry in &self.table.projection_entries {
            if &entry.descriptor.id == id {
                return (entry.handler)(&*self.state, state);
            }
        }
        vec![]
    }

    fn navigation_policy(
        &self,
        unit: &crate::display::unit::DisplayUnit,
    ) -> Option<crate::display::navigation::NavigationPolicy> {
        self.table
            .navigation_policy_handler
            .as_ref()
            .map(|h| h(&*self.state, unit))
    }

    fn navigation_action(
        &mut self,
        unit: &crate::display::unit::DisplayUnit,
        action: crate::display::navigation::NavigationAction,
    ) -> Option<crate::display::navigation::ActionResult> {
        let handler = self.table.navigation_action_handler.as_ref()?;
        let (new_state, result) = handler(&*self.state, unit, action);
        self.state = new_state;
        self.check_state_change();
        match result {
            crate::display::navigation::ActionResult::Pass => None,
            other => Some(other),
        }
    }

    fn collect_publications(&self, bus: &mut TopicBus, state: &AppView<'_>) {
        let plugin_id = self.id.clone();
        for entry in &self.table.publishers {
            let value = (entry.handler)(&*self.state, state);
            bus.publish(entry.topic.clone(), plugin_id.clone(), value);
        }
    }

    fn deliver_subscriptions(&mut self, bus: &TopicBus) -> bool {
        let mut changed = false;
        for entry in &self.table.subscribers {
            if let Some(publications) = bus.get_publications(&entry.topic) {
                for pub_value in publications {
                    self.state = (entry.handler)(&*self.state, &pub_value.value);
                    changed = true;
                }
            }
        }
        if changed {
            self.check_state_change();
        }
        changed
    }

    fn capability_descriptor(&self) -> Option<super::CapabilityDescriptor> {
        Some(self.table.capability_descriptor())
    }

    fn extension_definitions(&self) -> &[ExtensionDefinition] {
        &self.table.extension_definitions
    }

    fn evaluate_extension(
        &self,
        id: &ExtensionPointId,
        input: &super::channel::ChannelValue,
        state: &AppView<'_>,
    ) -> Vec<ExtensionOutput> {
        let mut outputs = Vec::new();
        // Check definition handlers (definer's own contribution).
        for def in &self.table.extension_definitions {
            if def.id == *id
                && let Some(handler) = &def.handler
            {
                outputs.push(ExtensionOutput {
                    plugin_id: self.id.clone(),
                    value: handler(&*self.state, input, state),
                });
            }
        }
        // Check contribution handlers (other plugins contributing).
        for contrib in &self.table.extension_contributions {
            if contrib.id == *id {
                outputs.push(ExtensionOutput {
                    plugin_id: self.id.clone(),
                    value: (contrib.handler)(&*self.state, input, state),
                });
            }
        }
        outputs
    }

    fn drain_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        std::mem::take(&mut self.pending_diagnostics)
    }
}

/// Marker trait for runtime detection of `Plugin`-backed plugins.
///
/// Enables the framework to access externalized state directly on `dyn PluginBackend`
/// objects that are backed by `PluginBridge`.
pub trait IsBridgedPlugin {
    fn plugin_state(&self) -> &dyn PluginState;
    fn plugin_state_mut(&mut self) -> &mut dyn PluginState;
}

impl IsBridgedPlugin for PluginBridge {
    fn plugin_state(&self) -> &dyn PluginState {
        &*self.state
    }
    fn plugin_state_mut(&mut self) -> &mut dyn PluginState {
        &mut *self.state
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::tests::{ColorPreviewPure, CursorLinePure, CursorLineState};
    use super::*;
    use crate::layout::Rect;
    use crate::plugin::{AnnotateContext, PluginCapabilities, PluginRuntime};
    use crate::protocol::{Color, NamedColor, WireFace};
    use crate::scroll::{ResolvedScroll, ScrollPolicyResult};
    use crate::state::AppState;

    // ---- PluginBridge tests ----

    #[test]
    fn bridge_delegates_id_and_capabilities() {
        let bridge = PluginBridge::new(CursorLinePure);
        assert_eq!(bridge.id(), PluginId("test.cursor-line-pure".into()));
        assert!(
            bridge
                .capabilities()
                .contains(PluginCapabilities::ANNOTATOR)
        );
        assert_eq!(bridge.state_hash(), 0);
    }

    #[test]
    fn bridge_auto_infers_capabilities() {
        let bridge = PluginBridge::new(CursorLinePure);
        let caps = bridge.capabilities();
        assert!(caps.contains(PluginCapabilities::ANNOTATOR));
        assert!(!caps.contains(PluginCapabilities::TRANSFORMER));
        assert!(!caps.contains(PluginCapabilities::INPUT_HANDLER));
    }

    #[test]
    fn bridge_view_deps() {
        let bridge = PluginBridge::new(CursorLinePure);
        assert_eq!(bridge.view_deps(), DirtyFlags::BUFFER);
    }

    #[test]
    fn bridge_tracks_state_changes() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let mut app = AppState::default();
        app.observed.cursor_pos.line = 5;

        assert_eq!(bridge.state_hash(), 0);

        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Same input → same state → no generation bump
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Different input → different state → generation bumps
        app.observed.cursor_pos.line = 10;
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 2);
    }

    #[test]
    fn bridge_no_change_on_irrelevant_dirty() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let app = AppState::default();

        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::STATUS);
        assert_eq!(bridge.state_hash(), 0);
    }

    #[test]
    fn bridge_annotates_cursor_line() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let mut app = AppState::default();
        app.observed.cursor_pos.line = 3;

        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);

        let view = AppView::new(&app);
        let ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
            pane_surface_id: None,
            pane_focused: true,
        };

        assert!(bridge.annotate_line_with_ctx(3, &view, &ctx).is_some());
        assert!(bridge.annotate_line_with_ctx(0, &view, &ctx).is_none());
        assert!(bridge.annotate_line_with_ctx(5, &view, &ctx).is_none());
    }

    #[test]
    fn bridge_handles_default_scroll_and_tracks_state_changes() {
        struct ScrollPlugin;

        impl Plugin for ScrollPlugin {
            type State = CursorLineState;

            fn id(&self) -> PluginId {
                PluginId("test.scroll-pure".into())
            }

            fn register(&self, r: &mut HandlerRegistry<CursorLineState>) {
                r.on_default_scroll(|state, candidate, _app| {
                    let mut next = state.clone();
                    next.active_line = candidate.screen_line as i32;
                    Some((
                        next,
                        ScrollPolicyResult::Immediate(ResolvedScroll::new(
                            candidate.resolved.amount,
                            candidate.resolved.line,
                            candidate.resolved.column,
                        )),
                    ))
                });
            }
        }

        let mut bridge = PluginBridge::new(ScrollPlugin);
        let state = AppState::default();
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            crate::input::Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            ResolvedScroll::new(3, 10, 5),
        );

        let result = bridge.handle_default_scroll(candidate, &AppView::new(&state));

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Immediate(ResolvedScroll::new(3, 10, 5)))
        );
        assert_eq!(bridge.state_hash(), 1);
    }

    #[test]
    fn bridge_tracks_workspace_changed_state_updates() {
        struct WorkspaceObserverPlugin;

        impl Plugin for WorkspaceObserverPlugin {
            type State = u32;

            fn id(&self) -> PluginId {
                PluginId("test.workspace-observer-pure".into())
            }

            fn register(&self, r: &mut HandlerRegistry<u32>) {
                r.on_workspace_changed(|state, _query| state + 1);
            }
        }

        let mut bridge = PluginBridge::new(WorkspaceObserverPlugin);
        let workspace = crate::workspace::Workspace::default();
        let query = workspace.query(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });

        bridge.on_workspace_changed(&query);

        assert_eq!(bridge.state_hash(), 1);
    }

    // ---- Registry integration tests ----

    #[test]
    fn register_integrates_with_registry() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn registry_init_and_state_change() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.observed.cursor_pos.line = 2;
        app.observed.lines = (vec![vec![], vec![], vec![], vec![], vec![]]).into();
        app.runtime.cols = 80;
        app.runtime.rows = 24;

        let _batch = registry.init_all_batch(&AppView::new(&app));

        // Notify plugins of state change
        let batch = registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);
        assert!(batch.effects.commands.is_empty());

        // Prepare cache — should detect state change
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.any_plugin_state_changed());

        // Second prepare with no further changes
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_plugin_state_changed());
    }

    #[test]
    fn register_collect_annotations() {
        let mut runtime = PluginRuntime::new();
        runtime.register(CursorLinePure);

        let mut app = AppState::default();
        app.observed.cursor_pos.line = 1;
        app.observed.lines = (vec![vec![], vec![], vec![]]).into();
        app.runtime.cols = 80;
        app.runtime.rows = 24;

        let _ = runtime.init_all_batch(&AppView::new(&app));
        runtime.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);

        let ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
            pane_surface_id: None,
            pane_focused: true,
        };
        let result = runtime
            .view()
            .collect_annotations(&AppView::new(&app), &ctx);
        assert!(result.line_backgrounds.is_some());
        let bgs = result.line_backgrounds.unwrap();
        assert!(bgs[0].is_none());
        assert!(bgs[1].is_some());
        assert!(bgs[2].is_none());
    }

    // ---- Complex state tests ----

    #[test]
    fn complex_state_tracks_changes() {
        let mut bridge = PluginBridge::new(ColorPreviewPure);
        let mut app = AppState::default();
        app.observed.cursor_pos.line = 0;

        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Same cursor → state still changes (generation increments)
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 2);
    }

    // ---- View deps / needs_recollect tests ----

    #[test]
    fn needs_recollect_false_when_dirty_disjoint_from_view_deps() {
        struct BufferOnlyPlugin;
        impl Plugin for BufferOnlyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.buffer-only".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.declare_interests(DirtyFlags::BUFFER);
            }
        }

        let mut registry = PluginRuntime::new();
        registry.register(BufferOnlyPlugin);

        // First prepare: always needs recollect (first frame)
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.any_needs_recollect());

        // After first frame, no dirty flags and no state change → skip
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_needs_recollect());

        // STATUS dirty is disjoint from BUFFER view_deps → still skip
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        assert!(!registry.any_needs_recollect());

        // BUFFER dirty intersects view_deps → needs recollect
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.any_needs_recollect());
    }

    #[test]
    fn needs_recollect_true_when_state_hash_changes() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.observed.cursor_pos.line = 5;

        // First frame
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.any_needs_recollect());

        // Mutate plugin state
        registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);

        // State hash changed → needs recollect even without matching dirty
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.any_needs_recollect());

        // No further state change, no matching dirty → skip
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_needs_recollect());
    }

    #[test]
    fn view_deps_exposed_through_plugin_view() {
        struct BufferOnlyPlugin;
        impl Plugin for BufferOnlyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.buffer-only-view".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.declare_interests(DirtyFlags::BUFFER);
            }
        }

        let mut registry = PluginRuntime::new();
        registry.register(BufferOnlyPlugin);

        // First frame
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.view().any_needs_recollect());

        // Second frame, STATUS only → skip
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        assert!(!registry.view().any_needs_recollect());

        // BUFFER dirty → recollect
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.view().any_needs_recollect());
    }

    // ---- IsBridgedPlugin ----

    #[test]
    fn is_bridged_plugin_access() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let mut app = AppState::default();
        app.observed.cursor_pos.line = 7;
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);

        let state = bridge.plugin_state();
        let typed = state.as_any().downcast_ref::<CursorLineState>().unwrap();
        assert_eq!(typed.active_line, 7);
    }

    // ---- Transform patch tests ----

    #[test]
    fn bridge_transform_patch_returns_raw_patch() {
        use crate::element::Element;
        use crate::plugin::element_patch::ElementPatch;
        use crate::plugin::{TransformContext, TransformTarget};

        struct AppendPlugin;
        impl Plugin for AppendPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.append-transform".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.on_transform(0, |_state, _target, _app, _ctx| ElementPatch::Append {
                    element: Element::plain_text("appended"),
                });
            }
        }

        let bridge = PluginBridge::new(AppendPlugin);
        let app = AppState::default();
        let ctx = TransformContext {
            is_default: true,
            chain_position: 0,
            pane_surface_id: None,
            pane_focused: true,
            target_line: None,
        };

        let patch = bridge.transform_patch(&TransformTarget::BUFFER, &AppView::new(&app), &ctx);
        assert!(patch.is_some());
        assert!(matches!(patch.unwrap(), ElementPatch::Append { .. }));
    }

    #[test]
    fn bridge_transform_patch_none_without_handler() {
        use crate::plugin::{TransformContext, TransformTarget};

        let bridge = PluginBridge::new(CursorLinePure); // no transform handler
        let app = AppState::default();
        let ctx = TransformContext {
            is_default: true,
            chain_position: 0,
            pane_surface_id: None,
            pane_focused: true,
            target_line: None,
        };

        assert!(
            bridge
                .transform_patch(&TransformTarget::BUFFER, &AppView::new(&app), &ctx)
                .is_none()
        );
    }

    #[test]
    fn transform_chain_algebraic_composition() {
        use crate::element::Element;
        use crate::plugin::element_patch::ElementPatch;
        use crate::plugin::{TransformSubject, TransformTarget};

        struct PrependPlugin;
        impl Plugin for PrependPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.prepend".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Prepend {
                    element: Element::plain_text("before"),
                });
            }
        }

        struct AppendPlugin;
        impl Plugin for AppendPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.append".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.on_transform(0, |_state, _target, _app, _ctx| ElementPatch::Append {
                    element: Element::plain_text("after"),
                });
            }
        }

        let mut runtime = PluginRuntime::new();
        runtime.register(PrependPlugin);
        runtime.register(AppendPlugin);

        let app = AppState::default();
        let subject = TransformSubject::Element(Element::plain_text("base"));
        let result = runtime.view().apply_transform_chain(
            TransformTarget::BUFFER,
            subject,
            &AppView::new(&app),
        );

        // Prepend (prio 10, applied first) + Append (prio 0, applied second)
        // Result should be a Flex with [before, Flex[base, after]]
        // due to sequential patch application
        match result.into_element() {
            Element::Flex { children, .. } => {
                assert_eq!(children.len(), 2, "Expected 2 children from Prepend patch");
            }
            other => panic!("expected Flex, got {other:?}"),
        }
    }

    #[test]
    fn transform_chain_replace_absorbs_prior_patches() {
        use crate::element::Element;
        use crate::plugin::element_patch::ElementPatch;
        use crate::plugin::{TransformSubject, TransformTarget};

        struct ModifyPlugin;
        impl Plugin for ModifyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.modify".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.on_transform(10, |_state, _target, _app, _ctx| {
                    ElementPatch::ModifyStyle {
                        overlay: std::sync::Arc::new(crate::protocol::UnresolvedStyle::from_face(
                            &WireFace {
                                fg: Color::Named(NamedColor::Red),
                                ..WireFace::default()
                            },
                        )),
                    }
                });
            }
        }

        struct ReplacePlugin;
        impl Plugin for ReplacePlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.replace".into())
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.on_transform(0, |_state, _target, _app, _ctx| ElementPatch::Replace {
                    element: Element::plain_text("replaced"),
                });
            }
        }

        let mut runtime = PluginRuntime::new();
        runtime.register(ModifyPlugin);
        runtime.register(ReplacePlugin);

        let app = AppState::default();
        let subject = TransformSubject::Element(Element::plain_text("original"));
        let result = runtime.view().apply_transform_chain(
            TransformTarget::BUFFER,
            subject,
            &AppView::new(&app),
        );

        // Replace (prio 0) absorbs ModifyFace (prio 10) during normalization
        assert_eq!(result.into_element(), Element::plain_text("replaced"));
    }

    // ---- Exhaustive handler dispatch coverage ----

    /// Verifies that every handler field in `HandlerTable` is dispatched through
    /// `PluginBridge`. If a new handler is added to `HandlerTable` but not wired
    /// in `PluginBridge`, this test will fail with a descriptive message.
    #[test]
    fn exhaustive_handler_dispatch_coverage() {
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};

        use crate::element::InteractiveId;
        use crate::input::{Key, Modifiers, MouseButton, MouseEvent, MouseEventKind};
        use crate::plugin::element_patch::ElementPatch;
        use crate::plugin::handler_table::GutterSide;
        use crate::plugin::pubsub::TopicId;
        use crate::plugin::{
            AnnotateContext, ContributeContext, OverlayContext, TransformContext, TransformTarget,
        };

        use crate::scroll::{ResolvedScroll, ScrollGranularity};

        const EXPECTED_HANDLER_NAMES: &[&str] = &[
            "init",
            "session_ready",
            "state_changed",
            "io_event",
            "workspace_changed",
            "shutdown",
            "update",
            "key",
            "key_middleware",
            "observe_key",
            "text_input",
            "observe_text_input",
            "observe_mouse",
            "handle_mouse",
            "default_scroll",
            "contribute",
            "transform",
            "gutter",
            "background",
            "inline",
            "virtual_text",
            "overlay",
            "display",
            "menu_transform",
            "publish",
            "subscribe",
            "paint_inline_box",
            "buffer_edit_intercept",
        ];

        let invoked: Arc<Mutex<HashSet<&'static str>>> = Arc::new(Mutex::new(HashSet::new()));

        // Build a plugin that registers every handler type.
        struct AllHandlersPlugin {
            invoked: Arc<Mutex<HashSet<&'static str>>>,
        }

        impl Plugin for AllHandlersPlugin {
            type State = u32;

            fn id(&self) -> PluginId {
                PluginId("test.all-handlers".into())
            }

            fn register(&self, r: &mut HandlerRegistry<u32>) {
                let inv = self.invoked.clone();
                r.on_init(move |s, _app| {
                    inv.lock().unwrap().insert("init");
                    (*s, Effects::default())
                });

                let inv = self.invoked.clone();
                r.on_session_ready(move |s, _app| {
                    inv.lock().unwrap().insert("session_ready");
                    (*s, Effects::default())
                });

                let inv = self.invoked.clone();
                r.on_state_changed(move |s, _app, _dirty| {
                    inv.lock().unwrap().insert("state_changed");
                    (*s, Effects::default())
                });

                let inv = self.invoked.clone();
                r.on_io_event(move |s, _event, _app| {
                    inv.lock().unwrap().insert("io_event");
                    (*s, Effects::default())
                });

                let inv = self.invoked.clone();
                r.on_workspace_changed(move |s, _query| {
                    inv.lock().unwrap().insert("workspace_changed");
                    *s
                });

                let inv = self.invoked.clone();
                r.on_shutdown(move |_s| {
                    inv.lock().unwrap().insert("shutdown");
                });

                let inv = self.invoked.clone();
                r.on_update(move |s, _msg, _app| {
                    inv.lock().unwrap().insert("update");
                    (*s, Effects::default())
                });

                let inv = self.invoked.clone();
                r.on_key(move |s, _key, _app| {
                    inv.lock().unwrap().insert("key");
                    Some((*s, Vec::<Command>::new()))
                });

                let inv = self.invoked.clone();
                r.on_key_middleware(move |s, _key, _app| {
                    inv.lock().unwrap().insert("key_middleware");
                    (*s, KeyHandleResult::Passthrough)
                });

                let inv = self.invoked.clone();
                r.on_observe_key(move |s, _key, _app| {
                    inv.lock().unwrap().insert("observe_key");
                    *s
                });

                let inv = self.invoked.clone();
                r.on_text_input(move |s, _text, _app| {
                    inv.lock().unwrap().insert("text_input");
                    Some((*s, Vec::<Command>::new()))
                });

                let inv = self.invoked.clone();
                r.on_observe_text_input(move |s, _text, _app| {
                    inv.lock().unwrap().insert("observe_text_input");
                    *s
                });

                let inv = self.invoked.clone();
                r.on_observe_mouse(move |s, _event, _app| {
                    inv.lock().unwrap().insert("observe_mouse");
                    *s
                });

                let inv = self.invoked.clone();
                r.on_handle_mouse(move |s, _event, _id, _app| {
                    inv.lock().unwrap().insert("handle_mouse");
                    Some((*s, Vec::<Command>::new()))
                });

                let inv = self.invoked.clone();
                r.on_default_scroll(move |s, _candidate, _app| {
                    inv.lock().unwrap().insert("default_scroll");
                    Some((
                        *s,
                        ScrollPolicyResult::Immediate(ResolvedScroll::new(1, 0, 0)),
                    ))
                });

                let inv = self.invoked.clone();
                r.on_contribute(SlotId::STATUS_LEFT, move |_s, _app, _ctx| {
                    inv.lock().unwrap().insert("contribute");
                    None
                });

                let inv = self.invoked.clone();
                r.on_transform(0, move |_s, _target, _app, _ctx| {
                    inv.lock().unwrap().insert("transform");
                    ElementPatch::Identity
                });

                let inv = self.invoked.clone();
                r.on_decorate_gutter(GutterSide::Left, 0, move |_s, _line, _app, _ctx| {
                    inv.lock().unwrap().insert("gutter");
                    None
                });

                let inv = self.invoked.clone();
                r.on_decorate_background(move |_s, _line, _app, _ctx| {
                    inv.lock().unwrap().insert("background");
                    None
                });

                let inv = self.invoked.clone();
                r.on_decorate_inline(move |_s, _line, _app, _ctx| {
                    inv.lock().unwrap().insert("inline");
                    None
                });

                let inv = self.invoked.clone();
                r.on_virtual_text(move |_s, _line, _app, _ctx| {
                    inv.lock().unwrap().insert("virtual_text");
                    vec![]
                });

                let inv = self.invoked.clone();
                r.on_overlay(move |_s, _app, _ctx| {
                    inv.lock().unwrap().insert("overlay");
                    None
                });

                let inv = self.invoked.clone();
                r.on_display(move |_s, _app| {
                    inv.lock().unwrap().insert("display");
                    vec![]
                });

                let inv = self.invoked.clone();
                r.on_menu_transform(move |_s, _item, _index, _selected, _app| {
                    inv.lock().unwrap().insert("menu_transform");
                    None
                });

                let inv = self.invoked.clone();
                r.on_paint_inline_box(move |_s, _box_id, _app| {
                    inv.lock().unwrap().insert("paint_inline_box");
                    None
                });

                let inv = self.invoked.clone();
                r.publish::<u32>(TopicId::new("test.topic"), move |_s, _app| {
                    inv.lock().unwrap().insert("publish");
                    42u32
                });

                let inv = self.invoked.clone();
                r.subscribe::<u32>(TopicId::new("test.topic"), move |s, _value| {
                    inv.lock().unwrap().insert("subscribe");
                    *s
                });

                let inv = self.invoked.clone();
                r.on_buffer_edit_intercept(move |s, _edit, _app| {
                    inv.lock().unwrap().insert("buffer_edit_intercept");
                    (
                        *s,
                        crate::state::shadow_cursor::BufferEditVerdict::PassThrough,
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(AllHandlersPlugin {
            invoked: invoked.clone(),
        });

        let app_state = AppState::default();
        let app = AppView::new(&app_state);
        let key = KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        };
        let mouse = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 0,
            column: 0,
            modifiers: Modifiers::empty(),
        };
        let annotate_ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
            pane_surface_id: None,
            pane_focused: true,
        };
        let contribute_ctx = ContributeContext {
            min_width: 0,
            max_width: None,
            min_height: 0,
            max_height: None,
            visible_lines: 0..24,
            screen_cols: 80,
            screen_rows: 24,
            pane_surface_id: None,
            pane_focused: true,
        };
        let transform_ctx = TransformContext {
            is_default: true,
            chain_position: 0,
            pane_surface_id: None,
            pane_focused: true,
            target_line: None,
        };
        let overlay_ctx = OverlayContext {
            screen_cols: 80,
            screen_rows: 24,
            menu_rect: None,
            existing_overlays: vec![],
            focused_surface_id: None,
        };

        // Lifecycle
        bridge.on_init_effects(&app);
        bridge.on_active_session_ready_effects(&app);
        bridge.on_state_changed_effects(&app, DirtyFlags::ALL);
        bridge.on_io_event_effects(
            &IoEvent::Process(crate::plugin::ProcessEvent::Stdout {
                job_id: 0,
                data: vec![],
            }),
            &app,
        );
        let workspace = crate::workspace::Workspace::default();
        let query = workspace.query(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        bridge.on_workspace_changed(&query);
        bridge.on_shutdown();
        let mut msg: Box<dyn Any> = Box::new(());
        bridge.update_effects(&mut *msg, &app);

        // Input
        bridge.handle_key(&key, &app);
        bridge.handle_key_middleware(&key, &app);
        bridge.observe_key(&key, &app);
        bridge.handle_text_input("text", &app);
        bridge.observe_text_input("text", &app);
        bridge.observe_mouse(&mouse, &app);
        bridge.handle_mouse(&mouse, InteractiveId::framework(0), &app);
        let candidate = DefaultScrollCandidate::new(
            0,
            0,
            Modifiers::empty(),
            ScrollGranularity::Line,
            1,
            ResolvedScroll::new(1, 0, 0),
        );
        bridge.handle_default_scroll(candidate, &app);

        // View
        bridge.contribute_to(&SlotId::STATUS_LEFT, &app, &contribute_ctx);
        bridge.transform_patch(&TransformTarget::BUFFER, &app, &transform_ctx);
        bridge.decorate_gutter(GutterSide::Left, 0, &app, &annotate_ctx);
        bridge.decorate_background(0, &app, &annotate_ctx);
        bridge.decorate_inline(0, &app, &annotate_ctx);
        bridge.annotate_virtual_text(0, &app, &annotate_ctx);
        bridge.contribute_overlay_with_ctx(&app, &overlay_ctx);
        bridge.display_directives(&app);
        bridge.transform_menu_item(&[crate::protocol::Atom::plain("item")], 0, false, &app);

        // Inline-box paint (ADR-031 Phase 10 Step 2-native)
        bridge.paint_inline_box(0, &app);

        // Buffer-edit intercept (ADR-035 ShadowCursor follow-up)
        let probe_edit = crate::state::shadow_cursor::BufferEdit {
            target: crate::state::selection::Selection::new(
                crate::state::selection::BufferPos::new(0, 0),
                crate::state::selection::BufferPos::new(0, 0),
            ),
            original: String::new(),
            replacement: String::new(),
            base_version: crate::history::VersionId::INITIAL,
        };
        bridge.intercept_buffer_edit(&probe_edit, &app);

        // Pub/Sub
        let mut bus = super::super::pubsub::TopicBus::new();
        bridge.collect_publications(&mut bus, &app);
        // Add an external publication so subscriber can receive it
        bus.publish(
            TopicId::new("test.topic"),
            PluginId("external".into()),
            super::super::channel::ChannelValue::new(&99u32).unwrap(),
        );
        bridge.deliver_subscriptions(&bus);

        // Assert
        let invoked = invoked.lock().unwrap();
        let expected: HashSet<&str> = EXPECTED_HANDLER_NAMES.iter().copied().collect();
        let missing: Vec<&&str> = expected.difference(&invoked).collect();
        let extra: Vec<&&str> = invoked.difference(&expected).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "Dispatch coverage mismatch.\n  Missing: {missing:?}\n  Extra: {extra:?}\n\
             When adding a new handler, update EXPECTED_HANDLER_NAMES and the Plugin::register() above."
        );
    }

    // --- inject_owner tests ---

    #[test]
    fn inject_owner_replaces_unassigned() {
        let tag = PluginTag(5);
        let mut el = Element::Interactive {
            child: Box::new(Element::text("test", crate::protocol::WireFace::default())),
            id: InteractiveId::unassigned(42),
        };
        inject_owner(&mut el, tag);
        match &el {
            Element::Interactive { id, .. } => {
                assert_eq!(id.owner, tag);
                assert_eq!(id.local, 42);
            }
            _ => panic!("expected Interactive"),
        }
    }

    #[test]
    fn inject_owner_preserves_existing_owner() {
        let existing_tag = PluginTag(3);
        let injection_tag = PluginTag(5);
        let mut el = Element::Interactive {
            child: Box::new(Element::text("test", crate::protocol::WireFace::default())),
            id: InteractiveId::new(42, existing_tag),
        };
        inject_owner(&mut el, injection_tag);
        match &el {
            Element::Interactive { id, .. } => {
                assert_eq!(
                    id.owner, existing_tag,
                    "should not overwrite existing owner"
                );
            }
            _ => panic!("expected Interactive"),
        }
    }

    #[test]
    fn inject_owner_walks_nested_tree() {
        use crate::element::{FlexChild, Overlay, OverlayAnchor};

        let tag = PluginTag(7);
        let inner = Element::Interactive {
            child: Box::new(Element::Empty),
            id: InteractiveId::unassigned(1),
        };
        let overlay_el = Element::Interactive {
            child: Box::new(Element::Empty),
            id: InteractiveId::unassigned(2),
        };
        let mut tree = Element::Stack {
            base: Box::new(Element::Flex {
                direction: crate::element::Direction::Row,
                children: vec![FlexChild {
                    element: inner,
                    flex: 1.0,
                    min_size: None,
                    max_size: None,
                }],
                gap: 0,
                align: crate::element::Align::Start,
                cross_align: crate::element::Align::Start,
            }),
            overlays: vec![Overlay {
                element: overlay_el,
                anchor: OverlayAnchor::Absolute {
                    x: 0,
                    y: 0,
                    w: 1,
                    h: 1,
                },
            }],
        };
        inject_owner(&mut tree, tag);

        // Check inner interactive in flex child
        if let Element::Stack { base, overlays } = &tree {
            if let Element::Flex { children, .. } = base.as_ref() {
                if let Element::Interactive { id, .. } = &children[0].element {
                    assert_eq!(id.owner, tag);
                    assert_eq!(id.local, 1);
                } else {
                    panic!("expected Interactive in flex child");
                }
            }
            // Check overlay interactive
            if let Element::Interactive { id, .. } = &overlays[0].element {
                assert_eq!(id.owner, tag);
                assert_eq!(id.local, 2);
            } else {
                panic!("expected Interactive in overlay");
            }
        }
    }
}
