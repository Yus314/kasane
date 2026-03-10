use kasane::kasane_core::plugin_prelude::*;

#[kasane_plugin]
mod line_numbers {
    use kasane::kasane_core::plugin_prelude::*;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[slot(Slot::BufferLeft)]
    pub fn gutter(_state: &State, core: &AppState) -> Option<Element> {
        let total = core.lines.len();
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

        Some(Element::column(children))
    }
}

fn main() {
    kasane::run(|registry| {
        registry.register(Box::new(LineNumbersPlugin::new()));
    });
}
