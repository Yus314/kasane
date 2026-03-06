use kasane_core::kasane_component;

#[kasane_component]
fn bad_component(name: &str) {
    let _ = name;
}

fn main() {}
