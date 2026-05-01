//! Element-tree shape probe.
//!
//! Reports `Element` size, child-count distributions across `Flex`,
//! `ResolvedSlot`, `Grid`, and `Stack.overlays`. Used to size the inline
//! buffer of a `SmallVec` migration (β-3) without guessing.
//!
//! ```sh
//! cargo run -p kasane-core --bin element_probe
//! ```

use kasane_core::element::{Element, FlexChild, Overlay};
use kasane_core::plugin::PluginRuntime;
use kasane_core::render::view;
use kasane_core::state::AppState;

fn walk(element: &Element, dist: &mut std::collections::BTreeMap<&'static str, Vec<usize>>) {
    match element {
        Element::Flex { children, .. } => {
            dist.entry("Flex.children")
                .or_default()
                .push(children.len());
            for c in children {
                walk(&c.element, dist);
            }
        }
        Element::ResolvedSlot { children, .. } => {
            dist.entry("ResolvedSlot.children")
                .or_default()
                .push(children.len());
            for c in children {
                walk(&c.element, dist);
            }
        }
        Element::Grid { children, .. } => {
            dist.entry("Grid.children")
                .or_default()
                .push(children.len());
            for c in children {
                walk(c, dist);
            }
        }
        Element::Stack { base, overlays } => {
            dist.entry("Stack.overlays")
                .or_default()
                .push(overlays.len());
            walk(base, dist);
            for o in overlays {
                walk(&o.element, dist);
            }
        }
        Element::Container { child, .. } => walk(child, dist),
        Element::Scrollable { child, .. } => walk(child, dist),
        Element::Interactive { child, .. } => walk(child, dist),
        _ => {}
    }
}

fn main() {
    println!("=== Type sizes ===");
    println!(
        "Element                 : {} bytes",
        std::mem::size_of::<Element>()
    );
    println!(
        "FlexChild               : {} bytes",
        std::mem::size_of::<FlexChild>()
    );
    println!(
        "Overlay                 : {} bytes",
        std::mem::size_of::<Overlay>()
    );
    println!(
        "Vec<FlexChild>          : {} bytes",
        std::mem::size_of::<Vec<FlexChild>>()
    );
    println!(
        "Vec<Element>            : {} bytes",
        std::mem::size_of::<Vec<Element>>()
    );
    println!(
        "Vec<Overlay>            : {} bytes",
        std::mem::size_of::<Vec<Overlay>>()
    );
    println!();

    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.lines = std::sync::Arc::new(
        (0..23)
            .map(|i| vec![kasane_core::protocol::Atom::plain(format!("line {i}"))])
            .collect(),
    );
    let registry = PluginRuntime::new();
    let element = view::view(&state, &registry.view());

    let mut dist = std::collections::BTreeMap::new();
    walk(&element, &mut dist);

    println!("=== Children-count distribution (default 80x24 view, no plugins) ===");
    for (name, sizes) in &dist {
        let mut by_count = std::collections::BTreeMap::<usize, usize>::new();
        for s in sizes {
            *by_count.entry(*s).or_default() += 1;
        }
        let total = sizes.len();
        let max = *sizes.iter().max().unwrap_or(&0);
        let mean: f64 = sizes.iter().map(|x| *x as f64).sum::<f64>() / total.max(1) as f64;
        print!("  {name:25}: nodes={total:4} mean={mean:5.2} max={max:3}  hist=[");
        let mut first = true;
        for (k, v) in &by_count {
            if !first {
                print!(", ");
            }
            print!("{k}:{v}");
            first = false;
        }
        println!("]");
    }
}
