use crate::protocol::KakouneRequest;
use crate::render::color_context::ColorContext;

use super::derived;
use super::derived::InferenceStrategy;
use super::{
    AppState, ConfigState, DirtyFlags, InferenceState, InfoIdentity, InfoState, MenuParams,
    MenuState, ObservedState, RuntimeState,
};

/// Config-side reactions that protocol ingestion detected but cannot apply
/// (because `apply_protocol` receives `&ConfigState`, not `&mut ConfigState`).
///
/// The caller (`update_inner`) applies these after `apply_protocol` returns.
#[derive(Debug, Default)]
pub(crate) struct ConfigReactions {
    /// Buffer content changed — fold toggle state should be cleared.
    pub clear_fold_toggle: bool,
    /// New color context derived from default_face — theme should be updated.
    pub new_color_context: Option<ColorContext>,
}

/// Protocol ingestion: updates observed + inference state from a Kakoune message.
///
/// Takes `&ConfigState` (immutable) so that writing config from the protocol
/// path is a compile error. Config-side reactions are returned in
/// `ConfigReactions` for the caller to apply.
pub(crate) fn apply_protocol(
    observed: &mut ObservedState,
    inference: &mut InferenceState,
    cursor_cache: &mut derived::CursorCache,
    config: &ConfigState,
    runtime: &RuntimeState,
    strategy: &dyn InferenceStrategy,
    request: KakouneRequest,
) -> (DirtyFlags, ConfigReactions) {
    let mut reactions = ConfigReactions::default();

    let flags = match request {
        KakouneRequest::Draw {
            lines,
            cursor_pos,
            default_style,
            padding_style,
            widget_columns,
        } => {
            observed.cursor_pos = cursor_pos;

            // Line-level dirty tracking via pure function (computed FIRST
            // so incremental cursor detection can use dirty flags)
            let observed_default_face = observed.default_style.to_face();
            let observed_padding_face = observed.padding_style.to_face();
            // Bridge to WireFace for `compute_lines_dirty` and `detect_selections`
            // until those functions migrate (Phase B3 follow-up).
            let default_face = default_style.to_face();
            let padding_face = padding_style.to_face();
            inference.lines_dirty = derived::compute_lines_dirty(
                &observed.lines,
                &lines,
                &observed_default_face,
                &default_face,
                &observed_padding_face,
                &padding_face,
            );

            // Heuristic cursor detection — incremental when possible
            let (cursor_count, secondary_cursors) =
                strategy.detect_cursors(&lines, cursor_pos, &inference.lines_dirty, cursor_cache);

            // I-1: primary cursor in detected set (self-consistency)
            debug_assert!(
                derived::check_primary_cursor_in_set(cursor_count, &secondary_cursors, cursor_pos),
                "I-1: primary cursor not in detected set (count={cursor_count}, secondaries={}, pos={cursor_pos:?})",
                secondary_cursors.len(),
            );
            if observed
                .ui_options
                .get("kasane_debug_inference")
                .map(|v| v == "true")
                .unwrap_or(false)
                && !derived::check_primary_cursor_in_set(
                    cursor_count,
                    &secondary_cursors,
                    cursor_pos,
                )
            {
                tracing::warn!(
                    cursor_count,
                    secondaries = secondary_cursors.len(),
                    ?cursor_pos,
                    "I-1: primary cursor not in detected set",
                );
            }

            // R-1: character width divergence detection
            debug_assert!(
                derived::check_cursor_width_consistency(&lines, cursor_pos).is_none(),
                "R-1: cursor width divergence: {:?}",
                derived::check_cursor_width_consistency(&lines, cursor_pos),
            );
            if observed
                .ui_options
                .get("kasane_debug_inference")
                .map(|v| v == "true")
                .unwrap_or(false)
                && let Some(div) = derived::check_cursor_width_consistency(&lines, cursor_pos)
            {
                tracing::warn!(
                    protocol_column = div.protocol_column,
                    computed_column = div.computed_column,
                    atom_text = %div.atom_text,
                    "R-1: cursor width divergence detected",
                );
            }

            // Lightweight always-on check: cursor shouldn't be beyond line width
            if let Some(line) = lines.get(cursor_pos.line as usize) {
                let line_width = derived::line_atom_display_width(line);
                debug_assert!(
                    (cursor_pos.column as u32) <= line_width,
                    "R-1: cursor column {} beyond line display width {line_width}",
                    cursor_pos.column,
                );
            }

            inference.cursor_count = cursor_count;
            inference.selections =
                strategy.detect_selections(&lines, cursor_pos, &secondary_cursors, &default_face);
            inference.secondary_cursors = secondary_cursors;

            observed.widget_columns = widget_columns;

            observed.lines = lines;
            observed.default_style = default_style.style.clone();
            observed.padding_style = padding_style.style.clone();

            // Signal config reactions (applied by caller)
            reactions.clear_fold_toggle = true;

            let new_ctx = ColorContext::derive(&observed.default_style.to_face());
            if new_ctx != inference.color_context {
                reactions.new_color_context = Some(new_ctx.clone());
                inference.color_context = new_ctx;
            }

            DirtyFlags::BUFFER
        }
        KakouneRequest::DrawStatus {
            prompt,
            content,
            content_cursor_pos,
            mode_line,
            default_style,
            style,
        } => {
            observed.status_prompt = prompt.clone();
            observed.status_content = content.clone();
            observed.status_content_cursor_pos = content_cursor_pos;

            // Derive CursorMode via pure function
            let new_mode = derived::derive_cursor_mode(content_cursor_pos);
            let mode_changed = inference.cursor_mode != new_mode;
            inference.cursor_mode = new_mode;

            // Combine prompt + content into status_line via pure function
            inference.status_line = derived::build_status_line(&prompt, &content);

            observed.status_mode_line = mode_line;
            observed.status_default_style = default_style.style.clone();
            observed.status_style = style;

            // Derive editor mode from cursor_mode + mode_line
            inference.editor_mode =
                strategy.derive_editor_mode(inference.cursor_mode, &observed.status_mode_line);

            if mode_changed {
                DirtyFlags::STATUS | DirtyFlags::BUFFER_CURSOR
            } else {
                DirtyFlags::STATUS
            }
        }
        KakouneRequest::MenuShow {
            items,
            anchor,
            selected_item_style,
            menu_style,
            style,
        } => {
            let screen_h = runtime.rows.saturating_sub(1);
            observed.menu = Some(MenuState::new(
                items,
                MenuParams {
                    anchor,
                    selected_item_face: selected_item_style.style.clone(),
                    menu_face: menu_style.style.clone(),
                    style,
                    screen_w: runtime.cols,
                    screen_h,
                    max_height: config.menu_max_height,
                },
            ));
            DirtyFlags::MENU_STRUCTURE
        }
        KakouneRequest::MenuSelect { selected } => {
            if let Some(menu) = &mut observed.menu {
                let old_first_item = menu.first_item;
                tracing::debug!(
                    "MenuSelect: selected={}, first_item={}, win_height={}, items={}, columns={}",
                    selected,
                    menu.first_item,
                    menu.win_height,
                    menu.items.len(),
                    menu.columns,
                );
                menu.select(selected);
                if menu.first_item != old_first_item {
                    DirtyFlags::MENU_SELECTION | DirtyFlags::MENU_STRUCTURE
                } else {
                    DirtyFlags::MENU_SELECTION
                }
            } else {
                DirtyFlags::MENU_SELECTION
            }
        }
        KakouneRequest::MenuHide => {
            observed.menu = None;
            DirtyFlags::MENU | DirtyFlags::BUFFER_CONTENT
        }
        KakouneRequest::InfoShow {
            title,
            content,
            anchor,
            info_style,
            style,
        } => {
            let identity = InfoIdentity {
                style,
                anchor_line: anchor.line as u32,
            };
            let new_info = InfoState {
                title,
                content,
                anchor,
                face: info_style.style.clone(),
                style,
                identity: identity.clone(),
                scroll_offset: 0,
            };
            // Replace existing info with same identity, or add new
            if let Some(pos) = observed.infos.iter().position(|i| i.identity == identity) {
                observed.infos[pos] = new_info;
            } else {
                observed.infos.push(new_info);
            }
            DirtyFlags::INFO
        }
        KakouneRequest::InfoHide => {
            // Remove the most recently added/updated info
            observed.infos.pop();
            DirtyFlags::INFO | DirtyFlags::BUFFER_CONTENT
        }
        KakouneRequest::SetUiOptions { options } => {
            if observed.ui_options == options {
                DirtyFlags::empty()
            } else {
                observed.ui_options = options;
                DirtyFlags::OPTIONS
            }
        }
        KakouneRequest::Refresh { force } => {
            inference.lines_dirty = vec![true; observed.lines.len()];
            if force {
                DirtyFlags::ALL
            } else {
                DirtyFlags::BUFFER | DirtyFlags::STATUS
            }
        }
    };

    (flags, reactions)
}

impl AppState {
    /// Apply a Kakoune JSON-RPC request to the state.
    ///
    /// Thin wrapper around [`apply_protocol()`] that delegates to the free
    /// function and applies config reactions.
    pub fn apply(&mut self, request: KakouneRequest) -> DirtyFlags {
        let strategy = self.runtime.inference_strategy.clone();
        let (flags, reactions) = apply_protocol(
            &mut self.observed,
            &mut self.inference,
            &mut self.cursor_cache,
            &self.config,
            &self.runtime,
            &*strategy,
            request,
        );

        // Apply config reactions that protocol ingestion signalled
        if reactions.clear_fold_toggle {
            self.config.fold_toggle_state.clear();
        }
        if let Some(ctx) = reactions.new_color_context {
            self.config.theme.apply_color_context(&ctx);
        }

        flags
    }
}
