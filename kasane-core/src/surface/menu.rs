//! MenuSurface: built-in Surface for the completion menu overlay.
//!
//! Wraps the existing menu rendering logic from `render::view::menu` as a
//! first-class Surface. Created dynamically when a menu appears and removed
//! when it disappears.

use crate::element::Element;
use crate::plugin::Command;
use crate::state::{AppState, DirtyFlags};

use super::{EventContext, SizeHint, Surface, SurfaceEvent, SurfaceId, ViewContext};

/// Built-in surface for the completion menu overlay.
///
/// This surface is ephemeral — it is registered in the SurfaceRegistry when
/// `AppState::menu` becomes `Some` and removed when it becomes `None`.
pub struct MenuSurface;

impl Surface for MenuSurface {
    fn id(&self) -> SurfaceId {
        SurfaceId::MENU
    }

    fn size_hint(&self) -> SizeHint {
        // Menus are overlays — size is determined by content and anchor position
        SizeHint::fill()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        // Delegate to the existing menu rendering pipeline.
        // The actual overlay positioning is handled by the Overlay/OverlayAnchor
        // system in the view layer, not by Surface layout.
        if let Some(menu_state) = ctx.state.menu.as_ref() {
            use crate::plugin::{DecorateTarget, ReplaceTarget};
            use crate::protocol::MenuStyle;
            use crate::render::view::menu;

            let replace_target = match menu_state.style {
                MenuStyle::Prompt => ReplaceTarget::MenuPrompt,
                MenuStyle::Inline => ReplaceTarget::MenuInline,
                MenuStyle::Search => ReplaceTarget::MenuSearch,
            };
            let menu_overlay = match ctx.registry.get_replacement(replace_target, ctx.state) {
                Some(replacement) => {
                    menu::build_replacement_menu_overlay(replacement, menu_state, ctx.state)
                }
                None => menu::build_menu_overlay(menu_state, ctx.state, ctx.registry),
            };
            match menu_overlay {
                Some(mut overlay) => {
                    overlay.element = ctx.registry.apply_decorator(
                        DecorateTarget::Menu,
                        overlay.element,
                        ctx.state,
                    );
                    overlay.element
                }
                None => Element::Empty,
            }
        } else {
            Element::Empty
        }
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        vec![]
    }

    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }
}
