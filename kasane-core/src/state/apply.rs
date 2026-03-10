use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Attributes, Coord, KakouneRequest};

use super::{AppState, DirtyFlags, InfoIdentity, InfoState, MenuParams, MenuState};

impl AppState {
    pub fn apply(&mut self, request: KakouneRequest) -> DirtyFlags {
        match request {
            KakouneRequest::Draw {
                lines,
                default_face,
                padding_face,
            } => {
                // Extract all cursor positions: atoms with FINAL_FG + REVERSE indicate cursor faces.
                // Track coordinates using grapheme display widths for accurate column positions.
                let mut all_cursors: Vec<Coord> = Vec::new();
                for (line_idx, line) in lines.iter().enumerate() {
                    let mut col: u32 = 0;
                    for atom in line.iter() {
                        let is_cursor = atom.face.attributes.contains(Attributes::FINAL_FG)
                            && atom.face.attributes.contains(Attributes::REVERSE);
                        if is_cursor {
                            all_cursors.push(Coord {
                                line: line_idx as i32,
                                column: col as i32,
                            });
                        }
                        for grapheme in atom.contents.as_str().graphemes(true) {
                            if grapheme.starts_with(|c: char| c.is_control()) {
                                continue;
                            }
                            col += UnicodeWidthStr::width(grapheme) as u32;
                        }
                    }
                }
                self.cursor_count = all_cursors.len();
                self.secondary_cursors = all_cursors
                    .into_iter()
                    .filter(|c| *c != self.cursor_pos)
                    .collect();

                // Line-level dirty tracking: compare old vs new lines
                let face_changed =
                    self.default_face != default_face || self.padding_face != padding_face;
                let len_changed = self.lines.len() != lines.len();

                if face_changed || len_changed {
                    self.lines_dirty = vec![true; lines.len()];
                } else {
                    self.lines_dirty = self
                        .lines
                        .iter()
                        .zip(lines.iter())
                        .map(|(old, new)| old != new)
                        .collect();
                }

                self.lines = lines;
                self.default_face = default_face;
                self.padding_face = padding_face;
                DirtyFlags::BUFFER
            }
            KakouneRequest::DrawStatus {
                status_line,
                mode_line,
                default_face,
            } => {
                self.status_line = status_line;
                self.status_mode_line = mode_line;
                self.status_default_face = default_face;
                DirtyFlags::STATUS
            }
            KakouneRequest::SetCursor { mode, coord } => {
                self.cursor_mode = mode;
                self.cursor_pos = coord;
                DirtyFlags::BUFFER
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
                    tracing::debug!(
                        "MenuSelect: selected={}, first_item={}, win_height={}, items={}, columns={}",
                        selected,
                        menu.first_item,
                        menu.win_height,
                        menu.items.len(),
                        menu.columns,
                    );
                    menu.select(selected);
                }
                DirtyFlags::MENU_SELECTION
            }
            KakouneRequest::MenuHide => {
                self.menu = None;
                DirtyFlags::MENU | DirtyFlags::BUFFER
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
                DirtyFlags::INFO | DirtyFlags::BUFFER
            }
            KakouneRequest::SetUiOptions { options } => {
                self.ui_options = options;
                DirtyFlags::OPTIONS
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
