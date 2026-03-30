//! Session lifecycle management.
//!
//! Session spawn, close, switch, pane death, ready gate, and ready batch handling.

use crate::layout::Rect;
use crate::plugin::{AppView, Command, CommandResult, EffectsBatch, execute_commands};
use crate::session::{SessionId, SessionManager, SessionSpec, SessionStateStore};
use crate::state::{AppState, DirtyFlags};
use crate::surface::{SurfaceId, SurfaceRegistry};

use super::SessionHost;
use super::context::{DeferredContext, focused_writer};
use super::dispatch::{apply_runtime_batch, dispatch_input_event};

/// Send resize commands to all pane clients so each knows its allocated area.
///
/// Should be called after terminal resize, split creation/deletion, or divider drag.
pub fn send_pane_resizes(
    surface_registry: &mut SurfaceRegistry,
    session_host: &mut dyn SessionHost,
    total: Rect,
) {
    let rects = surface_registry.workspace().compute_rects(total);
    for (surface_id, rect) in &rects {
        // Per-pane status bar occupies 1 row from each pane.
        let rows = rect.h.saturating_sub(1);
        if let Some(session_id) = surface_registry.session_for_surface(*surface_id)
            // Skip sessions whose kak process hasn't finished initializing.
            // The initial Resize is sent when their first Kakoune event arrives.
            && !surface_registry.has_pending_resize(session_id)
            // Only send when dimensions actually changed to avoid an infinite
            // Resize → Draw → dirty → Resize feedback loop.
            && surface_registry.needs_resize(session_id, rows, rect.w)
            && let Some(writer) = session_host.writer_for_session(session_id)
        {
            crate::io::send_request(
                writer,
                &crate::protocol::KasaneRequest::Resize { rows, cols: rect.w },
            );
        }
    }
}

/// Synchronize session metadata from SessionManager into AppState.
pub fn sync_session_metadata<R, W, C>(
    session_manager: &SessionManager<R, W, C>,
    session_states: &SessionStateStore,
    state: &mut AppState,
) {
    state.session_descriptors = session_manager.enriched_session_descriptors(session_states, state);
    state.active_session_key = session_manager.active_session_key().map(str::to_owned);
}

/// Groups the five `&mut` parameters shared by session lifecycle functions.
pub struct SessionMutContext<'a, R, W, C> {
    pub session_manager: &'a mut SessionManager<R, W, C>,
    pub session_states: &'a mut SessionStateStore,
    pub state: &'a mut AppState,
    pub dirty: &'a mut DirtyFlags,
    pub initial_resize_sent: &'a mut bool,
}

/// Handle the death of any Kakoune session (pane or primary).
///
/// Unbinds and cleans up the surface, closes the session, and restores the
/// next active session if needed. Non-BUFFER surfaces are removed from the
/// registry; BUFFER is only removed from the workspace tree (the registry
/// entry must survive for `register_builtin_surfaces` invariants).
///
/// Returns `true` if the application should quit (no sessions remain).
pub fn handle_pane_death<R, W, C>(
    session_id: SessionId,
    surface_registry: &mut SurfaceRegistry,
    ctx: &mut SessionMutContext<'_, R, W, C>,
) -> bool {
    // 1. Surface unbind
    let surface_id = surface_registry.unbind_session_by_session(session_id);

    if let Some(surface_id) = surface_id {
        // 2. Non-BUFFER surfaces are removed from the registry entirely
        if surface_id != SurfaceId::BUFFER {
            surface_registry.remove(surface_id);
        }
        // 3. Remove from workspace tree (last-leaf close returns false — safe)
        let _ = surface_registry.workspace_mut().close(surface_id);
    }

    // 4. Session lifecycle cleanup (mirrors handle_session_death)
    let was_active = ctx.session_manager.active_session_id() == Some(session_id);
    let _ = ctx.session_manager.close(session_id);
    ctx.session_states.remove(session_id);
    *ctx.dirty |= DirtyFlags::ALL;

    // 5. All sessions gone → quit
    if ctx.session_manager.is_empty() {
        return true;
    }

    // 6. Active session switched — restore state
    if was_active {
        let restored = ctx
            .session_manager
            .active_session_id()
            .is_some_and(|active| ctx.session_states.restore_into(active, ctx.state));
        if !restored {
            ctx.state.reset_for_session_switch();
        }
        *ctx.initial_resize_sent = false;
    }

    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    false
}

