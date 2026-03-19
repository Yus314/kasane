//! Winit application loop: handles window events, GPU frame rendering, and input.

use std::io::Write;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use kasane_core::config::Config;
use kasane_core::event_loop::{
    DeferredContext, TimerScheduler, handle_deferred_commands, handle_sourced_surface_commands,
    handle_workspace_divider_input, surface_event_from_input,
};
use kasane_core::input::InputEvent;
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    Command, CommandResult, IoEvent, PluginRegistry, ProcessDispatcher, ProcessEvent,
    execute_commands, extract_deferred_commands, extract_redraw_flags, extract_scroll_plans,
};
use kasane_core::protocol::KasaneRequest;
use kasane_core::render::scene_render_pipeline_cached;
use kasane_core::render::{CellGrid, RenderBackend, RenderResult, SceneCache};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::{
    SalsaInputHandles, sync_display_directives, sync_inputs_from_state, sync_plugin_contributions,
    sync_plugin_epoch,
};
use kasane_core::scroll::{ScrollPlan, ScrollRuntime};
use kasane_core::session::{SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, update};
use kasane_core::surface::SurfaceRegistry;
use kasane_core::surface::buffer::KakouneBufferSurface;

use crate::animation::CursorAnimation;
use crate::backend::GuiBackend;
use crate::colors::ColorResolver;
use crate::gpu::GpuState;
use crate::gpu::scene_renderer::SceneRenderer;
use crate::input::{apply_modifiers, convert_window_event};
use crate::{GuiEvent, TimerPayload, spawn_session_reader};

/// TimerScheduler that injects timer events into the winit event loop.
struct GuiTimerScheduler(winit::event_loop::EventLoopProxy<GuiEvent>);

impl TimerScheduler for GuiTimerScheduler {
    fn schedule_timer(
        &self,
        delay: std::time::Duration,
        target: kasane_core::plugin::PluginId,
        payload: Box<dyn std::any::Any + Send>,
    ) {
        let proxy = self.0.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = proxy.send_event(GuiEvent::PluginTimer(target, TimerPayload(payload)));
        });
    }
}

struct GuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    session_manager: &'a mut SessionManager<R, W, C>,
    session_states: &'a mut SessionStateStore,
    proxy: winit::event_loop::EventLoopProxy<GuiEvent>,
    spawn_session: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
}

impl<'a, R, W, C> kasane_core::event_loop::SessionRuntime for GuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
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
        if let Some((session_id, reader)) = kasane_core::event_loop::spawn_session_core(
            &spec,
            activate,
            self.session_manager,
            self.session_states,
            state,
            dirty,
            initial_resize_sent,
            self.spawn_session,
        ) {
            spawn_session_reader(session_id, reader, self.proxy.clone());
        }
    }

    fn close_session(
        &mut self,
        key: Option<&str>,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) -> bool {
        kasane_core::event_loop::close_session_core(
            key,
            self.session_manager,
            self.session_states,
            state,
            dirty,
            initial_resize_sent,
        )
    }

    fn switch_session(
        &mut self,
        key: &str,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) {
        kasane_core::event_loop::switch_session_core(
            key,
            self.session_manager,
            self.session_states,
            state,
            dirty,
            initial_resize_sent,
        );
    }
}

