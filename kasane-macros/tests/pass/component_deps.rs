use kasane_core::kasane_component;

#[kasane_component(deps(BUFFER, STATUS, OPTIONS))]
fn my_base(name: &str) -> String {
    format!("base: {name}")
}

#[kasane_component(deps(MENU_STRUCTURE, MENU_SELECTION))]
fn my_menu(items: &[u32]) -> usize {
    items.len()
}

#[kasane_component(deps(INFO))]
fn my_info() -> bool {
    true
}

#[kasane_component(deps(ALL))]
fn my_all() -> u32 {
    42
}

#[kasane_component(deps(MENU))]
fn my_composite() -> u32 {
    0
}

fn main() {
    let _ = my_base("test");
    let _ = my_menu(&[1, 2, 3]);
    let _ = my_info();
    let _ = my_all();
    let _ = my_composite();
}
