//! Winit application loop: handles window events, GPU frame rendering, and input.

use std::borrow::Cow;
use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use kasane_core::clipboard::SystemClipboard;
use kasane_core::config::Config;
use kasane_core::event_loop::{
    DeferredContext, SessionReadyGate, apply_bootstrap_effects, handle_command_batch,
    handle_sourced_surface_commands, handle_workspace_divider_input,
    maybe_flush_active_session_ready, normalize_input_for_state, notify_workspace_observers,
    register_builtin_surfaces, route_surface_key_input, route_surface_text_input,
    surface_event_from_input, sync_session_ready_gate as sync_ready_gate,
};
use kasane_core::input::{InputEvent, resolve_text_input_target};
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    AppView, Command, IoEvent, PluginDiagnosticOverlayState, PluginManager, PluginRuntime,
    ProcessDispatcher, ProcessEvent, extract_redraw_flags, report_plugin_diagnostics,
};
use kasane_core::protocol::KasaneRequest;
use kasane_core::render::{RenderResult, SceneCache};
use kasane_core::render::{SceneRenderOptions, scene_render_pipeline_cached};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::SalsaInputHandles;
use kasane_core::scroll::ScrollRuntime;
use kasane_core::session::{SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, UpdateResult, update};
use kasane_core::surface::SurfaceRegistry;
use kasane_core::surface::pane_map::PaneStates;

use crate::animation::CursorAnimation;
use crate::backend::GuiBackend;
use crate::colors::ColorResolver;
use crate::diagnostics_overlay::build_diagnostic_overlay_commands;
use crate::gpu::GpuState;
use crate::gpu::scene_renderer::SceneRenderer;
use crate::ime::{
    GuiImeState, build_ime_overlay_commands, sync_ime_cursor_area as sync_window_ime_cursor_area,
};
use crate::input::{apply_modifiers, convert_window_event};
use crate::{GuiEvent, GuiEventSink};

pub struct App<R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    // winit
    window: Option<Arc<Window>>,

    // GPU
    gpu: Option<GpuState>,
    scene_renderer: Option<SceneRenderer>,

    // kasane-core
    state: Box<AppState>,
    registry: PluginRuntime,
    surface_registry: SurfaceRegistry,
    clipboard: SystemClipboard,
    backend: Option<GuiBackend>,

    // Kakoune communication
    session_manager: SessionManager<R, W, C>,
    session_states: SessionStateStore,
    session_spawner: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,

    // Event state
    pending_events: Vec<GuiEvent>,
    dirty: DirtyFlags,
    initial_resize_sent: bool,
    session_ready_gate: SessionReadyGate,

    // Input state
    current_modifiers: winit::keyboard::ModifiersState,
    cursor_pos: Option<(f64, f64)>,
    mouse_button_held: Option<kasane_core::input::MouseButton>,

    // Config
    config: Config,
    color_resolver: Option<ColorResolver>,
    scroll_amount: i32,
    scroll_runtime: ScrollRuntime,
    scroll_runtime_session: Option<kasane_core::session::SessionId>,

    // Scene cache
    scene_cache: SceneCache,

    // Cursor animation
    cursor_animation: CursorAnimation,
    cursor_dirty: bool,
    last_render_result: Option<RenderResult>,
    ime: GuiImeState,
    diagnostic_overlay: PluginDiagnosticOverlayState,

    // Event loop proxy for scheduling
    event_proxy: winit::event_loop::EventLoopProxy<GuiEvent>,

    // Event sink for generic schedulers
    gui_sink: GuiEventSink,

    // Process dispatcher for plugin process execution
    process_dispatcher: Box<dyn ProcessDispatcher>,

    // Salsa database
    salsa_db: KasaneDatabase,
    salsa_handles: SalsaInputHandles,
}

