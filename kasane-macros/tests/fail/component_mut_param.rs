use kasane_core::kasane_component;

#[kasane_component]
fn bad_component(data: &mut Vec<u8>) -> usize {
    data.len()
}

fn main() {}
