use kasane_core::kasane_component;

#[kasane_component]
fn my_component(name: &str) -> String {
    format!("Hello, {name}")
}

fn main() {
    let result = my_component("world");
    assert_eq!(result, "Hello, world");
}
