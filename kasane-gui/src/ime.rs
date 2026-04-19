use kasane_core::input::ResolvedTextInputTarget;
use kasane_core::protocol::{Attributes, Face};
use kasane_core::render::scene::{PixelPos, PixelRect, line_display_width_str};
use kasane_core::render::{CellSize, DrawCommand, RenderResult};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::window::Window;

use crate::gpu::CellMetrics;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct GuiImeState {
    pub policy_allowed: bool,
    pub platform_enabled: bool,
    pub bound_target: Option<ResolvedTextInputTarget>,
    pub preedit: String,
    pub cursor_range: Option<(usize, usize)>,
    pub overlay_dirty: bool,
}

impl GuiImeState {
    pub fn clear_preedit(&mut self) -> bool {
        let changed = !self.preedit.is_empty() || self.cursor_range.is_some();
        self.preedit.clear();
        self.cursor_range = None;
        if changed {
            self.overlay_dirty = true;
        }
        changed
    }

    pub fn set_preedit(&mut self, text: String, range: Option<(usize, usize)>) -> bool {
        let changed = self.preedit != text || self.cursor_range != range;
        self.preedit = text;
        self.cursor_range = range;
        if changed {
            self.overlay_dirty = true;
        }
        changed
    }

    pub fn caret_display_offset_cols(&self) -> usize {
        if self.preedit.is_empty() {
            return 0;
        }

        match clamped_preedit_range(&self.preedit, self.cursor_range) {
            Some((start, _)) => display_cols_until_byte(&self.preedit, start),
            None => line_display_width_str(&self.preedit),
        }
    }

    pub fn bind_target(&mut self, target: Option<ResolvedTextInputTarget>) -> bool {
        if self.bound_target == target {
            return false;
        }

        self.bound_target = target;
        self.clear_preedit();
        true
    }
}

pub(crate) fn display_cols_until_byte(s: &str, byte_idx: usize) -> usize {
    let idx = clamp_to_char_boundary(s, byte_idx);
    line_display_width_str(&s[..idx])
}

