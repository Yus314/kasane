use crate::protocol::KakouneRequest;

use super::derived;
use super::{AppState, DirtyFlags, InfoIdentity, InfoState, MenuParams, MenuState};

impl AppState {
    pub fn apply(&mut self, request: KakouneRequest) -> DirtyFlags {
        match request {
            KakouneRequest::Draw {
                lines,
                cursor_pos,
                default_face,
                padding_face,
                widget_columns,
            } => {
                self.cursor_pos = cursor_pos;

                // Line-level dirty tracking via pure function (computed FIRST
                // so incremental cursor detection can use dirty flags)
                self.lines_dirty = derived::compute_lines_dirty(
                    &self.lines,
                    &lines,
                    &self.default_face,
                    &default_face,
                    &self.padding_face,
                    &padding_face,
                );

                // Heuristic cursor detection — incremental when possible
                let (cursor_count, secondary_cursors) = derived::detect_cursors_incremental(
                    &lines,
                    cursor_pos,
                    &self.lines_dirty,
                    &mut self.cursor_cache,
                );

                // I-1: primary cursor in detected set (self-consistency)
                debug_assert!(
                    derived::check_primary_cursor_in_set(
                        cursor_count,
                        &secondary_cursors,
                        cursor_pos
                    ),
                    "I-1: primary cursor not in detected set (count={cursor_count}, secondaries={}, pos={cursor_pos:?})",
                    secondary_cursors.len(),
                );
                if self
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
                if self
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

                self.cursor_count = cursor_count;
                self.selections = derived::detect_selections(
                    &lines,
                    cursor_pos,
                    &secondary_cursors,
                    &default_face,
                );
                self.secondary_cursors = secondary_cursors;

                self.widget_columns = widget_columns;

                self.lines = lines;
                self.default_face = default_face;
                self.padding_face = padding_face;
                // Clear fold toggle state when buffer content changes (e.g., undo,
                // file switch) so stale fold ranges don't persist.
                self.fold_toggle_state.clear();
                // Re-derive color context when default_face changes
                let new_ctx =
                    crate::render::color_context::ColorContext::derive(&self.default_face);
                if new_ctx != self.color_context {
                    self.color_context = new_ctx;
                    self.theme.apply_color_context(&self.color_context);
                }
                DirtyFlags::BUFFER
            }
            KakouneRequest::DrawStatus {
                prompt,
                content,
                content_cursor_pos,
                mode_line,
                default_face,
                style,
            } => {
                self.status_prompt = prompt.clone();
                self.status_content = content.clone();
                self.status_content_cursor_pos = content_cursor_pos;

                // Derive CursorMode via pure function
                let new_mode = derived::derive_cursor_mode(content_cursor_pos);
                let mode_changed = self.cursor_mode != new_mode;
                self.cursor_mode = new_mode;

                // Combine prompt + content into status_line via pure function
                self.status_line = derived::build_status_line(&prompt, &content);

                self.status_mode_line = mode_line;
                self.status_default_face = default_face;
                self.status_style = style;

                // Derive editor mode from cursor_mode + mode_line
                self.editor_mode =
                    derived::derive_editor_mode(self.cursor_mode, &self.status_mode_line);

                if mode_changed {
                    DirtyFlags::STATUS | DirtyFlags::BUFFER_CURSOR
                } else {
                    DirtyFlags::STATUS
                }
            }
            KakouneRequest::MenuShow {
                items,
                anchor,
                selected_item_face,
                menu_face,
                style,
            } => {
                let screen_h = self.available_height();
                self.menu = Some(MenuState::new(
                    items,
                    MenuParams {
                        anchor,
                        selected_item_face,
                        menu_face,
                        style,
                        screen_w: self.cols,
                        screen_h,
                        max_height: self.menu_max_height,
                    },
                ));
                DirtyFlags::MENU_STRUCTURE
            }
            KakouneRequest::MenuSelect { selected } => {
                if let Some(menu) = &mut self.menu {
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
                self.menu = None;
                DirtyFlags::MENU | DirtyFlags::BUFFER_CONTENT
            }
            KakouneRequest::InfoShow {
                title,
                content,
                anchor,
                face,
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
                    face,
                    style,
                    identity: identity.clone(),
                    scroll_offset: 0,
                };
                // Replace existing info with same identity, or add new
                if let Some(pos) = self.infos.iter().position(|i| i.identity == identity) {
                    self.infos[pos] = new_info;
                } else {
                    self.infos.push(new_info);
                }
                DirtyFlags::INFO
            }
            KakouneRequest::InfoHide => {
                // Remove the most recently added/updated info
                self.infos.pop();
                DirtyFlags::INFO | DirtyFlags::BUFFER_CONTENT
            }
            KakouneRequest::SetUiOptions { options } => {
                if self.ui_options == options {
                    DirtyFlags::empty()
                } else {
                    self.ui_options = options;
                    DirtyFlags::OPTIONS
                }
            }
            KakouneRequest::Refresh { force } => {
                self.lines_dirty = vec![true; self.lines.len()];
                if force {
                    DirtyFlags::ALL
                } else {
                    DirtyFlags::BUFFER | DirtyFlags::STATUS
                }
            }
        }
    }
}