/// Spawn a new managed session, returning the session ID and reader on success.
///
/// The reader is returned so the backend can wire it up to its specific event
/// channel. The activation logic (state restore, dirty flags) is handled here.
pub fn spawn_session_core<R, W, C>(
    spec: &SessionSpec,
    activate: bool,
    ctx: &mut SessionMutContext<'_, R, W, C>,
    spawn_fn: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
) -> Option<(SessionId, R)> {
    // Deduplicate the session key before spawning the process to avoid
    // orphaning a Kakoune process when insert() rejects a duplicate key.
    let spec = if ctx.session_manager.session_id_by_key(&spec.key).is_some() {
        let base = &spec.key;
        let mut deduped = None;
        for i in 2..=100 {
            let candidate = format!("{base}-{i}");
            if ctx.session_manager.session_id_by_key(&candidate).is_none() {
                deduped = Some(SessionSpec::new(
                    candidate,
                    spec.session.clone(),
                    spec.args.clone(),
                ));
                break;
            }
        }
        match deduped {
            Some(s) => s,
            None => {
                tracing::error!(key = spec.key, "failed to find unique session key");
                return None;
            }
        }
    } else {
        spec.clone()
    };
    let Ok((reader, writer, child)) = spawn_fn(&spec) else {
        tracing::error!("failed to spawn session {}", spec.key);
        return None;
    };
    let Ok(session_id) = ctx.session_manager.insert(spec, reader, writer, child) else {
        tracing::error!("failed to register spawned session");
        return None;
    };
    ctx.session_states.ensure_session(session_id, ctx.state);
    *ctx.dirty |= DirtyFlags::SESSION;
    let Ok(reader) = ctx.session_manager.take_reader(session_id) else {
        tracing::error!(session_id = session_id.0, "spawned session reader missing");
        return None;
    };
    if activate {
        if let Err(e) =
            ctx.session_manager
                .sync_and_activate(ctx.session_states, session_id, ctx.state)
        {
            tracing::error!("spawned session activation failed: {e:?}");
        }
        if !ctx.session_states.restore_into(session_id, ctx.state) {
            ctx.state.reset_for_session_switch();
        }
        *ctx.dirty |= DirtyFlags::ALL;
        *ctx.initial_resize_sent = false;
        sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    }
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    Some((session_id, reader))
}

/// Close a managed session by key, or the active session when `key` is `None`.
///
/// Returns `true` when the application should exit because no sessions remain.
pub fn close_session_core<R, W, C>(
    key: Option<&str>,
    ctx: &mut SessionMutContext<'_, R, W, C>,
) -> bool {
    let target = key
        .and_then(|k| ctx.session_manager.session_id_by_key(k))
        .or_else(|| ctx.session_manager.active_session_id());
    let Some(target) = target else {
        return false;
    };
    let was_active = ctx.session_manager.active_session_id() == Some(target);
    let _ = ctx.session_manager.close(target);
    ctx.session_states.remove(target);
    *ctx.dirty |= DirtyFlags::SESSION;
    if ctx.session_manager.is_empty() {
        return true;
    }
    if was_active {
        let restored = ctx
            .session_manager
            .active_session_id()
            .is_some_and(|active| ctx.session_states.restore_into(active, ctx.state));
        if !restored {
            ctx.state.reset_for_session_switch();
        }
        *ctx.dirty |= DirtyFlags::ALL;
        *ctx.initial_resize_sent = false;
    }
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
    false
}

/// Switch to an existing managed session by key.
///
/// No-op if the key doesn't exist or is already active.
pub fn switch_session_core<R, W, C>(key: &str, ctx: &mut SessionMutContext<'_, R, W, C>) {
    let Some(target) = ctx.session_manager.session_id_by_key(key) else {
        return;
    };
    if ctx.session_manager.active_session_id() == Some(target) {
        return;
    }
    if ctx
        .session_manager
        .sync_and_activate(ctx.session_states, target, ctx.state)
        .is_err()
    {
        tracing::warn!("session switch target {key} vanished");
        return;
    }
    if !ctx.session_states.restore_into(target, ctx.state) {
        ctx.state.reset_for_session_switch();
    }
    *ctx.dirty |= DirtyFlags::ALL | DirtyFlags::SESSION;
    *ctx.initial_resize_sent = false;
    sync_session_metadata(ctx.session_manager, ctx.session_states, ctx.state);
}

