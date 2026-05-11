kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        active_line: i32 = -1,
    },

    display() {
        if state.active_line < 0 {
            return vec![];
        }
        let bg = theme_style_or(
            "cursor.line.bg",
            if is_dark_background() {
                style_bg(rgb(40, 40, 50))
            } else {
                style_bg(rgb(220, 220, 235))
            },
        );
        vec![style_line(state.active_line as u32, bg)]
    },
}

// -----------------------------------------------------------------------------
// Unit tests via the SDK test harness (cargo test --features test-harness)
// -----------------------------------------------------------------------------

#[cfg(all(test, feature = "test-harness"))]
mod tests {
    use kasane_plugin_sdk::test::{MockBrush, MockStyle, TestHarness};

    use crate::exports::kasane::plugin::plugin_api::Guest;
    use crate::{Brush, DisplayDirective};

    /// Drive `display()` end-to-end with the mock host.
    fn run_display(_h: &mut TestHarness) -> Vec<DisplayDirective> {
        // ADR-044 Phase B-4: `#[bind]` auto-bindings now emit onto the
        // tier-1 export, so trigger the binding via the tier-1 entry point.
        let _ = crate::__KasanePlugin::on_state_changed_tier1_effects(crate::dirty::ALL);
        crate::__KasanePlugin::display()
    }

    #[test]
    fn display_returns_empty_when_active_line_negative() {
        let mut h = TestHarness::new();
        h.set_cursor_line(-1);
        let directives = run_display(&mut h);
        assert!(directives.is_empty(), "expected no directives when cursor < 0");
    }

    #[test]
    fn display_styles_active_line_with_dark_default() {
        let mut h = TestHarness::new();
        h.set_cursor_line(7);
        h.set_dark_background(true);
        // No theme override; falls through to dark-mode default rgb(40,40,50).
        let directives = run_display(&mut h);
        assert_eq!(directives.len(), 1);
        match &directives[0] {
            DisplayDirective::StyleLine(d) => {
                assert_eq!(d.line, 7);
                match &d.style.bg {
                    Brush::Rgb(c) => assert_eq!((c.r, c.g, c.b), (40, 40, 50)),
                    other => panic!("expected dark RGB fallback, got {other:?}"),
                }
            }
            other => panic!("expected StyleLine, got {other:?}"),
        }
    }

    #[test]
    fn display_uses_light_fallback_when_dark_background_false() {
        let mut h = TestHarness::new();
        h.set_cursor_line(3);
        h.set_dark_background(false);
        let directives = run_display(&mut h);
        match &directives[0] {
            DisplayDirective::StyleLine(d) => match &d.style.bg {
                Brush::Rgb(c) => assert_eq!((c.r, c.g, c.b), (220, 220, 235)),
                other => panic!("expected light RGB fallback, got {other:?}"),
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn display_prefers_theme_token_over_fallback() {
        let mut h = TestHarness::new();
        h.set_cursor_line(0);
        h.set_dark_background(true);
        h.set_theme_style(
            "cursor.line.bg",
            MockStyle {
                bg: MockBrush::Rgb { r: 99, g: 99, b: 99 },
                ..MockStyle::default()
            },
        );
        let directives = run_display(&mut h);
        match &directives[0] {
            DisplayDirective::StyleLine(d) => match &d.style.bg {
                Brush::Rgb(c) => assert_eq!((c.r, c.g, c.b), (99, 99, 99)),
                _ => panic!("theme override should produce its own RGB"),
            },
            _ => unreachable!(),
        }
    }
}
