use kasane::kasane_core::plugin_prelude::*;

struct LineNumbersPlugin;

impl Plugin for LineNumbersPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("line_numbers".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        _state: &(),
        region: &SlotId,
        app: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region != &SlotId::BUFFER_LEFT {
            return None;
        }

        let total = app.line_count();
        let width = total.to_string().len().max(2);

        let children: Vec<_> = (0..total)
            .map(|i| {
                let num = format!("{:>w$} ", i + 1, w = width);
                FlexChild::fixed(Element::text(
                    num,
                    Face {
                        fg: Color::Named(NamedColor::Cyan),
                        ..Face::default()
                    },
                ))
            })
            .collect();

        Some(Contribution {
            element: Element::column(children),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }
}

fn main() {
    kasane::run_with_factories([host_plugin("line_numbers", || {
        PluginBridge::new(LineNumbersPlugin)
    })]);
}
