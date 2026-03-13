use kasane_core::kasane_plugin;

#[kasane_plugin]
mod named_slot_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    /// A custom named slot contribution.
    #[slot("my.plugin.sidebar")]
    pub fn sidebar(_state: &State, core: &AppState) -> Option<Element> {
        let n = core.lines.len();
        Some(Element::text(format!("{n} lines"), Face::default()))
    }

    /// A legacy slot alongside a named slot.
    #[slot(Slot::StatusRight)]
    pub fn status(_state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text("ok", Face::default()))
    }
}

fn main() {
    use kasane_core::plugin::{Plugin, Slot, SlotId};
    use kasane_core::state::{AppState, DirtyFlags};

    let plugin = NamedSlotPluginPlugin::new();
    let state = AppState::default();

    // Legacy slot works
    let result = plugin.contribute(Slot::StatusRight, &state);
    assert!(result.is_some());

    // Named slot works via contribute_named_slot
    let result = plugin.contribute_named_slot("my.plugin.sidebar", &state);
    assert!(result.is_some());

    // Unknown named slot returns None
    let result = plugin.contribute_named_slot("other.slot", &state);
    assert!(result.is_none());

    // contribute_slot delegates correctly for both
    let result = plugin.contribute_slot(&SlotId::STATUS_RIGHT, &state);
    assert!(result.is_some());
    let result = plugin.contribute_slot(&SlotId::new("my.plugin.sidebar"), &state);
    assert!(result.is_some());

    // slot_id_deps for named slot: reads state.lines → BUFFER
    let deps = plugin.slot_id_deps(&SlotId::new("my.plugin.sidebar"));
    assert!(deps.contains(DirtyFlags::BUFFER));

    // slot_deps for legacy slot
    let deps = plugin.slot_deps(Slot::StatusRight);
    assert!(deps.is_empty()); // status fn doesn't read any flagged fields
}
