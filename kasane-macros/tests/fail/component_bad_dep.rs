use kasane_core::kasane_component;

#[kasane_component(deps(BUFFER, INVALID_FLAG))]
fn bad_deps(x: &str) -> String {
    x.to_string()
}

fn main() {}
