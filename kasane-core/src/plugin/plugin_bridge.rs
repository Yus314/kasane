//! `PluginBridge` — the framework's loaded-plugin shape.
//!
//! Construction calls `Plugin::register()` to capture a [`HandlerTable`];
//! all dispatch methods are inherent on `PluginBridge` and route through
//! the table's erased handlers. State changes are tracked via a
//! generation counter, bumped only when the handler's returned state
//! differs from the current state under [`PluginState::dyn_eq`].

use std::any::Any;

use crate::element::{Element, InteractiveId, Overlay, PluginTag};
use crate::input::{CompiledKeyMap, DropEvent, KeyEvent, KeyResponse, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::effect::error_attribution::PluginErrorEvent;
use super::handler_registry::HandlerRegistry;
use super::handler_table::HandlerTable;
use super::io::ProcessEvent;
use super::process_task::{
    ProcessTaskFeedResult, ProcessTaskHandle, collect_fallbacks, spawn_command,
};
use super::pubsub::TopicBus;
use super::state::{Plugin, PluginState};
use super::traits::{KeyPreDispatchResult, MousePreDispatchResult, TextInputPreDispatchResult};
use super::{
    AnnotateContext, AppView, BackgroundLayer, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, ElementPatch, GutterSide, IoEvent, KeyHandleResult, OverlayContext,
    OverlayContribution, PluginAuthorities, PluginCapabilities, PluginDiagnostic, PluginId, SlotId,
    TransformContext, TransformDescriptor, TransformSubject, TransformTarget, VirtualTextItem,
};

/// The framework's loaded-plugin shape — built by [`Plugin::register`]
/// and stored unboxed inside `PluginRuntime::PluginSlot`.
///
/// Construction calls `P::register()` to capture a [`HandlerTable`];
/// every dispatch method is inherent on `PluginBridge` and routes
/// through the table's erased handlers. State changes are tracked via
/// a generation counter for L1 cache invalidation: dispatch macros
/// compare the handler's returned state against the current state with
/// [`PluginState::dyn_eq`] *before* swapping, so the counter bumps only
/// on real mutations and no per-mutation snapshot clone is required.
pub struct PluginBridge {
    id: PluginId,
    table: HandlerTable,
    state: Box<dyn PluginState>,
    generation: u64,
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
            .projection_handlers
            .iter()
            .map(|e| e.key.clone())
            .collect();
        let state: Box<dyn PluginState> = Box::new(P::State::default());
        PluginBridge {
            id,
            table,
            state,
            generation: 0,
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

    /// Swap `self.state` for `new_state`, bumping `generation` when the
    /// two differ under [`PluginState::dyn_eq`].
    ///
    /// Comparing *before* the swap means no snapshot clone is needed —
    /// the previous state is still owned by `self.state` for the
    /// duration of the call.
    fn replace_state(&mut self, new_state: Box<dyn PluginState>) {
        // Deref through the trait objects so dispatch goes via each
        // inner type's vtable. `box.dyn_eq(...)` would otherwise resolve
        // to the blanket `impl<T> PluginState for T` on `Box<dyn
        // PluginState>` itself, causing the downcast to fail.
        if !(*new_state).dyn_eq(&*self.state) {
            self.generation += 1;
        }
        self.state = new_state;
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
                    self.replace_state(new_state);
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
        Element::Flex { children, .. } => {
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

/// Dispatch a state+effects handler: call handler, swap state in-place
/// (with pre-swap dyn_eq → generation bump), return effects.
macro_rules! dispatch_state_effect {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            let (new_state, effects) = handler(&*$self.state, $($arg),*);
            $self.replace_state(new_state);
            effects
        } else {
            Effects::default()
        }
    };
}

/// Dispatch a state-only handler: call handler, swap state in-place
/// (with pre-swap dyn_eq → generation bump).
macro_rules! dispatch_state_only {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            let new_state = handler(&*$self.state, $($arg),*);
            $self.replace_state(new_state);
        }
    };
}

/// Dispatch an optional-consume handler: call handler, swap state if `Some`
/// (with pre-swap dyn_eq → generation bump), return mapped result.
macro_rules! dispatch_optional_consume {
    ($self:expr, $field:ident $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            handler(&*$self.state, $($arg),*).map(|(new_state, result)| {
                $self.replace_state(new_state);
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

/// Dispatch a state-mutating handler returning `(State, T)`; replace state,
/// run the change check, and return `T`. The default expression is used
/// when no handler is registered.
///
/// Use this for handlers whose return type isn't `Effects` and isn't
/// `Option<T>` — e.g. `KeyHandleResult`, `KeyPreDispatchResult`,
/// `BufferEditVerdict`, `KeyResponse`. For state-mutating handlers
/// returning `Effects`, prefer [`dispatch_state_effect!`]; for those
/// returning `Option<T>`, prefer [`dispatch_optional_consume!`].
macro_rules! dispatch_state_with_default {
    ($self:expr, $field:ident, $default:expr $(, $arg:expr)*) => {
        if let Some(handler) = &$self.table.$field {
            let (new_state, result) = handler(&*$self.state, $($arg),*);
            $self.replace_state(new_state);
            result
        } else {
            $default
        }
    };
}

/// Dispatch a contribute-style handler returning `Option<C>` where `C`
/// owns an `element: Element` field, then run [`inject_owner`] on the
/// element so that any unassigned `InteractiveId`s pick up the bridge's
/// `plugin_tag`.
///
/// The `$elem` token names the field on `C` that holds the element
/// (typically `element`). For inputs that already produce a bare
/// `Element`, see [`dispatch_inject_owner_element!`] instead.
macro_rules! dispatch_inject_owner_contribution {
    ($self:expr, $handler:expr, $elem:ident $(, $arg:expr)*) => {
        $handler(&*$self.state, $($arg),*).map(|mut c| {
            inject_owner(&mut c.$elem, $self.plugin_tag);
            c
        })
    };
}

impl PluginBridge {
    pub fn id(&self) -> PluginId {
        self.id.clone()
    }

    pub fn set_plugin_tag(&mut self, tag: PluginTag) {
        self.plugin_tag = tag;
    }

    pub fn capabilities(&self) -> PluginCapabilities {
        self.table
            .capabilities_override
            .unwrap_or_else(|| self.table.capabilities())
    }

    pub fn authorities(&self) -> PluginAuthorities {
        self.table.authorities
    }

    pub fn allows_process_spawn(&self) -> bool {
        self.table.allows_process_spawn
    }

    pub fn suppressed_builtins(&self) -> &std::collections::HashSet<super::BuiltinTarget> {
        &self.table.suppressed_builtins
    }

    pub fn state_hash(&self) -> u64 {
        self.table
            .state_hash
            .as_ref()
            .map_or(self.generation, |h| h())
    }

    pub fn view_deps(&self) -> DirtyFlags {
        self.table.interests()
    }

    pub fn transform_priority(&self) -> i16 {
        self.table
            .transform_handler
            .as_ref()
            .map_or(0, |e| e.priority)
    }

    pub fn transform_descriptor(&self) -> Option<TransformDescriptor> {
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

    pub fn display_directive_priority(&self) -> i16 {
        self.table.display_priority
    }

    // === Lifecycle ===

    pub fn on_init_effects(&mut self, app: &AppView<'_>) -> Effects {
        dispatch_state_effect!(self, init_handler, app)
    }

    pub fn on_active_session_ready_effects(&mut self, app: &AppView<'_>) -> Effects {
        dispatch_state_effect!(self, session_ready_handler, app)
    }

    pub fn on_shutdown(&mut self) {
        if let Some(handler) = &self.table.shutdown_handler {
            handler(&*self.state);
        }
    }

    pub fn on_state_changed_effects(&mut self, app: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        dispatch_state_effect!(self, state_changed_handler, app, dirty)
    }

    pub fn intercept_buffer_edit(
        &mut self,
        edit: &crate::state::shadow_cursor::BufferEdit,
        app: &AppView<'_>,
    ) -> crate::state::shadow_cursor::BufferEditVerdict {
        dispatch_state_with_default!(
            self,
            buffer_edit_intercept_handler,
            crate::state::shadow_cursor::BufferEditVerdict::PassThrough,
            edit,
            app
        )
    }

    pub fn on_workspace_changed(&mut self, query: &WorkspaceQuery<'_>) {
        dispatch_state_only!(self, workspace_changed_handler, query);
    }

    pub fn surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        match &self.table.surfaces_handler {
            Some(factory) => factory(&*self.state),
            None => Vec::new(),
        }
    }

    pub fn workspace_request(&self) -> Option<crate::workspace::Placement> {
        self.table.workspace_request.clone()
    }

    pub fn register_lenses(&self, registry: &mut crate::lens::LensRegistry) -> usize {
        let Some(factory) = &self.table.lenses_handler else {
            return 0;
        };
        let lenses = factory();
        let count = lenses.len();
        for lens in lenses {
            registry.register(lens);
        }
        count
    }

    pub fn workspace_save(&self) -> Option<serde_json::Value> {
        self.table
            .workspace_save_handler
            .as_ref()
            .and_then(|h| h(&*self.state))
    }

    pub fn workspace_restore(&mut self, data: &serde_json::Value) {
        dispatch_state_only!(self, workspace_restore_handler, data);
    }

    pub fn persist_state(&self) -> Option<Vec<u8>> {
        self.table
            .persist_state_handler
            .as_ref()
            .and_then(|h| h(&*self.state))
    }

    pub fn restore_state(&mut self, data: &[u8]) -> bool {
        self.table
            .restore_state_handler
            .as_ref()
            .is_some_and(|h| h(&*self.state, data))
    }

    // === Input ===

    pub fn observe_key(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_key_handler, key, app);
    }

    pub fn observe_text_input(&mut self, text: &str, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_text_input_handler, text, app);
    }

    pub fn observe_mouse(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_mouse_handler, event, app);
    }

    pub fn observe_drop(&mut self, event: &DropEvent, app: &AppView<'_>) {
        dispatch_state_only!(self, observe_drop_handler, event, app);
    }

    pub fn handle_key(&mut self, key: &KeyEvent, app: &AppView<'_>) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, key_handler, key, app)
    }

    pub fn handle_text_input(&mut self, text: &str, app: &AppView<'_>) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, text_input_handler, text, app)
    }

    pub fn handle_key_middleware(&mut self, key: &KeyEvent, app: &AppView<'_>) -> KeyHandleResult {
        if self.table.key_middleware_handler.is_some() {
            dispatch_state_with_default!(
                self,
                key_middleware_handler,
                KeyHandleResult::Passthrough,
                key,
                app
            )
        } else {
            // No middleware handler: fall back to handle_key. Cannot
            // inline into the dispatch macro because the fallback
            // dispatches through another method on `self`.
            match self.handle_key(key, app) {
                Some(commands) => KeyHandleResult::Consumed(commands),
                None => KeyHandleResult::Passthrough,
            }
        }
    }

    pub fn handle_key_pre_dispatch(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyPreDispatchResult {
        dispatch_state_with_default!(
            self,
            key_pre_dispatch_handler,
            KeyPreDispatchResult::Pass {
                commands: Vec::new(),
                state_updates: super::effect::effects::StateUpdates::default(),
            },
            key,
            app
        )
    }

    pub fn handle_mouse_pre_dispatch(
        &mut self,
        event: &MouseEvent,
        app: &AppView<'_>,
    ) -> MousePreDispatchResult {
        dispatch_state_with_default!(
            self,
            mouse_pre_dispatch_handler,
            MousePreDispatchResult::Pass {
                commands: Vec::new(),
                state_updates: super::effect::effects::StateUpdates::default(),
            },
            event,
            app
        )
    }

    pub fn handle_text_input_pre_dispatch(
        &mut self,
        text: &str,
        app: &AppView<'_>,
    ) -> TextInputPreDispatchResult {
        dispatch_state_with_default!(
            self,
            text_input_pre_dispatch_handler,
            TextInputPreDispatchResult::Pass,
            text,
            app
        )
    }

    pub fn handle_mouse_fallback(
        &mut self,
        event: &MouseEvent,
        scroll_amount: i32,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        dispatch_state_with_default!(
            self,
            mouse_fallback_handler,
            None,
            event,
            scroll_amount,
            app
        )
    }

    pub fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, handle_mouse_handler, event, id, app)
    }

    pub fn handle_drop(
        &mut self,
        event: &DropEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        dispatch_optional_consume!(self, handle_drop_handler, event, id, app)
    }

    pub fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        dispatch_optional_consume!(self, default_scroll_handler, candidate, app)
    }

    pub fn compiled_key_map(&self) -> Option<&CompiledKeyMap> {
        self.table.key_map.as_ref()
    }

    pub fn invoke_action(
        &mut self,
        action_id: &str,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyResponse {
        dispatch_state_with_default!(self, action_handler, KeyResponse::Pass, action_id, key, app)
    }

    pub fn refresh_key_groups(&mut self, app: &AppView<'_>) {
        // γ-3.3c-4c: explicit `&mut self.table.generated` binding so the
        // simultaneous immutable + mutable borrows of distinct fields
        // resolve via split borrow on the underlying generated table —
        // Deref's `deref` / `deref_mut` method calls cannot be borrow-
        // split because each borrows the entire deref target.
        let table = &mut self.table.generated;
        if let Some(handler) = &table.group_refresh_handler
            && let Some(map) = &mut table.key_map
        {
            handler(&*self.state, app, map);
        }
    }

    pub fn update_effects(&mut self, msg: &mut dyn Any, app: &AppView<'_>) -> Effects {
        dispatch_state_effect!(self, update_handler, msg, app)
    }

    // === View contributions ===

    pub fn contribute_to(
        &self,
        region: &SlotId,
        app: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        for entry in &self.table.contribute_handlers {
            if entry.key == *region {
                return dispatch_inject_owner_contribution!(
                    self,
                    &entry.handler,
                    element,
                    app,
                    ctx
                );
            }
        }
        if let Some(handler) = &self.table.contribute_any_handler {
            return dispatch_inject_owner_contribution!(self, handler, element, region, app, ctx);
        }
        None
    }

    pub fn transform_patch(
        &self,
        target: &TransformTarget,
        app: &AppView<'_>,
        ctx: &TransformContext,
    ) -> Option<ElementPatch> {
        self.table.transform_handler.as_ref().and_then(|entry| {
            let mut patch = (entry.handler)(&*self.state, target, app, ctx);
            // An Identity patch is treated as "no opinion" so collection
            // can flush accumulated patches and fall through to the
            // full-rewrite path. This lets adapters (notably WASM
            // plugins) register both `on_transform` and
            // `on_transform_full` and let the bridge dispatch to the
            // imperative WIT export when the declarative one returned
            // nothing.
            if matches!(patch, ElementPatch::Identity) {
                return None;
            }
            inject_owner_in_patch(&mut patch, self.plugin_tag);
            Some(patch)
        })
    }

    pub fn transform(
        &self,
        target: &TransformTarget,
        subject: TransformSubject,
        app: &AppView<'_>,
        ctx: &TransformContext,
    ) -> TransformSubject {
        if let Some(patch) = self.transform_patch(target, app, ctx) {
            return patch.apply(subject);
        }
        if let Some(full) = &self.table.transform_full_handler {
            return full(&*self.state, target, subject, app, ctx);
        }
        subject
    }

    // `annotate_line_with_ctx` is overridden only when the registry has a
    // monolithic [`HandlerRegistry::on_annotate_line`] entry; otherwise
    // `has_decomposed_annotations` returns `true` and `collection.rs`
    // dispatches through `decorate_gutter` / `decorate_background` /
    // `decorate_inline` / `annotate_virtual_text` directly. The
    // single-call path is reserved for adapters whose underlying
    // contract surfaces all annotation parts together (`WasmPlugin`
    // via the `annotate-line` WIT export).

    pub fn has_decomposed_annotations(&self) -> bool {
        self.table.annotate_line_handler.is_none()
    }

    pub fn annotate_line_with_ctx(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<crate::plugin::LineAnnotation> {
        let handler = self.table.annotate_line_handler.as_ref()?;
        let mut ann = handler(&*self.state, line, app, ctx)?;
        if let Some(ref mut el) = ann.left_gutter {
            inject_owner(el, self.plugin_tag);
        }
        if let Some(ref mut el) = ann.right_gutter {
            inject_owner(el, self.plugin_tag);
        }
        Some(ann)
    }

    pub fn decorate_gutter(
        &self,
        side: GutterSide,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<(i16, crate::element::Element)> {
        for entry in &self.table.gutter_handlers {
            if entry.key == side
                && let Some(mut el) = (entry.handler)(&*self.state, line, app, ctx)
            {
                inject_owner(&mut el, self.plugin_tag);
                return Some((entry.priority, el));
            }
        }
        None
    }
    // (decorate_gutter retains the explicit form: it pairs `el` with
    // `entry.priority` from the iterated handler entry — neither
    // `dispatch_inject_owner_contribution!` nor a simpler macro
    // fits the tuple shape without obscuring the priority lookup.)

    pub fn decorate_background(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<BackgroundLayer> {
        dispatch_view_option!(self, background_handler, line, app, ctx)
    }

    pub fn decorate_inline(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<crate::render::InlineDecoration> {
        dispatch_view_option!(self, inline_handler, line, app, ctx)
    }

    pub fn annotate_virtual_text(
        &self,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Vec<VirtualTextItem> {
        dispatch_view_or!(self, virtual_text_handler, vec![], line, app, ctx)
    }

    pub fn compute_display_scroll_offset(
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

    pub fn render_menu_overlay(
        &self,
        state: &AppView<'_>,
        view: &super::PluginView<'_>,
    ) -> Option<Overlay> {
        let handler = self.table.menu_renderer_handler.as_ref()?;
        handler(&*self.state, state, view)
    }

    pub fn render_info_overlays(
        &self,
        state: &AppView<'_>,
        avoid: &[crate::layout::Rect],
        view: &super::PluginView<'_>,
    ) -> Option<Vec<Overlay>> {
        let handler = self.table.info_renderer_handler.as_ref()?;
        handler(&*self.state, state, avoid, view)
    }

    pub fn contribute_overlay_with_ctx(
        &self,
        app: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        let handler = self.table.overlay_handler.as_ref()?;
        dispatch_inject_owner_contribution!(self, handler, element, app, ctx)
    }

    pub fn render_ornaments(
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

    pub fn paint_inline_box(&self, box_id: u64, app: &AppView<'_>) -> Option<Element> {
        dispatch_view_or!(self, paint_inline_box_handler, None, box_id, app)
    }

    pub fn transform_menu_item(
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

    pub fn content_annotations(
        &self,
        state: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Vec<crate::display::ContentAnnotation> {
        dispatch_view_or!(self, content_annotation_handler, vec![], state, ctx)
    }

    pub fn display_directives(&self, app: &AppView<'_>) -> Vec<DisplayDirective> {
        dispatch_view_or!(self, display_handler, vec![], app)
    }

    pub fn has_unified_display(&self) -> bool {
        self.table.unified_display_handler.is_some()
    }

    pub fn unified_display(&self, app: &AppView<'_>) -> Vec<DisplayDirective> {
        dispatch_view_or!(self, unified_display_handler, vec![], app)
    }

    pub fn projection_descriptors(&self) -> &[crate::display::ProjectionDescriptor] {
        &self.cached_projection_descriptors
    }

    pub fn projection_directives(
        &self,
        id: &crate::display::ProjectionId,
        state: &AppView<'_>,
    ) -> Vec<DisplayDirective> {
        for entry in &self.table.projection_handlers {
            if &entry.key.id == id {
                return (entry.handler)(&*self.state, state);
            }
        }
        vec![]
    }

    pub fn navigation_policy(
        &self,
        unit: &crate::display::unit::DisplayUnit,
    ) -> Option<crate::display::navigation::NavigationPolicy> {
        self.table
            .navigation_policy_handler
            .as_ref()
            .map(|h| h(&*self.state, unit))
    }

    pub fn navigation_action(
        &mut self,
        unit: &crate::display::unit::DisplayUnit,
        action: crate::display::navigation::NavigationAction,
    ) -> Option<crate::display::navigation::ActionResult> {
        // `Pass` is the inert default; surface only `Some(other)`.
        match dispatch_state_with_default!(
            self,
            navigation_action_handler,
            crate::display::navigation::ActionResult::Pass,
            unit,
            action
        ) {
            crate::display::navigation::ActionResult::Pass => None,
            other => Some(other),
        }
    }

    pub fn capability_descriptor(&self) -> Option<super::CapabilityDescriptor> {
        Some(
            self.table
                .capability_descriptor_override
                .clone()
                .unwrap_or_else(|| self.table.capability_descriptor()),
        )
    }

    pub fn drain_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        std::mem::take(&mut self.pending_diagnostics)
    }

    // --- Pub/Sub ---

    pub fn collect_publications(&self, bus: &mut TopicBus, state: &AppView<'_>) {
        let plugin_id = self.id.clone();
        for entry in &self.table.publish_handlers {
            if let Some(value) = (entry.handler)(&*self.state, state) {
                bus.publish(entry.key.clone(), plugin_id.clone(), value);
            }
        }
    }

    pub fn deliver_subscriptions(&mut self, bus: &TopicBus, app: &AppView<'_>) -> Effects {
        let mut merged = Effects::default();
        // Per-value subscribers (state mutation only).
        //
        // The dyn_eq compare + generation bump are inlined (rather than
        // routed through `replace_state`) because the surrounding loop
        // holds `&self.table.subscribe_handlers`; `&mut self` would
        // conflict, while per-field borrows on `self.state` /
        // `self.generation` are disjoint from the table borrow.
        for entry in &self.table.subscribe_handlers {
            if let Some(publications) = bus.get_publications(&entry.key) {
                for pub_value in publications {
                    let new_state = (entry.handler)(&*self.state, &pub_value.value);
                    if !(*new_state).dyn_eq(&*self.state) {
                        self.generation += 1;
                    }
                    self.state = new_state;
                }
            }
        }
        // Per-topic batch handler registered through
        // `HandlerRegistry::on_subscription`. Mirrors the WIT
        // `on-subscription(topic, values) -> runtime-effects` shape and
        // forwards the handler's effects up so the dispatcher can route
        // them through the same pipeline as `notify_state_changed`.
        if let Some(handler) = &self.table.subscription_handler {
            for entry in &self.table.subscribe_handlers {
                if let Some(publications) = bus.get_publications(&entry.key) {
                    let values: Vec<super::ChannelValue> =
                        publications.iter().map(|p| p.value.clone()).collect();
                    if values.is_empty() {
                        continue;
                    }
                    let (new_state, effects) =
                        handler(&*self.state, entry.key.as_str(), &values, app);
                    if !(*new_state).dyn_eq(&*self.state) {
                        self.generation += 1;
                    }
                    self.state = new_state;
                    merged.merge(effects);
                }
            }
        }
        merged
    }

    // --- I/O ---

    pub fn on_io_event_effects(&mut self, event: &IoEvent, app: &AppView<'_>) -> Effects {
        // Route process events through active task handles first.
        if let IoEvent::Process(proc_event) = event
            && let Some(effects) = self.try_process_task_event(proc_event, app)
        {
            return effects;
        }

        // Fall through to the manual io_event handler.
        dispatch_state_effect!(self, io_event_handler, event, app)
    }

    /// Dispatch through the HandlerRegistry-driven `on_command_error`
    /// handler when registered. Plugins that did not register one inherit
    /// the trait's empty-Effects default.
    pub fn on_command_error_effects(
        &mut self,
        error: &PluginErrorEvent,
        app: &AppView<'_>,
    ) -> Effects {
        dispatch_state_effect!(self, command_error_handler, error, app)
    }

    pub fn start_process_task(&mut self, name: &str) -> Vec<Command> {
        let Some(entry) = self.table.process_tasks.iter().find(|e| e.name == name) else {
            tracing::warn!(plugin = self.id.as_str(), name, "unknown process task");
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
}

impl PluginBridge {
    /// Borrow the framework-managed [`PluginState`] for this bridge.
    pub fn plugin_state(&self) -> &dyn PluginState {
        &*self.state
    }

    /// Mutably borrow the framework-managed [`PluginState`].
    pub fn plugin_state_mut(&mut self) -> &mut dyn PluginState {
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
        assert_eq!(bridge.id(), PluginId::from("test.cursor-line-pure"));
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

        // Bridges expose decomposed annotations directly. The unified
        // `annotate_line_with_ctx` path is reserved for adapters
        // (WasmPlugin, `#[kasane_plugin]` macro) — see the comment in
        // the impl block.
        assert!(bridge.decorate_background(3, &view, &ctx).is_some());
        assert!(bridge.decorate_background(0, &view, &ctx).is_none());
        assert!(bridge.decorate_background(5, &view, &ctx).is_none());
    }

    #[test]
    fn bridge_handles_default_scroll_and_tracks_state_changes() {
        struct ScrollPlugin;

        impl Plugin for ScrollPlugin {
            type State = CursorLineState;

            fn id(&self) -> PluginId {
                PluginId::from("test.scroll-pure")
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
                PluginId::from("test.workspace-observer-pure")
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

        let mut db = crate::salsa_db::KasaneDatabase::default();
        let mut app = AppState::default();
        app.observed.cursor_pos.line = 2;
        app.observed.lines = (vec![vec![], vec![], vec![], vec![], vec![]]).into();
        app.runtime.cols = 80;
        app.runtime.rows = 24;

        let _batch = registry.init_all_batch(&AppView::new(&app));

        // Notify plugins of state change
        let batch = registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);
        assert!(batch.per_plugin_commands.is_empty());

        // Prepare cache — should detect state change
        registry.prepare_plugin_cache(DirtyFlags::BUFFER, &mut db);
        assert!(registry.any_plugin_state_changed());

        // Second prepare with no further changes
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
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
                PluginId::from("test.buffer-only")
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.declare_interests(DirtyFlags::BUFFER);
            }
        }

        let mut registry = PluginRuntime::new();
        registry.register(BufferOnlyPlugin);
        let mut db = crate::salsa_db::KasaneDatabase::default();

        // First prepare: always needs recollect (first frame)
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert!(registry.any_needs_recollect());

        // After first frame, no dirty flags and no state change → skip
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert!(!registry.any_needs_recollect());

        // STATUS dirty is disjoint from BUFFER view_deps → still skip
        registry.prepare_plugin_cache(DirtyFlags::STATUS, &mut db);
        assert!(!registry.any_needs_recollect());

        // BUFFER dirty intersects view_deps → needs recollect
        registry.prepare_plugin_cache(DirtyFlags::BUFFER, &mut db);
        assert!(registry.any_needs_recollect());
    }

    #[test]
    fn needs_recollect_true_when_state_hash_changes() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);
        let mut db = crate::salsa_db::KasaneDatabase::default();

        let mut app = AppState::default();
        app.observed.cursor_pos.line = 5;

        // First frame
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert!(registry.any_needs_recollect());

        // Mutate plugin state
        registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);

        // State hash changed → needs recollect even without matching dirty
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert!(registry.any_needs_recollect());

        // No further state change, no matching dirty → skip
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert!(!registry.any_needs_recollect());
    }

    /// `prepare_plugin_cache` lazily creates each slot's
    /// `PluginStateRevisionInput` and keeps it in lock-step with the
    /// bridge's `state_hash()` across subsequent calls.
    #[test]
    fn prepare_plugin_cache_mirrors_bridge_state_hash_onto_salsa_input() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);

        let mut db = crate::salsa_db::KasaneDatabase::default();
        let mut app = AppState::default();
        app.observed.cursor_pos.line = 3;

        // First prepare — input is lazily created with the bridge's
        // current state_hash (0, since no mutation has occurred).
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert_eq!(registry.state_revision_at(0, &db), Some(0));

        // Mutate state via a plugin lifecycle hook; bridge.state_hash
        // bumps to 1; the next prepare call mirrors that onto the input.
        registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert_eq!(registry.state_revision_at(0, &db), Some(1));

        // Idle prepare (no mutation) — input stays at 1; Salsa's
        // `set_revision().to()` short-circuits via PartialEq.
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert_eq!(registry.state_revision_at(0, &db), Some(1));
    }

    #[test]
    fn view_deps_exposed_through_plugin_view() {
        struct BufferOnlyPlugin;
        impl Plugin for BufferOnlyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId::from("test.buffer-only-view")
            }
            fn register(&self, r: &mut HandlerRegistry<()>) {
                r.declare_interests(DirtyFlags::BUFFER);
            }
        }

        let mut registry = PluginRuntime::new();
        registry.register(BufferOnlyPlugin);
        let mut db = crate::salsa_db::KasaneDatabase::default();

        // First frame
        registry.prepare_plugin_cache(DirtyFlags::empty(), &mut db);
        assert!(registry.view().any_needs_recollect());

        // Second frame, STATUS only → skip
        registry.prepare_plugin_cache(DirtyFlags::STATUS, &mut db);
        assert!(!registry.view().any_needs_recollect());

        // BUFFER dirty → recollect
        registry.prepare_plugin_cache(DirtyFlags::BUFFER, &mut db);
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
        use crate::plugin::algebra::element_patch::ElementPatch;
        use crate::plugin::{TransformContext, TransformTarget};

        struct AppendPlugin;
        impl Plugin for AppendPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId::from("test.append-transform")
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
        use crate::plugin::algebra::element_patch::ElementPatch;
        use crate::plugin::{TransformSubject, TransformTarget};

        struct PrependPlugin;
        impl Plugin for PrependPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId::from("test.prepend")
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
                PluginId::from("test.append")
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
        use crate::plugin::algebra::element_patch::ElementPatch;
        use crate::plugin::{TransformSubject, TransformTarget};

        struct ModifyPlugin;
        impl Plugin for ModifyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId::from("test.modify")
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
                PluginId::from("test.replace")
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

    // --- inject_owner tests ---

    #[test]
    fn inject_owner_replaces_unassigned() {
        let tag = PluginTag(5);
        let mut el = Element::Interactive {
            child: Box::new(Element::text("test", crate::protocol::Style::default())),
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
            child: Box::new(Element::text("test", crate::protocol::Style::default())),
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
                slot: None,
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
