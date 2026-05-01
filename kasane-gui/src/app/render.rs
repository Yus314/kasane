//! Frame rendering methods for [`App`].

use std::borrow::Cow;

use kasane_core::render::{RenderResult, SceneRenderOptions, scene_render_pipeline_cached};

use crate::animation::CursorAnimation;
use crate::animation::track::{EasingFn, TrackId};
use crate::colors::ColorResolver;
use crate::gpu::GpuState;
use crate::gpu::scene_renderer::SceneRenderer;
use kasane_core::surface::pane_map::PaneStates;

use crate::diagnostics_overlay::build_diagnostic_overlay_commands;
use crate::ime::{build_ime_overlay_commands, sync_ime_cursor_area as sync_window_ime_cursor_area};

use super::App;

impl<R, W, C> App<R, W, C>
where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    pub(super) fn render_frame(&mut self) {
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
            if let Some(surface) = gpu.surface.as_ref() {
                surface.configure(&gpu.device, &gpu.config);
            }
        }
        let gpu = self.gpu.as_ref().unwrap();
        let Some(_) = self.color_resolver.as_ref() else {
            tracing::warn!("[app] render_frame skipped: missing gpu/resolver");
            return;
        };
        self.color_resolver
            .as_mut()
            .expect("resolver checked above")
            .sync_defaults(&self.state.observed.default_style.to_face());
        tracing::debug!(
            "[app] render_frame start ({}x{})",
            self.state.runtime.cols,
            self.state.runtime.rows
        );
        let ime_overlay_face = if self.state.is_prompt_mode() {
            self.state.observed.status_default_style.to_face()
        } else {
            self.state.observed.default_style.to_face()
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
                w: self.state.runtime.cols,
                h: self.state.runtime.rows,
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
            self.plugin_manager.run_pre_render_hooks(&mut self.state);
            self.registry.prepare_plugin_cache(self.dirty);

            // Sync Salsa inputs from updated state
            kasane_core::event_loop::sync_salsa_for_render(
                &mut self.salsa_db,
                &self.state,
                &mut self.registry,
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
                    pixel_y_offset: self.scroll_spring.position as f32,
                },
            );
            self.last_render_result = Some(result.clone());
            if let Some(ref window) = self.window {
                sync_window_ime_cursor_area(window, &self.ime, &result, sr.metrics());
            }
            self.state.runtime.display_scroll_offset = result.display_scroll_offset;
            self.state.runtime.display_map = Some(display_map);
            self.state.runtime.display_unit_map = self
                .state
                .runtime
                .display_map
                .as_ref()
                .filter(|dm| !dm.is_identity())
                .map(|dm| kasane_core::display::DisplayUnitMap::build(dm));
            let overlay_commands = build_diagnostic_overlay_commands(
                &self.diagnostic_overlay,
                cell_size,
                self.state.runtime.cols,
                self.state.runtime.rows,
            );
            let ime_overlay_commands =
                build_ime_overlay_commands(&self.ime, &result, cell_size, ime_overlay_face);
            let mut overlay_commands = overlay_commands;
            overlay_commands.extend(ime_overlay_commands);
            let frame_commands = append_overlay_commands(commands, overlay_commands);

            let (cw, ch) = (sr.metrics().cell_width, sr.metrics().cell_height);
            let resolver = self
                .color_resolver
                .as_ref()
                .expect("resolver checked above");

            // Drive overlay fade transitions
            let overlay_count = frame_commands
                .iter()
                .filter(|c| matches!(c, kasane_core::render::DrawCommand::BeginOverlay))
                .count();
            let overlay_opacities = compute_overlay_opacities(
                &mut self.cursor_animation,
                overlay_count,
                &mut self.prev_overlay_count,
                self.config.effects.overlay_transition_ms,
            );

            submit_render(
                sr,
                gpu,
                resolver,
                &frame_commands,
                &mut self.cursor_animation,
                &result,
                cw,
                ch,
                &overlay_opacities,
                "scene render",
            );

            // Rebuild HitMap from cached view tree for plugin mouse routing
            kasane_core::event_loop::rebuild_hit_map(
                &mut self.state,
                &self.registry,
                &self.surface_registry,
            );
        } else if let Some(result) = self.last_render_result.clone() {
            // Cursor-only frame: reuse cached scene commands
            let _cursor_span = tracing::info_span!("cursor_only_frame").entered();
            let commands = self.scene_cache.composed_ref();
            if let Some(ref window) = self.window {
                sync_window_ime_cursor_area(window, &self.ime, &result, sr.metrics());
            }
            let overlay_commands = build_diagnostic_overlay_commands(
                &self.diagnostic_overlay,
                cell_size,
                self.state.runtime.cols,
                self.state.runtime.rows,
            );
            let ime_overlay_commands =
                build_ime_overlay_commands(&self.ime, &result, cell_size, ime_overlay_face);
            let mut overlay_commands = overlay_commands;
            overlay_commands.extend(ime_overlay_commands);
            let frame_commands = append_overlay_commands(commands, overlay_commands);
            let (cw, ch) = (sr.metrics().cell_width, sr.metrics().cell_height);
            let resolver = self
                .color_resolver
                .as_ref()
                .expect("resolver checked above");

            let overlay_count = frame_commands
                .iter()
                .filter(|c| matches!(c, kasane_core::render::DrawCommand::BeginOverlay))
                .count();
            let overlay_opacities = compute_overlay_opacities(
                &mut self.cursor_animation,
                overlay_count,
                &mut self.prev_overlay_count,
                self.config.effects.overlay_transition_ms,
            );

            submit_render(
                sr,
                gpu,
                resolver,
                &frame_commands,
                &mut self.cursor_animation,
                &result,
                cw,
                ch,
                &overlay_opacities,
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
    result: &RenderResult,
    cell_width: f32,
    cell_height: f32,
    overlay_opacities: &[f32],
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
        overlay_opacities,
        &result.visual_hints,
    ) {
        Ok(()) => tracing::debug!("[app] render_frame complete ({label})"),
        Err(e) => tracing::error!("[app] scene render failed: {e}"),
    }
}

