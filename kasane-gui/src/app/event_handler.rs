//! Winit `ApplicationHandler` and `Drop` implementations for [`App`].

use std::io::Write;

use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

use kasane_core::plugin::Command;
use kasane_core::state::DirtyFlags;

use crate::GuiEvent;
use crate::input::{apply_modifiers, convert_window_event};

use super::App;

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
        self.process_dispatcher.shutdown();
        self.http_dispatcher.shutdown();
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
        let hit_test = |px: f64, py: f64| sr.hit_test(px, py);
        let mut input_events = convert_window_event(
            &event,
            metrics,
            &mut self.cursor_pos,
            &mut self.mouse_button_held,
            Some(&hit_test),
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

    /// Wake-up handler. Only triggers a redraw when the wake came from the
    /// `WaitUntil` deadline (`ResumeTimeReached`) and an animation is still
    /// active; previously the redraw was requested unconditionally inside
    /// `about_to_wait`, which queued a `RedrawRequested` event that winit
    /// dispatched immediately, bypassing the deadline and producing a tight
    /// render loop (~360-680 fps observed). Routing the wake-up redraw
    /// through `new_events` lets `WaitUntil` actually gate the frame rate
    /// at the configured 60/30 fps cadence.
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if !matches!(cause, StartCause::ResumeTimeReached { .. }) {
            return;
        }
        let needs_animation_frame = self.cursor_animation.is_animating
            || self.cursor_animation.engine().is_animating()
            || !self.scroll_spring.is_at_rest()
            || self.scroll_runtime.active_frame_interval().is_some();
        if needs_animation_frame && let Some(ref window) = self.window {
            window.request_redraw();
            self.cursor_dirty = true;
        }
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

        // Sub-pixel scroll spring tick. Only advance the physics here; the
        // wake-up that drives the next render comes from `new_events` when
        // the WaitUntil deadline fires, so we do NOT call request_redraw
        // from this branch (doing so would queue a RedrawRequested that
        // winit dispatches immediately, defeating WaitUntil and tight-
        // looping at hundreds of fps).
        if !self.scroll_spring.is_at_rest() {
            let now = std::time::Instant::now();
            let dt = now
                .duration_since(self.scroll_spring_last_tick)
                .as_secs_f64();
            self.scroll_spring_last_tick = now;
            self.scroll_spring.tick(dt);
            self.cursor_dirty = true;
        }

        // Cursor/overlay animation: redraw is requested in `new_events` when
        // the WaitUntil deadline fires (see comment there). No work needed
        // here beyond letting the deadline computation below schedule the
        // wake-up.

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
        let spring_deadline = if self.scroll_spring.is_at_rest() {
            None
        } else {
            // 60fps for spring animation
            Some(std::time::Instant::now() + std::time::Duration::from_nanos(16_666_667))
        };
        let cursor_deadline = self.cursor_animation.next_frame_deadline();
        let engine_deadline = self.cursor_animation.engine().next_frame_deadline();
        let deadline = [
            scroll_deadline,
            spring_deadline,
            cursor_deadline,
            engine_deadline,
        ]
        .into_iter()
        .flatten()
        .min();
        match deadline {
            Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

pub(super) fn print_gpu_troubleshooting() {
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