impl<'a, R, W, C> kasane_core::event_loop::SessionHost for GuiSessionRuntime<'a, R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    fn active_writer(&mut self) -> &mut dyn Write {
        self.session_manager
            .active_writer_mut()
            .expect("missing active session writer")
    }
}

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
    state: AppState,
    registry: PluginRegistry,
    surface_registry: SurfaceRegistry,
    grid: CellGrid, // used for resize tracking
    backend: Option<GuiBackend>,

    // Kakoune communication
    session_manager: SessionManager<R, W, C>,
    session_states: SessionStateStore,
    session_spawner: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,

    // Event state
    pending_events: Vec<GuiEvent>,
    dirty: DirtyFlags,
    initial_resize_sent: bool,

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

    // Timer scheduler for plugin timer events
    timer_scheduler: GuiTimerScheduler,

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
        session_manager: SessionManager<R, W, C>,
        session_spawner: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
        event_proxy: winit::event_loop::EventLoopProxy<GuiEvent>,
        registry: PluginRegistry,
        process_dispatcher: Box<dyn ProcessDispatcher>,
    ) -> Self {
        let scroll_amount = config.scroll.lines_per_scroll;

        let mut state = AppState::default();
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
        surface_registry
            .try_register(Box::new(KakouneBufferSurface::new()))
            .expect("failed to register built-in surface kasane.buffer");
        surface_registry
            .try_register(Box::new(
                kasane_core::surface::status::StatusBarSurface::new(),
            ))
            .expect("failed to register built-in surface kasane.status");

        // Collect plugin-owned surfaces before plugin init so invalid surface
        // contracts do not get a chance to produce side effects.
        kasane_core::event_loop::setup_plugin_surfaces(
            &mut registry,
            &mut surface_registry,
            &state,
        );

        let _init_commands = registry.init_all(&state);
        // init_commands will be executed once initial_resize_sent is true

        let (salsa_db, salsa_handles) = {
            let mut db = KasaneDatabase::default();
            let handles = SalsaInputHandles::new(&mut db);
            sync_inputs_from_state(&mut db, &state, &handles);
            (db, handles)
        };
        let scroll_runtime_session = session_manager.active_session_id();

        App {
            window: None,
            gpu: None,
            scene_renderer: None,
            state,
            registry,
            surface_registry,
            grid: CellGrid::new(1, 1),
            backend: None,
            session_manager,
            session_states,
            session_spawner,
            pending_events: Vec::new(),
            dirty: DirtyFlags::ALL,
            initial_resize_sent: false,
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
            timer_scheduler: GuiTimerScheduler(event_proxy),
            process_dispatcher,
            salsa_db,
            salsa_handles,
        }
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
        window.set_ime_allowed(true);

        let scale_factor = window.scale_factor();
        let phys_size = window.inner_size();

        // Initialize GPU
        match GpuState::new(window.clone()) {
            Ok(gpu) => {
                let sr = SceneRenderer::new(&gpu, &self.config.font, scale_factor, phys_size);
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
                event_loop.exit();
                return;
            }
        }

        self.window = Some(window);
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
                    let (flags, commands, _source) = update(
                        &mut self.state,
                        Msg::Kakoune(req),
                        &mut self.registry,
                        self.scroll_amount,
                    );
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
                    if flags.contains(DirtyFlags::ALL) {
                        self.grid.resize(self.state.cols, self.state.rows);
                        self.grid.invalidate_all();
                    }
                    self.dirty |= flags;
                    if self.exec_commands(commands) {
                        event_loop.exit();
                        return;
                    }
                    if self.exec_surface_command_groups(surface_command_groups) {
                        event_loop.exit();
                        return;
                    }
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                }
                GuiEvent::KakouneDied(session_id) => {
                    if kasane_core::event_loop::handle_session_death(
                        session_id,
                        &mut self.session_manager,
                        &mut self.session_states,
                        &mut self.state,
                        &mut self.dirty,
                        &mut self.initial_resize_sent,
                    ) {
                        event_loop.exit();
                        return;
                    }
                    // handle_session_death may have reset initial_resize_sent.
                    if !self.initial_resize_sent {
                        kasane_core::io::send_initial_resize(
                            self.session_manager
                                .active_writer_mut()
                                .expect("missing active session writer"),
                            &mut self.initial_resize_sent,
                            self.state.rows,
                            self.state.cols,
                        );
                    }
                    // Notify plugins of session change so cached state is updated.
                    for plugin in self.registry.plugins_mut() {
                        plugin.on_state_changed(&self.state, DirtyFlags::SESSION);
                    }
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                    continue;
                }
                GuiEvent::PluginTimer(target, payload) => {
                    let (flags, commands) =
                        self.registry
                            .deliver_message(&target, payload.0, &self.state);
                    self.dirty |= flags;
                    if self.exec_commands(commands) {
                        event_loop.exit();
                        return;
                    }
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                }
                GuiEvent::ProcessOutput(plugin_id, io_event) => {
                    let (flags, commands) =
                        self.registry
                            .deliver_io_event(&plugin_id, &io_event, &self.state);
                    self.dirty |= flags;
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
                    if self.exec_commands_from(commands, Some(&plugin_id)) {
                        event_loop.exit();
                        return;
                    }
                    self.session_states
                        .sync_active_from_manager(&self.session_manager, &self.state);
                }
            }
        }
    }

    fn handle_input_event(&mut self, input: InputEvent, event_loop: &ActiveEventLoop) {
        let total = Rect {
            x: 0,
            y: 0,
            w: self.state.cols,
            h: self.state.rows,
        };
        let (mut flags, commands, source, mut surface_command_groups) = if let Some(dirty) =
            handle_workspace_divider_input(&input, &mut self.surface_registry, total)
        {
            (dirty, vec![], None, vec![])
        } else {
            let surface_event = surface_event_from_input(&input);
            let msg = Msg::from(input);
            let (flags, commands, source) =
                update(&mut self.state, msg, &mut self.registry, self.scroll_amount);
            let surface_command_groups = surface_event
                .map(|event| {
                    self.surface_registry
                        .route_event_with_sources(event, &self.state, total)
                })
                .unwrap_or_default();
            (flags, commands, source, surface_command_groups)
        };
        for entry in &mut surface_command_groups {
            flags |= extract_redraw_flags(&mut entry.commands);
        }
        let (commands, plans) = extract_scroll_plans(commands);
        if flags.contains(DirtyFlags::ALL) {
            self.grid.resize(self.state.cols, self.state.rows);
            self.grid.invalidate_all();
        }
        self.dirty |= flags;
        // Suppress commands to Kakoune until initialization is complete.
        // Data sent before m_on_key is set may be misinterpreted as raw key input.
        if self.initial_resize_sent {
            self.enqueue_scroll_plans(plans);
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

    fn enqueue_scroll_plans(&mut self, plans: Vec<ScrollPlan>) {
        if !self.initial_resize_sent {
            return;
        }
        for plan in plans {
            self.scroll_runtime.enqueue(plan);
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
        let (normal, deferred) = extract_deferred_commands(commands);
        if matches!(
            execute_commands(
                normal,
                self.session_manager
                    .active_writer_mut()
                    .expect("missing active session writer"),
                &mut || { self.backend.as_mut().and_then(|b| b.clipboard_get()) },
            ),
            CommandResult::Quit
        ) {
            return true;
        }
        self.with_deferred_context(|ctx| handle_deferred_commands(deferred, ctx, source_plugin))
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
        let proxy = self.timer_scheduler.0.clone();
        let spawn_session = self.session_spawner;
        let mut session_runtime = GuiSessionRuntime {
            session_manager: &mut self.session_manager,
            session_states: &mut self.session_states,
            proxy,
            spawn_session,
        };
        let mut ctx = DeferredContext {
            state: &mut self.state,
            registry: &mut self.registry,
            surface_registry: &mut self.surface_registry,
            clipboard_get: &mut || self.backend.as_mut().and_then(|b| b.clipboard_get()),
            dirty: &mut self.dirty,
            timer: &self.timer_scheduler,
            session_host: &mut session_runtime,
            initial_resize_sent: &mut self.initial_resize_sent,
            process_dispatcher: &mut *self.process_dispatcher,
        };
        f(&mut ctx)
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
    }

    fn render_frame(&mut self) {
        if self.gpu.is_none() || self.color_resolver.is_none() {
            tracing::warn!("[app] render_frame skipped: missing gpu/resolver");
            return;
        }
        tracing::debug!(
            "[app] render_frame start ({}x{})",
            self.state.cols,
            self.state.rows
        );

        let Some(ref mut sr) = self.scene_renderer else {
            return;
        };

        let cell_size = sr.cell_size();

        // Only run the pipeline when there are actual dirty flags.
        // Cursor-only animation reuses the cached scene commands.
        if !self.dirty.is_empty() {
            self.surface_registry.sync_ephemeral_surfaces(&self.state);
            self.registry.prepare_plugin_cache(self.dirty);

            // Sync Salsa inputs from updated state
            sync_inputs_from_state(&mut self.salsa_db, &self.state, &self.salsa_handles);
            let _epoch_changed =
                sync_plugin_epoch(&mut self.salsa_db, &self.registry, &self.salsa_handles);
            sync_display_directives(
                &mut self.salsa_db,
                &self.state,
                &self.registry,
                &self.salsa_handles,
            );
            sync_plugin_contributions(
                &mut self.salsa_db,
                &self.state,
                &self.registry,
                &self.salsa_handles,
            );

            let (commands, result) = scene_render_pipeline_cached(
                &self.salsa_db,
                &self.salsa_handles,
                &self.state,
                &self.registry,
                cell_size,
                self.dirty,
                &mut self.scene_cache,
            );
            self.last_render_result = Some(result);

            let gpu = self.gpu.as_ref().unwrap();
            let resolver = self.color_resolver.as_ref().unwrap();
            let (cw, ch) = (sr.metrics().cell_width, sr.metrics().cell_height);
            submit_render(
                sr,
                gpu,
                resolver,
                commands,
                &mut self.cursor_animation,
                result,
                cw,
                ch,
                "scene render",
            );

            // Rebuild HitMap from cached view tree for plugin mouse routing
            kasane_core::event_loop::rebuild_hit_map(
                &self.state,
                &mut self.registry,
                &self.surface_registry,
            );
        } else if let Some(result) = self.last_render_result {
            // Cursor-only frame: reuse cached scene commands
            let gpu = self.gpu.as_ref().unwrap();
            let resolver = self.color_resolver.as_ref().unwrap();
            let commands = self.scene_cache.composed_ref();
            let (cw, ch) = (sr.metrics().cell_width, sr.metrics().cell_height);
            submit_render(
                sr,
                gpu,
                resolver,
                commands,
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
    cursor_animation.update_target(result.cursor_x, result.cursor_y);
    let cursor_state = cursor_animation.tick(cell_width, cell_height);
    tracing::debug!("[app] {label}: {} commands", commands.len());
    match sr.render_with_cursor(gpu, commands, resolver, result.cursor_style, &cursor_state) {
        Ok(()) => tracing::debug!("[app] render_frame complete ({label})"),
        Err(e) => tracing::error!("[app] scene render failed: {e}"),
    }
}

impl<R, W, C> Drop for App<R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: Write + Send + 'static,
    C: Send + 'static,
{
    fn drop(&mut self) {
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
                if !self.dirty.is_empty() || self.cursor_dirty {
                    self.render_frame();
                    self.dirty = DirtyFlags::empty();
                    self.cursor_dirty = false;
                }
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
        self.sync_scroll_runtime();

        // Host-owned smooth scroll runtime tick
        if let Some(resolved) = self.scroll_runtime.tick() {
            kasane_core::plugin::execute_commands(
                vec![Command::SendToKakoune(resolved.to_kakoune_request())],
                self.session_manager
                    .active_writer_mut()
                    .expect("missing active session writer"),
                &mut || None, // GUI doesn't have clipboard_get in this context
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
    }
}
