wit_bindgen::generate!({
    world: "bench-plugin",
    path: "../../wit",
});

use exports::kasane::bench::plugin_api::{
    Color, WireFace, FaceColor, Guest, GutterResult, LineDecoration, TextElement,
};

struct BenchPlugin;

impl Guest for BenchPlugin {
    fn noop() {}

    fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    fn echo_string(input: String) -> String {
        input
    }

    fn build_gutter(line_count: u32) -> GutterResult {
        let lines = (1..=line_count)
            .map(|i| {
                let width = (line_count as f32).log10() as usize + 1;
                let width = width.max(2);
                TextElement {
                    content: format!("{:>w$} ", i, w = width),
                    face: WireFace {
                        fg: FaceColor::Rgb(Color {
                            r: 0,
                            g: 200,
                            b: 200,
                        }),
                        bg: FaceColor::None,
                        bold: false,
                    },
                }
            })
            .collect();
        GutterResult { lines }
    }

    fn on_state_changed(dirty_flags: u16) {
        if dirty_flags & 0x01 != 0 {
            let _line = kasane::bench::host_api::get_cursor_line();
            let _col = kasane::bench::host_api::get_cursor_col();
            let _count = kasane::bench::host_api::get_line_count();
        }
    }

    fn contribute_lines(start: u32, end_exclusive: u32) -> Vec<Option<LineDecoration>> {
        let cursor = kasane::bench::host_api::get_cursor_line() as u32;
        (start..end_exclusive)
            .map(|line| {
                if line == cursor {
                    Some(LineDecoration {
                        has_background: true,
                        bg_r: 40,
                        bg_g: 40,
                        bg_b: 50,
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

export!(BenchPlugin);
