use std::io::Write;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use kasane_core::config::Config;
use kasane_core::input::InputEvent;
use kasane_core::plugin::{Command, CommandResult, PluginRegistry, execute_commands};
use kasane_core::protocol::KasaneRequest;
use kasane_core::render::{CellGrid, RenderBackend, render_pipeline};
use kasane_core::state::{AppState, DirtyFlags, Msg, tick_scroll_animation, update};

use crate::GuiEvent;
use crate::backend::GuiBackend;
use crate::colors::ColorResolver;
use crate::gpu::GpuState;
use crate::gpu::cell_renderer::CellRenderer;
use crate::input::{apply_modifiers, convert_window_event};

pub struct App<W: Write + Send + 'static> {
    // winit
    window: Option<Arc<Window>>,

    // GPU
    gpu: Option<GpuState>,
    cell_renderer: Option<CellRenderer>,

    // kasane-core
    state: AppState,
    registry: PluginRegistry,
    grid: CellGrid,
    backend: Option<GuiBackend>,

    // Kakoune communication
    kak_writer: W,

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
}

impl<W: Write + Send + 'static> App<W> {
    pub fn new(config: Config, kak_writer: W) -> Self {
        let scroll_amount = config.scroll.lines_per_scroll;

        App {
            window: None,
            gpu: None,
            cell_renderer: None,
            state: AppState::default(),
            registry: PluginRegistry::new(),
            grid: CellGrid::new(80, 24),
            backend: None,
            kak_writer,
            pending_events: Vec::new(),
            dirty: DirtyFlags::ALL,
            initial_resize_sent: false,
            current_modifiers: winit::keyboard::ModifiersState::empty(),
            cursor_pos: None,
            mouse_button_held: None,
            scroll_amount,
            config,
            color_resolver: None,
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
                let cr = CellRenderer::new(&gpu, &self.config.font, scale_factor, phys_size);
                let metrics = cr.metrics().clone();

                // Setup color resolver
                let color_resolver = ColorResolver::from_config(&self.config.colors);

                // Setup state with measured dimensions
                self.state.cols = metrics.cols;
                self.state.rows = metrics.rows;
                self.state.apply_config(&self.config);

                // Setup grid and backend
                self.grid = CellGrid::new(metrics.cols, metrics.rows);
                let gui_backend = GuiBackend::new(metrics);
                self.backend = Some(gui_backend);

                self.color_resolver = Some(color_resolver);
                self.cell_renderer = Some(cr);
                self.gpu = Some(gpu);
            }
            Err(e) => {
                tracing::error!("GPU initialization failed: {e}");
                // TODO: CPU fallback (Phase G3)
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
                GuiEvent::Kakoune(req) => {
                    if !self.initial_resize_sent {
                        self.initial_resize_sent = true;
                        let resize = KasaneRequest::Resize {
                            rows: self.state.rows.saturating_sub(1),
                            cols: self.state.cols,
                        };
                        let _ = writeln!(self.kak_writer, "{}", resize.to_json());
                        let _ = self.kak_writer.flush();
                    }
                    let (flags, commands) = update(
                        &mut self.state,
                        Msg::Kakoune(req),
                        &mut self.registry,
                        &mut self.grid,
                        self.scroll_amount,
                    );
                    self.dirty |= flags;
                    if self.exec_commands(commands) {
                        event_loop.exit();
                        return;
                    }
                }
                GuiEvent::KakouneDied => {
                    event_loop.exit();
                    return;
                }
            }
        }
    }

    fn handle_input_event(&mut self, input: InputEvent, event_loop: &ActiveEventLoop) {
        let msg = Msg::from(input);
        let (flags, commands) = update(
            &mut self.state,
            msg,
            &mut self.registry,
            &mut self.grid,
            self.scroll_amount,
        );
        self.dirty |= flags;
        // Suppress commands to Kakoune until initialization is complete.
        // Data sent before m_on_key is set may be misinterpreted as raw key input.
        if self.initial_resize_sent && self.exec_commands(commands) {
            event_loop.exit();
        }
    }

    /// Execute side-effect commands. Returns `true` if Quit was requested.
    fn exec_commands(&mut self, commands: Vec<Command>) -> bool {
        matches!(
            execute_commands(commands, &mut self.kak_writer, &mut || {
                self.backend.as_mut().and_then(|b| b.clipboard_get())
            }),
            CommandResult::Quit
        )
    }

    fn handle_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if let Some(ref mut gpu) = self.gpu {
            gpu.resize(size.width, size.height);
        }
        if let (Some(cr), Some(gpu)) = (&mut self.cell_renderer, &self.gpu) {
            let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
            cr.resize(gpu, &self.config.font, scale, size);
            let metrics = cr.metrics().clone();
            self.state.cols = metrics.cols;
            self.state.rows = metrics.rows;
            self.grid.resize(metrics.cols, metrics.rows);
            if let Some(ref mut backend) = self.backend {
                backend.update_metrics(metrics.clone());
            }
            // Send resize to Kakoune
            if self.initial_resize_sent {
                let resize = KasaneRequest::Resize {
                    rows: metrics.rows.saturating_sub(1),
                    cols: metrics.cols,
                };
                let _ = writeln!(self.kak_writer, "{}", resize.to_json());
                let _ = self.kak_writer.flush();
            }
            self.dirty = DirtyFlags::ALL;
        }
    }

    fn render_frame(&mut self) {
        if self.gpu.is_none() || self.cell_renderer.is_none() || self.color_resolver.is_none() {
            tracing::warn!("[app] render_frame skipped: missing gpu/renderer/resolver");
            return;
        }
        tracing::debug!(
            "[app] render_frame start ({}x{})",
            self.state.cols,
            self.state.rows
        );

        let result = render_pipeline(&self.state, &self.registry, &mut self.grid);

        // GPU render
        let gpu = self.gpu.as_ref().unwrap();
        let cr = self.cell_renderer.as_mut().unwrap();
        let resolver = self.color_resolver.as_ref().unwrap();

        tracing::debug!("[app] submitting to GPU");
        match cr.render(
            gpu,
            &self.grid,
            resolver,
            Some((result.cursor_x, result.cursor_y, result.cursor_style)),
        ) {
            Ok(()) => tracing::debug!("[app] render_frame complete"),
            Err(e) => tracing::error!("[app] render failed: {e}"),
        }

        self.grid.swap();
    }
}

impl<W: Write + Send + 'static> ApplicationHandler<GuiEvent> for App<W> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracing::info!("[app] resumed, window exists: {}", self.window.is_some());
        if self.window.is_none() {
            self.init_window(event_loop);
            tracing::info!(
                "[app] window initialized, gpu: {}, renderer: {}",
                self.gpu.is_some(),
                self.cell_renderer.is_some()
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
                if !self.dirty.is_empty() {
                    self.render_frame();
                    self.dirty = DirtyFlags::empty();
                }
                return;
            }
            _ => {}
        }

        // Convert input events
        let Some(ref cr) = self.cell_renderer else {
            return;
        };
        let metrics = cr.metrics();
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
        tracing::trace!(
            "[app] about_to_wait, pending: {}, dirty: {:?}",
            self.pending_events.len(),
            self.dirty
        );
        self.process_pending_events(event_loop);

        // Smooth scroll animation tick
        if tick_scroll_animation(&mut self.state, &mut self.kak_writer)
            && let Some(ref window) = self.window
        {
            window.request_redraw();
        }

        if !self.dirty.is_empty()
            && let Some(ref window) = self.window
        {
            window.request_redraw();
        }
    }
}
