use crate::protocol::{Attributes, KakouneRequest};

use super::{AppState, DirtyFlags, InfoIdentity, InfoState, MenuParams, MenuState};

impl AppState {
    pub fn apply(&mut self, request: KakouneRequest) -> DirtyFlags {
        match request {
            KakouneRequest::Draw {
                lines,
                default_face,
                padding_face,
            } => {
                // Count cursor positions: atoms with FINAL_FG + REVERSE indicate cursor faces
                self.cursor_count = lines
                    .iter()
                    .flat_map(|line| line.iter())
                    .filter(|atom| {
                        atom.face.attributes.contains(Attributes::FINAL_FG)
                            && atom.face.attributes.contains(Attributes::REVERSE)
                    })
                    .count();
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
                DirtyFlags::MENU
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
                DirtyFlags::MENU
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
                if force {
                    DirtyFlags::ALL
                } else {
                    DirtyFlags::BUFFER | DirtyFlags::STATUS
                }
            }
        }
    }
}