impl<R, W, C> App<R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    pub fn new(
        config: Config,
        mut session_manager: SessionManager<R, W, C>,
        session_spawner: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
        event_proxy: winit::event_loop::EventLoopProxy<GuiEvent>,
        plugin_manager: &mut PluginManager,
        registry: PluginRuntime,
        process_dispatcher: Box<dyn ProcessDispatcher>,
    ) -> Result<Self> {
        let scroll_amount = config.scroll.lines_per_scroll;

        let mut state = Box::new(AppState::default());
        let mut session_states = SessionStateStore::new();
        if let Some(active) = session_manager.active_session_id() {
            session_states.sync_from_active(active, &state);
        }
        kasane_core::event_loop::sync_session_metadata(
            &session_manager,
            &session_states,
            &mut state,
        );
        let mut registry = registry;

        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);

        // Collect plugin-owned surfaces before plugin init so invalid surface
        // contracts do not get a chance to produce side effects.
        let initial_plugins = plugin_manager.initialize(&mut registry, |_, registry| {
            kasane_core::event_loop::setup_plugin_surfaces(registry, &mut surface_registry, &state)
        })?;
        initial_plugins.apply_settings(&mut state);
        let mut diagnostic_overlay = PluginDiagnosticOverlayState::default();
        report_plugin_diagnostics(&initial_plugins.diagnostics);
        let gui_sink = GuiEventSink(event_proxy.clone());
        kasane_core::event_loop::schedule_diagnostic_overlay(
            &kasane_core::event_loop::GenericDiagnosticScheduler(gui_sink.clone()),
            &mut diagnostic_overlay,
            &initial_plugins.diagnostics,
        );

        let init_batch = registry.init_all_batch(&AppView::new(&state));
        let mut initial_dirty = DirtyFlags::ALL;
        apply_bootstrap_effects(init_batch.effects, &mut initial_dirty);
        kasane_core::event_loop::notify_workspace_observers(
            &mut registry,
            &surface_registry,
            &state,
        );
        let mut session_ready_gate = SessionReadyGate::default();
        sync_ready_gate(&mut session_ready_gate, &state);

        let (salsa_db, salsa_handles) = {
            let mut db = KasaneDatabase::default();
            let handles = SalsaInputHandles::new(&mut db);
            kasane_core::salsa_sync::sync_inputs_from_state(&mut db, &state, &handles);
            (db, handles)
        };
        let scroll_runtime_session = session_manager.active_session_id();

        // Bind initial session to the primary buffer surface
        if let Some(active) = session_manager.active_session_id() {
            surface_registry.bind_session(kasane_core::surface::SurfaceId::BUFFER, active);
        }
        if let Some(spec) = session_manager.active_spec()
            && let Some(ref name) = spec.session
        {
            surface_registry.set_server_session_name(name.clone());
        }

        // --- Layout restore ---
        let mut initial_resize_sent = false;
        if let Some(server_name) = surface_registry.server_session_name().map(str::to_owned)
            && let Some(saved) = kasane_core::workspace::persist::load_layout(&server_name)
            && let Some(plan) = kasane_core::workspace::persist::plan_restore(saved)
        {
            kasane_core::event_loop::restore_panes(
                &plan,
                &server_name,
                &mut surface_registry,
                &mut session_manager,
                &mut session_states,
                &mut state,
                &mut initial_resize_sent,
                session_spawner,
                &gui_sink,
            );
            kasane_core::event_loop::notify_workspace_observers(
                &mut registry,
                &surface_registry,
                &state,
            );
        }

        Ok(App {
            window: None,
            gpu: None,
            scene_renderer: None,
            state,
            registry,
            surface_registry,
            clipboard: SystemClipboard::new(),
            backend: None,
            session_manager,
            session_states,
            session_spawner,
            pending_events: Vec::new(),
            dirty: initial_dirty,
            initial_resize_sent,
            session_ready_gate,
            current_modifiers: winit::keyboard::ModifiersState::empty(),
            cursor_pos: None,
            mouse_button_held: None,
            scroll_amount,
            scroll_runtime: ScrollRuntime::default(),
            scroll_runtime_session,
            config,
            color_resolver: None,
            scene_cache: SceneCache::new(),
            cursor_animation: CursorAnimation::new(),
            cursor_dirty: false,
            last_render_result: None,
            ime: GuiImeState::default(),
            diagnostic_overlay,
            event_proxy: event_proxy.clone(),
            gui_sink,
            process_dispatcher,
            salsa_db,
            salsa_handles,
        })
    }

    fn init_window(&mut self, event_loop: &ActiveEventLoop) {
        let initial_cols = self.config.window.initial_cols as f64;
        let initial_rows = self.config.window.initial_rows as f64;
        // Approximate logical size (will be recalculated after font metrics)
        let logical_size = LogicalSize::new(initial_cols * 9.0, initial_rows * 18.0);

        let mut attrs = WindowAttributes::default()
            .with_title("kasane")
            .with_inner_size(logical_size)
            .with_maximized(self.config.window.maximized);

        if self.config.window.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

        let scale_factor = window.scale_factor();
        let phys_size = window.inner_size();

        // Initialize GPU
        match GpuState::new(window.clone(), self.config.window.present_mode.as_deref()) {
            Ok(gpu) => {
                let sr = SceneRenderer::new(
                    &gpu,
                    &self.config.font,
                    scale_factor,
                    phys_size,
                    self.event_proxy.clone(),
                );
                let metrics = sr.metrics().clone();

                // Setup color resolver
                let color_resolver = ColorResolver::from_config(&self.config.colors);

                // Setup state with measured dimensions
                self.state.cols = metrics.cols;
                self.state.rows = metrics.rows;
                self.state.apply_config(&self.config);

                // Setup backend
                let gui_backend = GuiBackend::new(metrics);
                self.backend = Some(gui_backend);

                self.color_resolver = Some(color_resolver);
                self.scene_renderer = Some(sr);
                self.gpu = Some(gpu);
            }
            Err(e) => {
                tracing::error!("GPU initialization failed: {e}");
                eprintln!("GPU initialization failed: {e}");
                eprintln!();
                print_gpu_troubleshooting();
                event_loop.exit();
                return;
            }
        }

        self.window = Some(window);
        self.sync_ime_binding();
    }

    fn toggle_fullscreen(&mut self) {
        if let Some(ref window) = self.window {
            let new = if window.fullscreen().is_some() {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            window.set_fullscreen(new);
        }
    }

    fn process_pending_events(&mut self, event_loop: &ActiveEventLoop) {
        let events: Vec<_> = self.pending_events.drain(..).collect();
        for event in events {
            match event {
                GuiEvent::Kakoune(session_id, req) => {
                    if self.session_manager.active_session_id() != Some(session_id) {
                        self.session_states
                            .ensure_session(session_id, &self.state)
                            .apply(req);
                        // Send the deferred initial Resize now that the kak process
                        // has proven it's initialized (it sent its first event).
                        if self.surface_registry.take_pending_resize(session_id)
                            && let Some(surface_id) =
                                self.surface_registry.surface_for_session(session_id)
                        {
                            let total = Rect {
                                x: 0,
                                y: 0,
                                w: self.state.cols,
                                h: self.state.rows,
                            };
                            let rects = self.surface_registry.workspace().compute_rects(total);
                            if let Some(rect) = rects.get(&surface_id)
                                && let Ok(writer) = self.session_manager.writer_mut(session_id)
                            {
                                // Per-pane status bar occupies 1 row from each pane.
                                let rows = rect.h.saturating_sub(1);
                                kasane_core::io::send_request(
                                    writer,
                                    &KasaneRequest::Resize { rows, cols: rect.w },
                                );
                                self.surface_registry
                                    .record_resize(session_id, rows, rect.w);
                            }
                        }
                        // If the session is a visible pane, trigger a redraw
                        if self
                            .surface_registry
                            .surface_for_session(session_id)
                            .is_some()
                        {
                            self.dirty |= DirtyFlags::ALL;
                        }
                        continue;
                    }
                    kasane_core::io::send_initial_resize(
                        self.session_manager
                            .active_writer_mut()
                            .expect("missing active session writer"),
                        &mut self.initial_resize_sent,
                        self.state.rows,
                        self.state.cols,
                    );
                    self.sync_session_ready_gate();
                    if self.initial_resize_sent {
                        self.session_ready_gate.mark_initial_resize_sent();
                    }
                    if self.flush_active_session_ready() {
                        event_loop.exit();
                        return;
                    }
                    let state = std::mem::take(&mut self.state);
                    let (
                        state,
                        UpdateResult {
                            flags,
                            commands,
                            scroll_plans,
                            source_plugin: _source,
                        },
                    ) = update(
                        state,
                        Msg::Kakoune(req),
                        &mut self.registry,
                        self.scroll_amount,
                    );
                    self.state = state;
                    let mut surface_command_groups = if flags.is_empty() {
                        vec![]
                    } else {
                        self.surface_registry
                            .on_state_changed_with_sources(&self.state, flags)
                    };
                    let extra_flags = surface_command_groups
                        .iter_mut()
                        .fold(DirtyFlags::empty(), |acc, entry| {
                            acc | extract_redraw_flags(&mut entry.commands)
                        });
                    let flags = flags | extra_flags;
                    self.dirty |= flags;
                    self.enqueue_scroll_plans(scroll_plans);
                    if self.exec_commands(commands) {
                        event_loop.exit();
                        return;
                    }
                    if self.exec_surface_command_groups(surface_command_groups) {
                        event_loop.exit();
                        return;
                    }
                    self.sync_session_ready_gate();
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                }
                GuiEvent::KakouneDied(session_id) => {
                    let mut session_ctx = kasane_core::event_loop::SessionMutContext {
                        session_manager: &mut self.session_manager,
                        session_states: &mut self.session_states,
                        state: &mut self.state,
                        dirty: &mut self.dirty,
                        initial_resize_sent: &mut self.initial_resize_sent,
                    };
                    if kasane_core::event_loop::handle_pane_death(
                        session_id,
                        &mut self.surface_registry,
                        &mut session_ctx,
                    ) {
                        event_loop.exit();
                        return;
                    }
                    // handle_pane_death may have reset initial_resize_sent.
                    if !self.initial_resize_sent {
                        kasane_core::io::send_initial_resize(
                            self.session_manager
                                .active_writer_mut()
                                .expect("missing active session writer"),
                            &mut self.initial_resize_sent,
                            self.state.rows,
                            self.state.cols,
                        );
                        self.sync_session_ready_gate();
                        if self.initial_resize_sent {
                            self.session_ready_gate.mark_initial_resize_sent();
                        }
                        if self.flush_active_session_ready() {
                            event_loop.exit();
                            return;
                        }
                    }
                    notify_workspace_observers(
                        &mut self.registry,
                        &self.surface_registry,
                        &self.state,
                    );
                    // Notify plugins of session change so cached state is updated.
                    let batch = self.registry.notify_state_changed_batch(
                        &AppView::new(&self.state),
                        DirtyFlags::SESSION,
                    );
                    if self.apply_runtime_batch(batch, None) {
                        event_loop.exit();
                        return;
                    }
                    self.sync_session_ready_gate();
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                    continue;
                }
                GuiEvent::PluginTimer(target, payload) => {
                    let batch = self.registry.deliver_message_batch(
                        &target,
                        payload.0,
                        &AppView::new(&self.state),
                    );
                    if self.apply_runtime_batch(batch, Some(&target)) {
                        event_loop.exit();
                        return;
                    }
                    self.sync_session_ready_gate();
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                }
                GuiEvent::ProcessOutput(plugin_id, io_event) => {
                    let batch = self.registry.deliver_io_event_batch(
                        &plugin_id,
                        &io_event,
                        &AppView::new(&self.state),
                    );
                    // Free per-plugin process count slot when a job finishes
                    let IoEvent::Process(ref pe) = io_event;
                    let finished_job = match pe {
                        ProcessEvent::Exited { job_id, .. }
                        | ProcessEvent::SpawnFailed { job_id, .. } => Some(*job_id),
                        _ => None,
                    };
                    if let Some(job_id) = finished_job {
                        self.process_dispatcher
                            .remove_finished_job(&plugin_id, job_id);
                    }
                    if self.apply_runtime_batch(batch, Some(&plugin_id)) {
                        event_loop.exit();
                        return;
                    }
                    self.sync_session_ready_gate();
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                }
                GuiEvent::DiagnosticOverlayExpire(generation) => {
                    if self.diagnostic_overlay.dismiss(generation) {
                        self.dirty |= DirtyFlags::ALL;
                    }
                }
                GuiEvent::ImageLoaded(key, result) => {
                    if let (Some(gpu), Some(sr)) = (self.gpu.as_ref(), self.scene_renderer.as_mut())
                        && sr.finalize_image_load(key, result, gpu)
                    {
                        self.dirty |= DirtyFlags::ALL;
                    }
                }
            }
        }
        self.sync_ime_binding();
    }

    fn drain_runtime_diagnostics(&mut self) {
        let diagnostics = self.registry.drain_all_diagnostics();
        if !diagnostics.is_empty() {
            report_plugin_diagnostics(&diagnostics);
            kasane_core::event_loop::schedule_diagnostic_overlay(
                &kasane_core::event_loop::GenericDiagnosticScheduler(self.gui_sink.clone()),
                &mut self.diagnostic_overlay,
                &diagnostics,
            );
        }
    }

    fn handle_input_event(&mut self, input: InputEvent, event_loop: &ActiveEventLoop) {
        let input = normalize_input_for_state(input, &self.state);
        let total = Rect {
            x: 0,
            y: 0,
            w: self.state.cols,
            h: self.state.rows,
        };
        let (mut flags, commands, source, mut surface_command_groups, scroll_plans) =
            if let Some(dirty) =
                handle_workspace_divider_input(&input, &mut self.surface_registry, total)
            {
                if !dirty.is_empty() {
                    notify_workspace_observers(
                        &mut self.registry,
                        &self.surface_registry,
                        &self.state,
                    );
                }
                (dirty, vec![], None, vec![], vec![])
            } else if let Some(surface_commands) =
                route_surface_key_input(&input, &mut self.surface_registry, &self.state, total)
            {
                (
                    DirtyFlags::empty(),
                    vec![],
                    None,
                    vec![surface_commands],
                    vec![],
                )
            } else if let Some(surface_commands) =
                route_surface_text_input(&input, &mut self.surface_registry, &self.state, total)
            {
                (
                    DirtyFlags::empty(),
                    vec![],
                    None,
                    vec![surface_commands],
                    vec![],
                )
            } else {
                let surface_event = surface_event_from_input(&input);
                let msg = Msg::from(input);
                let state = std::mem::take(&mut self.state);
                let (
                    state,
                    UpdateResult {
                        flags,
                        commands,
                        scroll_plans,
                        source_plugin,
                    },
                ) = update(state, msg, &mut self.registry, self.scroll_amount);
                self.state = state;
                let surface_command_groups = surface_event
                    .map(|event| {
                        self.surface_registry
                            .route_event_with_sources(event, &self.state, total)
                    })
                    .unwrap_or_default();
                (
                    flags,
                    commands,
                    source_plugin,
                    surface_command_groups,
                    scroll_plans,
                )
            };
        for entry in &mut surface_command_groups {
            flags |= extract_redraw_flags(&mut entry.commands);
        }
        self.dirty |= flags;
        // Suppress commands to Kakoune until initialization is complete.
        // Data sent before m_on_key is set may be misinterpreted as raw key input.
        if self.initial_resize_sent {
            self.enqueue_scroll_plans(scroll_plans);
            if self.exec_commands_from(commands, source.as_ref()) {
                event_loop.exit();
                return;
            }
            if self.exec_surface_command_groups(surface_command_groups) {
                event_loop.exit();
                return;
            }
        }
        self.sync_scroll_runtime();
        self.session_states
            .sync_active_from_manager(&self.session_manager, &self.state);
        self.sync_session_ready_gate();
    }

    fn sync_scroll_runtime(&mut self) {
        let active_session = self.session_manager.active_session_id();
        if self.scroll_runtime_session != active_session {
            self.scroll_runtime.advance_generation();
            self.scroll_runtime.suspend();
            self.scroll_runtime_session = active_session;
        }
        self.scroll_runtime
            .set_initial_resize_complete(self.initial_resize_sent);
    }

    fn enqueue_scroll_plans(&mut self, scroll_plans: Vec<kasane_core::scroll::ScrollPlan>) {
        for plan in scroll_plans {
            self.scroll_runtime.enqueue(plan);
        }
    }

    fn apply_runtime_batch(
        &mut self,
        mut batch: kasane_core::plugin::EffectsBatch,
        source_plugin: Option<&kasane_core::plugin::PluginId>,
    ) -> bool {
        self.dirty |= batch.effects.redraw;
        self.enqueue_scroll_plans(std::mem::take(&mut batch.effects.scroll_plans));
        self.dirty |= extract_redraw_flags(&mut batch.effects.commands);
        self.exec_commands_from(batch.effects.commands, source_plugin)
    }

    fn flush_active_session_ready(&mut self) -> bool {
        self.with_deferred_context(maybe_flush_active_session_ready)
    }

    fn sync_session_ready_gate(&mut self) {
        sync_ready_gate(&mut self.session_ready_gate, &self.state);
        if !self.initial_resize_sent {
            self.session_ready_gate.clear_initial_resize();
        }
    }

    /// Execute side-effect commands, including deferred ones. Returns `true` if Quit was requested.
    fn exec_commands(&mut self, commands: Vec<Command>) -> bool {
        self.exec_commands_from(commands, None)
    }

    /// Execute side-effect commands with an optional source plugin ID for process dispatch.
    fn exec_commands_from(
        &mut self,
        commands: Vec<Command>,
        source_plugin: Option<&kasane_core::plugin::PluginId>,
    ) -> bool {
        self.with_deferred_context(|ctx| handle_command_batch(commands, ctx, source_plugin))
    }

    fn exec_surface_command_groups(
        &mut self,
        surface_command_groups: Vec<kasane_core::surface::SourcedSurfaceCommands>,
    ) -> bool {
        self.with_deferred_context(|ctx| {
            handle_sourced_surface_commands(surface_command_groups, ctx)
        })
    }

    /// Build a `DeferredContext` from `self` fields and pass it to the closure.
    fn with_deferred_context<T>(&mut self, f: impl FnOnce(&mut DeferredContext<'_>) -> T) -> T {
        let timer = kasane_core::event_loop::GenericTimerScheduler(self.gui_sink.clone());
        let spawn_session = self.session_spawner;
        let mut session_runtime = kasane_core::event_loop::SharedSessionRuntime {
            session_manager: &mut self.session_manager,
            session_states: &mut self.session_states,
            sink: self.gui_sink.clone(),
            spawn_session,
        };
        let scroll_runtime = &mut self.scroll_runtime;
        let mut workspace_changed = false;
        let result = {
            let mut ctx = DeferredContext {
                state: &mut self.state,
                registry: &mut self.registry,
                surface_registry: &mut self.surface_registry,
                clipboard: &mut self.clipboard,
                dirty: &mut self.dirty,
                timer: &timer,
                session_host: &mut session_runtime,
                initial_resize_sent: &mut self.initial_resize_sent,
                session_ready_gate: Some(&mut self.session_ready_gate),
                scroll_plan_sink: &mut |plan| scroll_runtime.enqueue(plan),
                process_dispatcher: &mut *self.process_dispatcher,
                workspace_changed: &mut workspace_changed,
                scroll_amount: self.scroll_amount,
            };
            f(&mut ctx)
        };
        if workspace_changed {
            notify_workspace_observers(&mut self.registry, &self.surface_registry, &self.state);
            // Save layout on structural changes
            if let Some(server_name) = self.surface_registry.server_session_name() {
                kasane_core::workspace::persist::save_layout(
                    server_name,
                    self.surface_registry.workspace(),
                    &self.surface_registry,
                    &self.session_states,
                    &self.state,
                    self.session_manager.active_session_id(),
                );
            }
        }
        result
    }

    fn handle_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if let Some(ref mut gpu) = self.gpu {
            gpu.resize(size.width, size.height);
        }
        let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor());

        let metrics = if let (Some(sr), Some(gpu)) = (&mut self.scene_renderer, &self.gpu) {
            sr.resize(gpu, &self.config.font, scale, size);
            sr.metrics().clone()
        } else {
            return;
        };

        self.state.cols = metrics.cols;
        self.state.rows = metrics.rows;
        if let Some(ref mut backend) = self.backend {
            backend.update_metrics(metrics);
        }
        // Send resize to Kakoune
        if self.initial_resize_sent {
            let resize = KasaneRequest::Resize {
                rows: self.state.available_height(),
                cols: self.state.cols,
            };
            kasane_core::io::send_request(
                self.session_manager
                    .active_writer_mut()
                    .expect("missing active session writer"),
                &resize,
            );
        }
        self.dirty = DirtyFlags::ALL;
        self.session_states
            .sync_active_from_manager(&self.session_manager, &self.state);
        notify_workspace_observers(&mut self.registry, &self.surface_registry, &self.state);
    }

    fn request_redraw(&self) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    fn sync_ime_binding(&mut self) {
        let target =
            resolve_text_input_target(&self.state, self.session_manager.active_session_id());
        let target_changed = self.ime.bind_target(target);
        let allowed = target.is_some();
        let allowed_changed = self.ime.policy_allowed != allowed;

        self.ime.policy_allowed = allowed;
        if !allowed {
            self.ime.platform_enabled = false;
        }

        if allowed_changed && let Some(ref window) = self.window {
            window.set_ime_allowed(allowed);
        }

        if target_changed && self.ime.overlay_dirty {
            self.request_redraw();
        }
        self.sync_ime_cursor_area();
    }

    fn sync_ime_cursor_area(&self) {
        let (Some(window), Some(sr), Some(render_result)) = (
            self.window.as_ref(),
            self.scene_renderer.as_ref(),
            self.last_render_result,
        ) else {
            return;
        };

        sync_window_ime_cursor_area(window, &self.ime, render_result, sr.metrics());
    }

    fn handle_ime_event(&mut self, ime: &Ime, event_loop: &ActiveEventLoop) {
        match ime {
            Ime::Enabled => {
                self.ime.platform_enabled = true;
                self.sync_ime_cursor_area();
            }
            Ime::Preedit(text, range) => {
                if self.ime.set_preedit(text.clone(), *range) {
                    self.request_redraw();
                }
                self.sync_ime_cursor_area();
            }
            Ime::Commit(text) => {
                let had_overlay = self.ime.clear_preedit();
                self.sync_ime_cursor_area();
                if had_overlay {
                    self.request_redraw();
                }
                if !text.is_empty() {
                    self.handle_input_event(InputEvent::TextInput(text.clone()), event_loop);
                }
            }
            Ime::Disabled => {
                self.ime.platform_enabled = false;
                if self.ime.clear_preedit() {
                    self.request_redraw();
                }
            }
        }
    }

    fn render_frame(&mut self) {
        let Some(ref mut gpu) = self.gpu else {
            tracing::warn!("[app] render_frame skipped: missing gpu/resolver");
            return;
        };
        // Attempt recovery if device reported an error
        if gpu
            .device_error
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            tracing::warn!("[app] device error detected, reconfiguring surface");
            gpu.surface.configure(&gpu.device, &gpu.config);
        }
        let gpu = self.gpu.as_ref().unwrap();
        let Some(_) = self.color_resolver.as_ref() else {
            tracing::warn!("[app] render_frame skipped: missing gpu/resolver");
            return;
        };
        self.color_resolver
            .as_mut()
            .expect("resolver checked above")
            .sync_defaults(&self.state.default_face);
        tracing::debug!(
            "[app] render_frame start ({}x{})",
            self.state.cols,
            self.state.rows
        );
        let ime_overlay_face = if self.state.is_prompt_mode() {
            self.state.status_default_face
        } else {
            self.state.default_face
        };

        let Some(ref mut sr) = self.scene_renderer else {
            return;
        };

        let cell_size = sr.cell_size();

        // Send resize commands to pane clients when layout may have changed
        if !self.dirty.is_empty() {
            let total = kasane_core::layout::Rect {
                x: 0,
                y: 0,
                w: self.state.cols,
                h: self.state.rows,
            };
            let spawn_session = self.session_spawner;
            let mut session_runtime = kasane_core::event_loop::SharedSessionRuntime {
                session_manager: &mut self.session_manager,
                session_states: &mut self.session_states,
                sink: self.gui_sink.clone(),
                spawn_session,
            };
            kasane_core::event_loop::send_pane_resizes(
                &mut self.surface_registry,
                &mut session_runtime,
                total,
            );
        }

        // Only run the pipeline when there are actual dirty flags.
        // Cursor-only animation reuses the cached scene commands.
        if !self.dirty.is_empty() {
            self.surface_registry.sync_ephemeral_surfaces(&self.state);
            self.registry.prepare_plugin_cache(self.dirty);

            // Sync Salsa inputs from updated state
            kasane_core::event_loop::sync_salsa_for_render(
                &mut self.salsa_db,
                &self.state,
                &self.registry,
                &mut self.salsa_handles,
            );
            let view = self.registry.view();

            let pane_states_val;
            let pane_states_opt = if self.surface_registry.is_multi_pane() {
                pane_states_val = PaneStates::from_registry(
                    &self.surface_registry,
                    &self.session_states,
                    &self.state,
                    self.session_manager.active_session_id(),
                );
                Some(&pane_states_val)
            } else {
                None
            };

            let (commands, result, display_map) = scene_render_pipeline_cached(
                &self.salsa_db,
                &self.salsa_handles,
                &self.state,
                &view,
                cell_size,
                self.dirty,
                &mut self.scene_cache,
                SceneRenderOptions {
                    surface_registry: Some(&self.surface_registry),
                    pane_states: pane_states_opt,
                },
            );
            self.last_render_result = Some(result);
            if let Some(ref window) = self.window {
                sync_window_ime_cursor_area(window, &self.ime, result, sr.metrics());
            }
            self.state.display_scroll_offset = result.display_scroll_offset;
            self.state.display_map = Some(display_map);
            self.state.display_unit_map = self
                .state
                .display_map
                .as_ref()
                .filter(|dm| !dm.is_identity())
                .map(|dm| kasane_core::display::DisplayUnitMap::build(dm));
            let overlay_commands = build_diagnostic_overlay_commands(
                &self.diagnostic_overlay,
                cell_size,
                self.state.cols,
                self.state.rows,
            );
            let ime_overlay_commands =
                build_ime_overlay_commands(&self.ime, result, cell_size, ime_overlay_face);
            let mut overlay_commands = overlay_commands;
            overlay_commands.extend(ime_overlay_commands);
            let frame_commands = append_overlay_commands(commands, overlay_commands);

            let (cw, ch) = (sr.metrics().cell_width, sr.metrics().cell_height);
            let resolver = self
                .color_resolver
                .as_ref()
                .expect("resolver checked above");
            submit_render(
                sr,
                gpu,
                resolver,
                &frame_commands,
                &mut self.cursor_animation,
                result,
                cw,
                ch,
                "scene render",
            );

            // Rebuild HitMap from cached view tree for plugin mouse routing
            kasane_core::event_loop::rebuild_hit_map(
                &mut self.state,
                &self.registry,
                &self.surface_registry,
            );
        } else if let Some(result) = self.last_render_result {
            // Cursor-only frame: reuse cached scene commands
            let _cursor_span = tracing::info_span!("cursor_only_frame").entered();
            let commands = self.scene_cache.composed_ref();
            if let Some(ref window) = self.window {
                sync_window_ime_cursor_area(window, &self.ime, result, sr.metrics());
            }
            let overlay_commands = build_diagnostic_overlay_commands(
                &self.diagnostic_overlay,
                cell_size,
                self.state.cols,
                self.state.rows,
            );
            let ime_overlay_commands =
                build_ime_overlay_commands(&self.ime, result, cell_size, ime_overlay_face);
            let mut overlay_commands = overlay_commands;
            overlay_commands.extend(ime_overlay_commands);
            let frame_commands = append_overlay_commands(commands, overlay_commands);
            let (cw, ch) = (sr.metrics().cell_width, sr.metrics().cell_height);
            let resolver = self
                .color_resolver
                .as_ref()
                .expect("resolver checked above");
            submit_render(
                sr,
                gpu,
                resolver,
                &frame_commands,
                &mut self.cursor_animation,
                result,
                cw,
                ch,
                "cursor-only",
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn submit_render(
    sr: &mut SceneRenderer,
    gpu: &GpuState,
    resolver: &ColorResolver,
    commands: &[kasane_core::render::DrawCommand],
    cursor_animation: &mut CursorAnimation,
    result: RenderResult,
    cell_width: f32,
    cell_height: f32,
    label: &str,
) {
    cursor_animation.apply_hints(result.cursor_blink, result.cursor_movement);
    cursor_animation.update_target(result.cursor_x, result.cursor_y);
    let cursor_state = cursor_animation.tick(cell_width, cell_height);
    tracing::debug!("[app] {label}: {} commands", commands.len());
    match sr.render_with_cursor(
        gpu,
        commands,
        resolver,
        result.cursor_style,
        &cursor_state,
        result.cursor_color,
    ) {
        Ok(()) => tracing::debug!("[app] render_frame complete ({label})"),
        Err(e) => tracing::error!("[app] scene render failed: {e}"),
    }
}

fn append_overlay_commands(
    base_commands: &[kasane_core::render::DrawCommand],
    overlay_commands: Vec<kasane_core::render::DrawCommand>,
) -> Cow<'_, [kasane_core::render::DrawCommand]> {
    if overlay_commands.is_empty() {
        return Cow::Borrowed(base_commands);
    }

    let mut combined = Vec::with_capacity(base_commands.len() + overlay_commands.len());
    combined.extend_from_slice(base_commands);
    combined.extend(overlay_commands);
    Cow::Owned(combined)
}

impl<R, W, C> Drop for App<R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    fn drop(&mut self) {
        // Save workspace layout before shutdown — but only if sessions are still alive.
        // When all sessions died via :q, the workspace is already degraded to a single
        // pane; saving now would delete the layout file and prevent daemon survival for
        // reconnect.
        if !self.session_manager.is_empty()
            && let Some(server_name) = self.surface_registry.server_session_name()
        {
            kasane_core::workspace::persist::save_layout(
                server_name,
                self.surface_registry.workspace(),
                &self.surface_registry,
                &self.session_states,
                &self.state,
                self.session_manager.active_session_id(),
            );
        }
        self.registry.shutdown_all();
    }
}