#[derive(Debug, Clone, Default)]
pub struct SessionReadyGate {
    active_session_key: Option<String>,
    generation: u64,
    notified_generation: Option<u64>,
    initial_resize_sent: bool,
}

impl SessionReadyGate {
    pub fn sync_active_session(&mut self, key: Option<&str>) -> bool {
        let next = key.map(str::to_owned);
        if self.active_session_key == next {
            return false;
        }
        self.active_session_key = next;
        self.generation += 1;
        self.notified_generation = None;
        self.initial_resize_sent = false;
        true
    }

    pub fn mark_initial_resize_sent(&mut self) {
        self.initial_resize_sent = true;
    }

    pub fn clear_initial_resize(&mut self) {
        self.initial_resize_sent = false;
    }

    pub fn should_notify_ready(&self) -> bool {
        self.active_session_key.is_some()
            && self.initial_resize_sent
            && self.notified_generation != Some(self.generation)
    }

    pub fn mark_ready_notified(&mut self) {
        self.notified_generation = Some(self.generation);
    }

    pub fn rearm_ready_notification(&mut self) {
        self.notified_generation = None;
    }
}

pub fn apply_ready_batch(batch: EffectsBatch, ctx: &mut DeferredContext<'_>) -> bool {
    *ctx.dirty |= batch.effects.redraw;

    for command in batch.effects.commands {
        match command {
            Command::SendToKakoune(request) => {
                if matches!(
                    execute_commands(
                        vec![Command::SendToKakoune(request)],
                        focused_writer!(ctx),
                        ctx.clipboard,
                    ),
                    CommandResult::Quit
                ) {
                    return true;
                }
            }
            Command::InsertText(text) => {
                if matches!(
                    execute_commands(
                        vec![Command::InsertText(text)],
                        focused_writer!(ctx),
                        ctx.clipboard,
                    ),
                    CommandResult::Quit
                ) {
                    return true;
                }
            }
            Command::PasteClipboard => {
                if let Some(text) = ctx.clipboard.get()
                    && dispatch_input_event(ctx, crate::input::InputEvent::Paste(text), 0)
                {
                    return true;
                }
            }
            Command::PluginMessage { target, payload } => {
                let batch =
                    ctx.registry
                        .deliver_message_batch(&target, payload, &AppView::new(ctx.state));
                if apply_runtime_batch(batch, ctx, Some(&target), 0) {
                    return true;
                }
            }
            Command::RequestRedraw(flags) => {
                *ctx.dirty |= flags;
            }
            _ => {
                tracing::warn!("SessionReady phase: unexpected command variant, dropping");
            }
        }
    }

    for plan in batch.effects.scroll_plans {
        (ctx.scroll_plan_sink)(plan);
    }

    false
}

// ── Generic session helpers (EventSink-based) ───────────────────

use super::EventSink;

/// Spawn a Kakoune reader thread that delivers events through an [`EventSink`].
pub fn spawn_session_reader<R, E: EventSink>(session_id: SessionId, reader: R, sink: E)
where
    R: std::io::BufRead + Send + 'static,
{
    let died_sink = sink.clone();
    crate::io::spawn_kak_reader(
        reader,
        move |req| {
            sink.send_kakoune(session_id, req);
        },
        move || {
            died_sink.send_died(session_id);
        },
    );
}

/// Session runtime generic over [`EventSink`].
///
/// Replaces the backend-specific `GuiSessionRuntime` / `TuiSessionRuntime`.
pub struct SharedSessionRuntime<'a, R, W, C, E: EventSink> {
    pub session_manager: &'a mut SessionManager<R, W, C>,
    pub session_states: &'a mut SessionStateStore,
    pub sink: E,
    pub spawn_session: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
}

