//! Winit application loop: handles window events, GPU frame rendering, and input.

mod event_handler;
mod render;

use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use winit::dpi::LogicalSize;
use winit::event::Ime;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowAttributes};

use crate::animation::CursorAnimation;
use crate::animation::SpringPhysics;
use crate::backend::GuiBackend;
use crate::colors::ColorResolver;
use crate::gpu::GpuState;
use crate::gpu::scene_renderer::SceneRenderer;
use crate::ime::{GuiImeState, sync_ime_cursor_area as sync_window_ime_cursor_area};
use crate::{GuiEvent, GuiEventSink};
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
    AppView, Command, HttpDispatcher, HttpEvent, IoEvent, PluginDiagnosticOverlayState,
    PluginManager, PluginRuntime, ProcessDispatcher, ProcessEvent, extract_redraw_flags,
    report_plugin_diagnostics,
};
use kasane_core::protocol::KasaneRequest;
use kasane_core::render::{RenderResult, SceneCache};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::SalsaInputHandles;
use kasane_core::scroll::ScrollRuntime;
use kasane_core::session::{SessionManager, SessionSpec, SessionStateStore};
use kasane_core::state::{AppState, DirtyFlags, Msg, UpdateResult, update};
use kasane_core::surface::SurfaceRegistry;

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
    /// Spring physics for sub-pixel smooth scroll offset.
    scroll_spring: SpringPhysics,
    scroll_spring_last_tick: std::time::Instant,

    // Scene cache
    scene_cache: SceneCache,

    // Cursor animation
    cursor_animation: CursorAnimation,
    cursor_dirty: bool,
    last_render_result: Option<RenderResult>,
    // Overlay fade state
    prev_overlay_count: usize,
    ime: GuiImeState,
    diagnostic_overlay: PluginDiagnosticOverlayState,

    // Event loop proxy for scheduling
    event_proxy: winit::event_loop::EventLoopProxy<GuiEvent>,

    // Event sink for generic schedulers
    gui_sink: GuiEventSink,

    // Process dispatcher for plugin process execution
    process_dispatcher: Box<dyn ProcessDispatcher>,
    // HTTP dispatcher for plugin HTTP requests
    http_dispatcher: Box<dyn HttpDispatcher>,

    // Plugin manager (owned for hot-reload)
    plugin_manager: PluginManager,

    // Hot-reload state
    widget_names: Vec<String>,
    last_config_hash: u64,

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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Config,
        mut session_manager: SessionManager<R, W, C>,
        session_spawner: fn(&SessionSpec) -> anyhow::Result<(R, W, C)>,
        event_proxy: winit::event_loop::EventLoopProxy<GuiEvent>,
        mut plugin_manager: PluginManager,
        registry: PluginRuntime,
        process_dispatcher: Box<dyn ProcessDispatcher>,
        http_dispatcher: Box<dyn HttpDispatcher>,
    ) -> Result<(Self, Vec<std::path::PathBuf>)> {
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

        // Load widgets from unified kasane.kdl (each widget becomes its own plugin)
        let mut widget_names: Vec<String> = Vec::new();
        let mut widget_included_paths: Vec<std::path::PathBuf> = Vec::new();
        let mut last_config_hash: u64 = 0;
        {
            let config_path = kasane_core::config::config_path();
            if let Ok(source) = std::fs::read_to_string(&config_path) {
                {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    source.hash(&mut hasher);
                    last_config_hash = hasher.finish();
                }
                match kasane_core::config::unified::parse_unified(&source) {
                    Ok((_config, config_errors, widget_file, errors)) => {
                        for err in &config_errors {
                            tracing::warn!("config: {err}");
                        }
                        for err in &errors {
                            tracing::warn!("widget `{}`: {}", err.name, err.message);
                        }
                        widget_included_paths = widget_file.included_paths.clone();
                        widget_names = kasane_core::widget::register_all_widgets(
                            widget_file,
                            &errors,
                            &mut registry,
                        );
                    }
                    Err(e) => {
                        tracing::warn!("kasane.kdl widget parse failed: {e}");
                    }
                }
            }
        }

        // Collect plugin-owned surfaces before plugin init so invalid surface
        // contracts do not get a chance to produce side effects.
        let initial_plugins = plugin_manager.initialize(&mut registry, |_, registry| {
            kasane_core::event_loop::setup_plugin_surfaces(registry, &mut surface_registry, &state)
        })?;
        initial_plugins.apply_settings(&mut state);
        kasane_core::event_loop::sync_suppressed_builtins(&mut state, &registry);
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

        Ok((
            App {
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
                scroll_spring: SpringPhysics::critically_damped(300.0),
                scroll_spring_last_tick: std::time::Instant::now(),
                config,
                color_resolver: None,
                scene_cache: SceneCache::new(),
                cursor_animation: CursorAnimation::new(),
                cursor_dirty: false,
                last_render_result: None,
                prev_overlay_count: 0,
                ime: GuiImeState::default(),
                diagnostic_overlay,
                event_proxy: event_proxy.clone(),
                gui_sink,
                process_dispatcher,
                http_dispatcher,
                plugin_manager,
                widget_names,
                last_config_hash,
                salsa_db,
                salsa_handles,
            },
            widget_included_paths,
        ))
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
                let mut sr = SceneRenderer::new(&gpu, &self.config.font, scale_factor, phys_size);
                sr.set_event_proxy(self.event_proxy.clone());
                sr.set_effects(self.config.effects.clone());
                let metrics = sr.metrics().clone();

                // Setup color resolver
                let color_resolver = ColorResolver::from_config(&self.config.colors);

                // Setup state with measured dimensions
                self.state.runtime.cols = metrics.cols;
                self.state.runtime.rows = metrics.rows;
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
                event_handler::print_gpu_troubleshooting();
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
                                w: self.state.runtime.cols,
                                h: self.state.runtime.rows,
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
                        self.state.runtime.rows,
                        self.state.runtime.cols,
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
                            self.state.runtime.rows,
                            self.state.runtime.cols,
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
                    // Free per-plugin count slot when a job finishes
                    match &io_event {
                        IoEvent::Process(pe) => {
                            let finished_job = match pe {
                                ProcessEvent::Exited { job_id, .. }
                                | ProcessEvent::SpawnFailed { job_id, .. } => Some(*job_id),
                                _ => None,
                            };
                            if let Some(job_id) = finished_job {
                                self.process_dispatcher
                                    .remove_finished_job(&plugin_id, job_id);
                            }
                        }
                        IoEvent::Http(he) => {
                            let finished_job = match he {
                                HttpEvent::Response { job_id, .. }
                                | HttpEvent::StreamEnd { job_id }
                                | HttpEvent::Error { job_id, .. } => Some(*job_id),
                                HttpEvent::Chunk { .. } => None,
                            };
                            if let Some(job_id) = finished_job {
                                self.http_dispatcher.cancel(&plugin_id, job_id);
                            }
                        }
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
                GuiEvent::FileReload => {
                    self.handle_file_reload();
                }
                GuiEvent::PluginReload => {
                    if self.handle_plugin_reload() {
                        event_loop.exit();
                        return;
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

    fn handle_file_reload(&mut self) {
        use kasane_core::event_loop::schedule_diagnostic_overlay;
        use kasane_core::plugin::PluginDiagnostic;

        let config_path = kasane_core::config::config_path();
        match std::fs::read_to_string(&config_path) {
            Ok(source) => {
                let hash = {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    source.hash(&mut hasher);
                    hasher.finish()
                };
                if hash == self.last_config_hash {
                    return;
                }
                match kasane_core::config::unified::parse_unified(&source) {
                    Ok((new_config, config_errors, widget_file, widget_errors)) => {
                        for err in &config_errors {
                            tracing::warn!("config: {err}");
                        }
                        if !config_errors.is_empty() {
                            let diagnostics: Vec<PluginDiagnostic> = config_errors
                                .iter()
                                .map(|e| PluginDiagnostic {
                                    target: kasane_core::plugin::PluginDiagnosticTarget::Plugin(
                                        kasane_core::plugin::PluginId("kasane.config".to_string()),
                                    ),
                                    kind: kasane_core::plugin::PluginDiagnosticKind::RuntimeError {
                                        method: "parse".to_string(),
                                    },
                                    message: e.to_string(),
                                    previous: None,
                                    attempted: None,
                                })
                                .collect();
                            schedule_diagnostic_overlay(
                                &kasane_core::event_loop::GenericDiagnosticScheduler(
                                    self.gui_sink.clone(),
                                ),
                                &mut self.diagnostic_overlay,
                                &diagnostics,
                            );
                        }

                        // Check for restart-required fields, excluding GUI-handleable ones
                        let restart_fields: Vec<&str> = self
                            .config
                            .restart_required_diff(&new_config)
                            .into_iter()
                            .filter(|f| {
                                !matches!(*f, "font" | "window" | "scroll.lines_per_scroll")
                            })
                            .collect();
                        if !restart_fields.is_empty() {
                            let field_list = restart_fields.join(", ");
                            tracing::warn!("restart required for: {field_list}");
                            let diagnostic = PluginDiagnostic {
                                target: kasane_core::plugin::PluginDiagnosticTarget::Plugin(
                                    kasane_core::plugin::PluginId("kasane.config".to_string()),
                                ),
                                kind: kasane_core::plugin::PluginDiagnosticKind::RuntimeError {
                                    method: "reload".to_string(),
                                },
                                message: format!(
                                    "restart required for: {field_list}. \
                                     Exit and re-run kasane to apply"
                                ),
                                previous: None,
                                attempted: None,
                            };
                            schedule_diagnostic_overlay(
                                &kasane_core::event_loop::GenericDiagnosticScheduler(
                                    self.gui_sink.clone(),
                                ),
                                &mut self.diagnostic_overlay,
                                &[diagnostic],
                            );
                        }

                        // Apply config to state
                        self.state.apply_config(&new_config);

                        // Hot-reload per-widget plugins (diff-based)
                        self.widget_names = kasane_core::widget::hot_reload_widgets(
                            &self.widget_names,
                            widget_file,
                            &widget_errors,
                            &mut self.registry,
                        );

                        // Route widget parse errors to diagnostic overlay
                        if !widget_errors.is_empty() {
                            let diagnostics: Vec<PluginDiagnostic> = widget_errors
                                .iter()
                                .map(kasane_core::widget::node_error_to_diagnostic)
                                .collect();
                            for err in &widget_errors {
                                tracing::warn!("widget `{}`: {}", err.name, err.message);
                            }
                            schedule_diagnostic_overlay(
                                &kasane_core::event_loop::GenericDiagnosticScheduler(
                                    self.gui_sink.clone(),
                                ),
                                &mut self.diagnostic_overlay,
                                &diagnostics,
                            );
                        }

                        // GUI-specific: font reload
                        if self.scene_renderer.is_some() && self.config.font != new_config.font {
                            self.config = new_config.clone();
                            if let Some(ref window) = self.window {
                                self.handle_resize(window.inner_size());
                            }
                        }

                        // GUI-specific: color palette reload
                        if self.scene_renderer.is_some() && self.config.colors != new_config.colors
                        {
                            self.color_resolver =
                                Some(ColorResolver::from_config(&new_config.colors));
                        }

                        // GUI-specific: scroll amount
                        self.scroll_amount = new_config.scroll.lines_per_scroll;

                        self.config = new_config;
                        self.last_config_hash = hash;
                        self.dirty |= DirtyFlags::ALL;
                        tracing::info!("kasane.kdl hot-reloaded");
                    }
                    Err(err) => {
                        tracing::warn!("kasane.kdl reload failed (keeping previous): {err}");
                        let diagnostic = PluginDiagnostic {
                            target: kasane_core::plugin::PluginDiagnosticTarget::Plugin(
                                kasane_core::plugin::PluginId("kasane.widget.reload".to_string()),
                            ),
                            kind: kasane_core::plugin::PluginDiagnosticKind::RuntimeError {
                                method: "reload".to_string(),
                            },
                            message: format!("kasane.kdl reload failed (keeping previous): {err}"),
                            previous: None,
                            attempted: None,
                        };
                        schedule_diagnostic_overlay(
                            &kasane_core::event_loop::GenericDiagnosticScheduler(
                                self.gui_sink.clone(),
                            ),
                            &mut self.diagnostic_overlay,
                            &[diagnostic],
                        );
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File deleted: unload all widget plugins, reset config to defaults
                self.state
                    .apply_config(&kasane_core::config::Config::default());
                for name in self.widget_names.drain(..) {
                    let id = kasane_core::widget::WidgetPlugin::plugin_id_for(&name);
                    self.registry.remove_plugin(&id);
                }
                self.dirty |= DirtyFlags::ALL;
            }
            Err(e) => {
                tracing::warn!("cannot read {}: {e}", config_path.display());
            }
        }
    }

    /// Hot-reload WASM plugins. Returns `true` if the app should quit.
    fn handle_plugin_reload(&mut self) -> bool {
        use kasane_core::event_loop::{
            apply_bootstrap_effects, notify_workspace_observers, reconcile_plugin_surfaces,
            schedule_diagnostic_overlay,
        };

        let reload_result = self.plugin_manager.reload(
            &mut self.registry,
            &AppView::new(&self.state),
            |result, registry| {
                if result.deltas.is_empty() {
                    return vec![];
                }
                reconcile_plugin_surfaces(
                    registry,
                    &mut self.surface_registry,
                    &self.state,
                    result.deltas.as_slice(),
                )
            },
        );
        match reload_result {
            Ok(reload) => {
                reload.apply_settings(&mut self.state);
                kasane_core::event_loop::sync_suppressed_builtins(&mut self.state, &self.registry);
                report_plugin_diagnostics(&reload.diagnostics);
                schedule_diagnostic_overlay(
                    &kasane_core::event_loop::GenericDiagnosticScheduler(self.gui_sink.clone()),
                    &mut self.diagnostic_overlay,
                    &reload.diagnostics,
                );
                let ready_targets: Vec<_> = reload.ready_targets().cloned().collect();
                let mut flags = DirtyFlags::all();
                apply_bootstrap_effects(reload.bootstrap, &mut flags);
                sync_ready_gate(&mut self.session_ready_gate, &self.state);
                if !reload.deltas.is_empty() {
                    notify_workspace_observers(
                        &mut self.registry,
                        &self.surface_registry,
                        &self.state,
                    );
                }
                self.dirty |= flags;
                // Flush ready targets for reloaded plugins
                if self.initial_resize_sent {
                    for plugin_id in &ready_targets {
                        let batch = self.registry.notify_plugin_active_session_ready_batch(
                            plugin_id,
                            &AppView::new(&self.state),
                        );
                        if self.apply_runtime_batch(batch, Some(plugin_id)) {
                            return true;
                        }
                    }
                }
                tracing::info!("hot-reloaded plugins");
            }
            Err(err) => {
                tracing::error!("failed to hot-reload plugins: {err}");
            }
        }
        false
    }

    fn handle_input_event(&mut self, input: InputEvent, event_loop: &ActiveEventLoop) {
        let input = normalize_input_for_state(input, &self.state);
        let total = Rect {
            x: 0,
            y: 0,
            w: self.state.runtime.cols,
            h: self.state.runtime.rows,
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
        let timer = kasane_core::event_loop::GenericTimerScheduler::new(self.gui_sink.clone());
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
                http_dispatcher: &mut *self.http_dispatcher,
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

        self.state.runtime.cols = metrics.cols;
        self.state.runtime.rows = metrics.rows;
        if let Some(ref mut backend) = self.backend {
            backend.update_metrics(metrics);
        }
        // Send resize to Kakoune
        if self.initial_resize_sent {
            let resize = KasaneRequest::Resize {
                rows: self.state.available_height(),
                cols: self.state.runtime.cols,
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
            self.last_render_result.as_ref(),
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
}
