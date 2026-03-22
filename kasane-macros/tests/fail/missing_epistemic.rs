use kasane_core::DirtyTracked;

#[derive(DirtyTracked)]
struct Bad {
    #[dirty(free)]
    pub x: u32,
}

fn main() {}