impl<'a, R, W, C, E: EventSink> super::SessionRuntime for SharedSessionRuntime<'a, R, W, C, E>
where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    fn spawn_session(
        &mut self,
        spec: SessionSpec,
        activate: bool,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) {
        let mut ctx = SessionMutContext {
            session_manager: self.session_manager,
            session_states: self.session_states,
            state,
            dirty,
            initial_resize_sent,
        };
        if let Some((session_id, reader)) =
            spawn_session_core(&spec, activate, &mut ctx, self.spawn_session)
        {
            spawn_session_reader(session_id, reader, self.sink.clone());
        }
    }

    fn close_session(
        &mut self,
        key: Option<&str>,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) -> bool {
        let mut ctx = SessionMutContext {
            session_manager: self.session_manager,
            session_states: self.session_states,
            state,
            dirty,
            initial_resize_sent,
        };
        close_session_core(key, &mut ctx)
    }

    fn switch_session(
        &mut self,
        key: &str,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) {
        let mut ctx = SessionMutContext {
            session_manager: self.session_manager,
            session_states: self.session_states,
            state,
            dirty,
            initial_resize_sent,
        };
        switch_session_core(key, &mut ctx);
    }

    fn session_id_by_key(&self, key: &str) -> Option<SessionId> {
        self.session_manager.session_id_by_key(key)
    }
}

impl<'a, R, W, C, E: EventSink> super::SessionHost for SharedSessionRuntime<'a, R, W, C, E>
where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    fn active_writer(&mut self) -> &mut dyn std::io::Write {
        self.session_manager
            .active_writer_mut()
            .expect("missing active session writer")
    }

    fn writer_for_session(&mut self, session_id: SessionId) -> Option<&mut dyn std::io::Write> {
        self.session_manager
            .writer_mut(session_id)
            .ok()
            .map(|w| w as &mut dyn std::io::Write)
    }
}

/// Restore workspace panes from a persisted layout, generic over [`EventSink`].
#[allow(clippy::too_many_arguments)]
pub fn restore_panes<R, W, C, E: EventSink>(
    plan: &crate::workspace::persist::RestorePlan,
    server_name: &str,
    surface_registry: &mut SurfaceRegistry,
    session_manager: &mut SessionManager<R, W, C>,
    session_states: &mut SessionStateStore,
    state: &mut AppState,
    initial_resize_sent: &mut bool,
    spawn_fn: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
    sink: &E,
) where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    use crate::surface::buffer::ClientBufferSurface;
    use crate::workspace::persist;
    use std::collections::HashMap;

    let mut id_map: HashMap<String, SurfaceId> = HashMap::new();

    for pane in &plan.panes {
        let surface_id = surface_registry.workspace_mut().next_surface_id();

        let mut args = vec!["-c".to_string(), server_name.to_string()];
        if let Some(ref buf_name) = pane.buffer_name {
            args.push("-e".to_string());
            args.push(format!("buffer {}", persist::kak_quote(buf_name)));
        }

        let spec = SessionSpec::new(pane.pane_key.clone(), Some(server_name.to_string()), args);

        let mut dirty = DirtyFlags::empty();
        let mut ctx = SessionMutContext {
            session_manager,
            session_states,
            state,
            dirty: &mut dirty,
            initial_resize_sent,
        };

        let Some((session_id, reader)) = spawn_session_core(&spec, false, &mut ctx, spawn_fn)
        else {
            tracing::warn!(pane_key = pane.pane_key, "failed to spawn restored pane");
            continue;
        };

        spawn_session_reader(session_id, reader, sink.clone());

        surface_registry.register(Box::new(ClientBufferSurface::with_key(
            surface_id,
            &*pane.pane_key,
        )));
        surface_registry.bind_session(surface_id, session_id);
        surface_registry.mark_pending_resize(session_id);

        id_map.insert(pane.pane_key.clone(), surface_id);
    }

    if let Some(restored) = persist::build_restored_tree(&plan.saved.root, &id_map) {
        surface_registry
            .workspace_mut()
            .replace_root(restored.root, restored.next_id_min);

        if let Some(ref focused_key) = plan.saved.focused_key
            && let Some(surface_id) = surface_registry.surface_id_by_key(focused_key)
        {
            surface_registry.workspace_mut().focus(surface_id);
        }
    }

    sync_session_metadata(session_manager, session_states, state);
}