/// Compute per-overlay-layer opacities by driving animation engine tracks.
///
/// Detects overlay appearance/disappearance and drives fade-in/fade-out
/// via the cursor animation engine's MENU_OPACITY and INFO_OPACITY tracks.
fn compute_overlay_opacities(
    cursor_animation: &mut CursorAnimation,
    overlay_count: usize,
    prev_overlay_count: &mut usize,
    transition_ms: u16,
) -> Vec<f32> {
    if transition_ms == 0 {
        *prev_overlay_count = overlay_count;
        return vec![1.0; overlay_count];
    }

    let duration = transition_ms as f32 / 1000.0;
    let engine = cursor_animation.engine_mut();

    // Ensure tracks are registered unconditionally.
    // register() overwrites only if the track doesn't exist yet.
    let tracks = [TrackId::MENU_OPACITY, TrackId::INFO_OPACITY];
    for &track in &tracks {
        if !engine.has_track(track) {
            engine.register(track, 0.0, duration, EasingFn::EaseOut);
        }
    }

    // Drive transitions based on overlay count changes.
    //
    // Key insight: overlay_count changes like 1→3→1→3 (menu stays, info
    // appears/disappears). We must snap tracks to 0 when their layer
    // disappears, not only when ALL overlays disappear.
    if overlay_count > *prev_overlay_count {
        // New overlays appeared — fade in each new layer
        if *prev_overlay_count == 0 {
            engine.snap(TrackId::MENU_OPACITY, 0.0);
            engine.set_duration(TrackId::MENU_OPACITY, duration);
            engine.set_target(TrackId::MENU_OPACITY, 1.0);
        }
        if overlay_count > 1 && *prev_overlay_count <= 1 {
            engine.snap(TrackId::INFO_OPACITY, 0.0);
            engine.set_duration(TrackId::INFO_OPACITY, duration);
            engine.set_target(TrackId::INFO_OPACITY, 1.0);
        }
    } else if overlay_count < *prev_overlay_count {
        if overlay_count == 0 {
            // All overlays gone
            engine.snap(TrackId::MENU_OPACITY, 0.0);
            engine.snap(TrackId::INFO_OPACITY, 0.0);
        } else if overlay_count <= 1 && *prev_overlay_count > 1 {
            // Info layers disappeared but menu remains
            engine.snap(TrackId::INFO_OPACITY, 0.0);
        }
    }

    *prev_overlay_count = overlay_count;

    // Collect opacities for each overlay layer
    let mut opacities = Vec::with_capacity(overlay_count);
    for i in 0..overlay_count {
        let track = if i == 0 {
            TrackId::MENU_OPACITY
        } else {
            TrackId::INFO_OPACITY
        };
        opacities.push(engine.value(track).max(0.01));
    }
    opacities
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
