//! Render-ornament collection (RENDER_ORNAMENT plugins).

use crate::plugin::traits::PluginBackend;
use crate::plugin::{AppView, PluginCapabilities, RenderOrnamentContext};

use super::super::{CollectedOrnaments, PluginView};

impl<'a> PluginView<'a> {
    pub fn collect_ornaments(
        &self,
        state: &AppView<'_>,
        ctx: &RenderOrnamentContext,
    ) -> CollectedOrnaments {
        let mut emphasis = Vec::new();
        let mut cursor_style: Option<(crate::plugin::CursorStyleOrn, usize)> = None;
        let mut cursor_position: Option<(crate::plugin::CursorPositionOrn, usize)> = None;
        let mut cursor_effects = Vec::new();
        let mut surfaces = Vec::new();

        for (idx, slot) in self.slots.iter().enumerate() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::RENDER_ORNAMENT)
            {
                continue;
            }
            let batch = slot.backend.render_ornaments(state, ctx);
            if batch.is_empty() {
                continue;
            }

            emphasis.extend(batch.emphasis);

            if let Some(candidate) = batch.cursor_style {
                let replace = match &cursor_style {
                    None => true,
                    Some((current, _)) => {
                        let lhs = (candidate.modality.rank(), candidate.priority);
                        let rhs = (current.modality.rank(), current.priority);
                        lhs > rhs
                    }
                };
                if replace {
                    cursor_style = Some((candidate, idx));
                }
            }

            if let Some(candidate) = batch.cursor_position {
                let replace = match &cursor_position {
                    None => true,
                    Some((current, _)) => {
                        let lhs = (candidate.modality.rank(), candidate.priority);
                        let rhs = (current.modality.rank(), current.priority);
                        lhs > rhs
                    }
                };
                if replace {
                    cursor_position = Some((candidate, idx));
                }
            }

            cursor_effects.extend(batch.cursor_effects);
            surfaces.extend(batch.surfaces);
        }

        emphasis.sort_by_key(|d| d.priority);

        CollectedOrnaments {
            emphasis,
            cursor_style: cursor_style.map(|(orn, _)| orn.hint),
            cursor_position: cursor_position.map(|(orn, _)| (orn.x, orn.y, orn.style, orn.color)),
            cursor_effects,
            surfaces,
        }
    }
}
