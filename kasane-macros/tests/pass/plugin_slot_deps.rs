use kasane_core::kasane_plugin;

#[kasane_plugin]
mod deps_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    /// This slot reads state.lines (BUFFER) — slot_deps should auto-derive BUFFER.
    #[slot(Slot::BufferLeft)]
    pub fn gutter(_state: &State, core: &AppState) -> Option<Element> {
        let n = core.lines.len();
        Some(Element::text(format!("{n}"), Face::default()))
    }

    /// This slot reads state.status_line (STATUS) — slot_deps should auto-derive STATUS.
    #[slot(Slot::StatusRight)]
    pub fn status(_state: &State, core: &AppState) -> Option<Element> {
        if core.status_line.is_empty() {
            None
        } else {
            Some(Element::text("ok", Face::default()))
        }
    }
}

fn main() {
    use kasane_core::plugin::{Plugin, Slot};
    use kasane_core::state::DirtyFlags;

    let plugin = DepsPluginPlugin::new();

    // BufferLeft reads state.lines → should include BUFFER
    let deps = plugin.slot_deps(Slot::BufferLeft);
    assert!(deps.contains(DirtyFlags::BUFFER));
    assert!(!deps.contains(DirtyFlags::STATUS));

    // StatusRight reads state.status_line → should include STATUS
    let deps = plugin.slot_deps(Slot::StatusRight);
    assert!(deps.contains(DirtyFlags::STATUS));
    assert!(!deps.contains(DirtyFlags::BUFFER));

    // Unused slot → empty
    let deps = plugin.slot_deps(Slot::Overlay);
    assert!(deps.is_empty());
}