fn clamp_to_char_boundary(s: &str, byte_idx: usize) -> usize {
    let mut idx = byte_idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn clamped_preedit_range(s: &str, range: Option<(usize, usize)>) -> Option<(usize, usize)> {
    let (start, end) = range?;
    let start = clamp_to_char_boundary(s, start);
    let end = clamp_to_char_boundary(s, end);
    Some((start.min(end), start.max(end)))
}

fn push_preedit_text(
    commands: &mut Vec<DrawCommand>,
    x: f32,
    y: f32,
    text: &str,
    face: Face,
    cell_size: CellSize,
) {
    if text.is_empty() {
        return;
    }

    commands.push(DrawCommand::DrawText {
        pos: PixelPos { x, y },
        text: text.to_string(),
        face,
        max_width: line_display_width_str(text).max(1) as f32 * cell_size.width,
    });
}

pub(crate) fn build_ime_overlay_commands(
    ime: &GuiImeState,
    render: &RenderResult,
    cell_size: CellSize,
    face: Face,
) -> Vec<DrawCommand> {
    if ime.preedit.is_empty() {
        return vec![];
    }

    let mut base_face = face;
    base_face.attributes |= Attributes::UNDERLINE;
    let mut active_face = base_face;
    active_face.attributes |= Attributes::REVERSE;

    let base_x = render.cursor_x as f32 * cell_size.width;
    let base_y = render.cursor_y as f32 * cell_size.height;
    let mut commands = vec![DrawCommand::BeginOverlay];

    match clamped_preedit_range(&ime.preedit, ime.cursor_range) {
        Some((start, end)) if start != end => {
            let prefix = &ime.preedit[..start];
            let active = &ime.preedit[start..end];
            let suffix = &ime.preedit[end..];

            let prefix_x = base_x;
            let active_x = prefix_x + line_display_width_str(prefix) as f32 * cell_size.width;
            let suffix_x = active_x + line_display_width_str(active) as f32 * cell_size.width;

            push_preedit_text(
                &mut commands,
                prefix_x,
                base_y,
                prefix,
                base_face,
                cell_size,
            );
            push_preedit_text(
                &mut commands,
                active_x,
                base_y,
                active,
                active_face,
                cell_size,
            );
            push_preedit_text(
                &mut commands,
                suffix_x,
                base_y,
                suffix,
                base_face,
                cell_size,
            );
        }
        Some((start, _)) => {
            let prefix = &ime.preedit[..start];
            let suffix = &ime.preedit[start..];
            let caret_x = base_x + line_display_width_str(prefix) as f32 * cell_size.width;
            let caret_width = (cell_size.width * 0.08).max(1.0);
            let mut caret_face = base_face;
            caret_face.attributes |= Attributes::REVERSE;

            push_preedit_text(&mut commands, base_x, base_y, prefix, base_face, cell_size);
            commands.push(DrawCommand::FillRect {
                rect: PixelRect {
                    x: caret_x,
                    y: base_y,
                    w: caret_width,
                    h: cell_size.height,
                },
                face: caret_face,
                elevated: false,
            });
            push_preedit_text(&mut commands, caret_x, base_y, suffix, base_face, cell_size);
        }
        None => {
            push_preedit_text(
                &mut commands,
                base_x,
                base_y,
                &ime.preedit,
                base_face,
                cell_size,
            );
        }
    }

    commands
}

pub(crate) fn sync_ime_cursor_area(
    window: &Window,
    ime: &GuiImeState,
    render: &RenderResult,
    metrics: &CellMetrics,
) {
    if !ime.policy_allowed || !ime.platform_enabled {
        return;
    }

    let caret_x = ((render.cursor_x as usize + ime.caret_display_offset_cols()) as f32
        * metrics.cell_width)
        .round() as i32;
    let caret_y = (render.cursor_y as f32 * metrics.cell_height).round() as i32;
    let size = PhysicalSize::new(
        metrics.cell_width.max(1.0).round() as u32,
        metrics.cell_height.max(1.0).round() as u32,
    );
    window.set_ime_cursor_area(PhysicalPosition::new(caret_x, caret_y), size);
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::input::{
        TextInputTargetAuthority, TextInputTargetKind, resolve_text_input_target,
    };
    use kasane_core::protocol::StatusStyle;
    use kasane_core::protocol::{Attributes, Face};
    use kasane_core::session::SessionId;
    use kasane_core::state::AppState;
    use kasane_core::state::derived::EditorMode;

    #[test]
    fn display_cols_until_byte_clamps_to_char_boundary() {
        assert_eq!(display_cols_until_byte("aあb", 1), 1);
        assert_eq!(display_cols_until_byte("aあb", 2), 1);
        assert_eq!(display_cols_until_byte("aあb", 4), 3);
        assert_eq!(display_cols_until_byte("aあb", 99), 4);
    }

    #[test]
    fn build_ime_overlay_commands_emits_overlay_text() {
        let commands = build_ime_overlay_commands(
            &GuiImeState {
                preedit: "変換中".to_string(),
                ..GuiImeState::default()
            },
            &RenderResult {
                cursor_x: 3,
                cursor_y: 4,
                cursor_style: kasane_core::render::CursorStyle::Block,
                cursor_color: kasane_core::protocol::Color::Default,
                cursor_blink: None,
                cursor_movement: None,
                display_scroll_offset: 0,
                visual_hints: Default::default(),
            },
            CellSize {
                width: 10.0,
                height: 20.0,
            },
            Face::default(),
        );

        assert!(matches!(commands.first(), Some(DrawCommand::BeginOverlay)));
        assert!(commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::DrawText { text, .. } if text == "変換中"
            )
        }));
    }

    #[test]
    fn build_ime_overlay_commands_highlights_active_segment() {
        let commands = build_ime_overlay_commands(
            &GuiImeState {
                preedit: "abc".to_string(),
                cursor_range: Some((1, 2)),
                ..GuiImeState::default()
            },
            &RenderResult {
                cursor_x: 1,
                cursor_y: 2,
                cursor_style: kasane_core::render::CursorStyle::Block,
                cursor_color: kasane_core::protocol::Color::Default,
                cursor_blink: None,
                cursor_movement: None,
                display_scroll_offset: 0,
                visual_hints: Default::default(),
            },
            CellSize {
                width: 10.0,
                height: 20.0,
            },
            Face::default(),
        );

        let text_segments: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText { text, face, .. } => Some((text.as_str(), *face)),
                _ => None,
            })
            .collect();

        assert_eq!(text_segments.len(), 3);
        assert_eq!(text_segments[0].0, "a");
        assert_eq!(text_segments[1].0, "b");
        assert_eq!(text_segments[2].0, "c");
        assert!(
            text_segments[1].1.attributes.contains(Attributes::REVERSE),
            "active preedit span should be visually distinguished"
        );
        assert!(
            text_segments[0]
                .1
                .attributes
                .contains(Attributes::UNDERLINE)
                && text_segments[2]
                    .1
                    .attributes
                    .contains(Attributes::UNDERLINE)
        );
    }

    #[test]
    fn build_ime_overlay_commands_draws_bar_caret_for_collapsed_range() {
        let commands = build_ime_overlay_commands(
            &GuiImeState {
                preedit: "ab".to_string(),
                cursor_range: Some((1, 1)),
                ..GuiImeState::default()
            },
            &RenderResult {
                cursor_x: 0,
                cursor_y: 0,
                cursor_style: kasane_core::render::CursorStyle::Block,
                cursor_color: kasane_core::protocol::Color::Default,
                cursor_blink: None,
                cursor_movement: None,
                display_scroll_offset: 0,
                visual_hints: Default::default(),
            },
            CellSize {
                width: 10.0,
                height: 20.0,
            },
            Face::default(),
        );

        assert!(commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::FillRect {
                    rect,
                    ..
                } if (rect.x - 10.0).abs() < f32::EPSILON && rect.h == 20.0
            )
        }));
    }

    #[test]
    fn build_ime_overlay_commands_clamps_non_boundary_ranges() {
        let commands = build_ime_overlay_commands(
            &GuiImeState {
                preedit: "aあb".to_string(),
                cursor_range: Some((2, 4)),
                ..GuiImeState::default()
            },
            &RenderResult {
                cursor_x: 0,
                cursor_y: 0,
                cursor_style: kasane_core::render::CursorStyle::Block,
                cursor_color: kasane_core::protocol::Color::Default,
                cursor_blink: None,
                cursor_movement: None,
                display_scroll_offset: 0,
                visual_hints: Default::default(),
            },
            CellSize {
                width: 10.0,
                height: 20.0,
            },
            Face::default(),
        );

        assert!(commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::DrawText { text, face, .. }
                    if text == "あ" && face.attributes.contains(Attributes::REVERSE)
            )
        }));
    }

    #[test]
    fn resolve_text_input_target_prefers_observed_prompt_style() {
        let mut state = AppState::default();
        state.observed.status_style = StatusStyle::Command;
        state.observed.status_content_cursor_pos = -1;
        state.inference.editor_mode = EditorMode::Insert;

        assert_eq!(
            resolve_text_input_target(&state, Some(SessionId(7))),
            Some(ResolvedTextInputTarget {
                session_id: Some(SessionId(7)),
                kind: TextInputTargetKind::Prompt(StatusStyle::Command),
                authority: TextInputTargetAuthority::ObservedStatusStyle,
            })
        );
    }

    #[test]
    fn resolve_text_input_target_falls_back_to_prompt_cursor() {
        let mut state = AppState::default();
        state.observed.status_style = StatusStyle::Status;
        state.observed.status_content_cursor_pos = 0;
        state.inference.editor_mode = EditorMode::Normal;

        assert_eq!(
            resolve_text_input_target(&state, Some(SessionId(3))),
            Some(ResolvedTextInputTarget {
                session_id: Some(SessionId(3)),
                kind: TextInputTargetKind::Prompt(StatusStyle::Status),
                authority: TextInputTargetAuthority::ObservedPromptCursor,
            })
        );
    }

    #[test]
    fn resolve_text_input_target_uses_heuristic_buffer_mode_last() {
        let mut state = AppState::default();
        state.observed.status_style = StatusStyle::Status;
        state.observed.status_content_cursor_pos = -1;
        state.inference.editor_mode = EditorMode::Replace;

        assert_eq!(
            resolve_text_input_target(&state, None),
            Some(ResolvedTextInputTarget {
                session_id: None,
                kind: TextInputTargetKind::Buffer,
                authority: TextInputTargetAuthority::HeuristicModeLine,
            })
        );
    }

    #[test]
    fn bind_target_change_clears_existing_preedit() {
        let mut ime = GuiImeState {
            bound_target: Some(ResolvedTextInputTarget {
                session_id: Some(SessionId(1)),
                kind: TextInputTargetKind::Prompt(StatusStyle::Command),
                authority: TextInputTargetAuthority::ObservedStatusStyle,
            }),
            preedit: "abc".to_string(),
            cursor_range: Some((1, 1)),
            ..GuiImeState::default()
        };

        assert!(ime.bind_target(Some(ResolvedTextInputTarget {
            session_id: Some(SessionId(2)),
            kind: TextInputTargetKind::Buffer,
            authority: TextInputTargetAuthority::HeuristicModeLine,
        })));
        assert!(ime.preedit.is_empty());
        assert_eq!(ime.cursor_range, None);
        assert!(ime.overlay_dirty);
    }
}