impl<R, W, C> ApplicationHandler<GuiEvent> for App<R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracing::info!("[app] resumed, window exists: {}", self.window.is_some());
        if self.window.is_none() {
            self.init_window(event_loop);
            tracing::info!(
                "[app] window initialized, gpu: {}, renderer: {}",
                self.gpu.is_some(),
                self.scene_renderer.is_some()
            );
            self.sync_ime_binding();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.current_modifiers = mods.state();
                return;
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(*size);
                return;
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // Handled via Resized which follows
                return;
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == winit::event::ElementState::Pressed
                    && event.logical_key
                        == winit::keyboard::Key::Named(winit::keyboard::NamedKey::F11) =>
            {
                self.toggle_fullscreen();
                return;
            }

            WindowEvent::RedrawRequested => {
                if !self.dirty.is_empty() || self.cursor_dirty || self.ime.overlay_dirty {
                    self.render_frame();
                    self.dirty = DirtyFlags::empty();
                    self.cursor_dirty = false;
                    self.ime.overlay_dirty = false;
                }
                return;
            }
            WindowEvent::Focused(focused) => {
                if *focused {
                    self.cursor_animation.resume();
                } else {
                    self.cursor_animation.pause();
                    self.ime.platform_enabled = false;
                    if self.ime.clear_preedit() {
                        self.request_redraw();
                    }
                }
                // Fall through to input conversion so plugins can observe focus
            }
            WindowEvent::Ime(ime) => {
                self.handle_ime_event(ime, event_loop);
                return;
            }
            _ => {}
        }

        // Convert input events
        let Some(ref sr) = self.scene_renderer else {
            return;
        };
        let metrics = sr.metrics();
        let mut input_events = convert_window_event(
            &event,
            metrics,
            &mut self.cursor_pos,
            &mut self.mouse_button_held,
        );

        // Apply modifier state
        for ie in &mut input_events {
            apply_modifiers(ie, &self.current_modifiers);
        }

        for ie in input_events {
            self.handle_input_event(ie, event_loop);
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: GuiEvent) {
        tracing::debug!(
            "[app] user_event received, pending: {}",
            self.pending_events.len()
        );
        self.pending_events.push(event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let _frame_span = tracing::debug_span!("frame").entered();
        let pending_count = self.pending_events.len();
        tracing::trace!(
            "[app] about_to_wait, pending: {}, dirty: {:?}",
            pending_count,
            self.dirty
        );
        if pending_count > 1 {
            tracing::debug!(batch_count = pending_count, "event batch drained");
        }
        self.process_pending_events(event_loop);
        self.drain_runtime_diagnostics();
        self.sync_scroll_runtime();

        // Host-owned smooth scroll runtime tick
        if let Some(resolved) = self.scroll_runtime.tick() {
            let focused_surface = self.surface_registry.workspace().focused();
            let focused_sid = self.surface_registry.session_for_surface(focused_surface);
            let writer = match focused_sid.and_then(|sid| self.session_manager.writer_mut(sid).ok())
            {
                Some(w) => w,
                None => self
                    .session_manager
                    .active_writer_mut()
                    .expect("missing active session writer"),
            };
            kasane_core::plugin::execute_commands(
                vec![Command::SendToKakoune(resolved.to_kakoune_request())],
                writer,
                &mut self.clipboard,
            );
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
            self.session_states
                .sync_active_from_manager(&self.session_manager, &self.state);
        }

        // Cursor animation drives continuous redraw when active
        if self.cursor_animation.is_animating
            && let Some(ref window) = self.window
        {
            window.request_redraw();
            self.cursor_dirty = true;
        }

        if !self.dirty.is_empty()
            && let Some(ref window) = self.window
        {
            window.request_redraw();
        }

        if self.ime.overlay_dirty {
            self.request_redraw();
        }

        let scroll_deadline = self
            .scroll_runtime
            .active_frame_interval()
            .map(|d| std::time::Instant::now() + d);
        let cursor_deadline = self.cursor_animation.next_frame_deadline();
        let deadline = match (scroll_deadline, cursor_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        match deadline {
            Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn print_gpu_troubleshooting() {
    #[cfg(target_os = "linux")]
    {
        eprintln!("Troubleshooting:");
        eprintln!("  Install a Vulkan driver:");
        eprintln!("    Arch:   pacman -S vulkan-icd-loader mesa-vulkan-drivers");
        eprintln!("    Debian: apt install mesa-vulkan-drivers");
        eprintln!("    Fedora: dnf install mesa-vulkan-drivers");
    }
    #[cfg(target_os = "macos")]
    {
        eprintln!("Troubleshooting:");
        eprintln!("  Metal should be available on macOS. Try updating macOS.");
    }
    eprintln!();
    eprintln!("To use the terminal backend instead: kasane --ui tui");
}
