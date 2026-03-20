kasane_plugin_sdk::generate!();

use kasane_plugin_sdk::plugin;

fn smooth_scroll_enabled() -> bool {
    host_state::get_config_string("smooth-scroll.enabled")
        .or_else(|| host_state::get_config_string("smooth_scroll"))
        .and_then(|raw| raw.parse::<bool>().ok())
        .unwrap_or(false)
}

struct SmoothScrollPlugin;

#[plugin]
impl Guest for SmoothScrollPlugin {
    fn get_id() -> String {
        "smooth_scroll".to_string()
    }

    fn state_hash() -> u64 {
        0
    }

    fn handle_default_scroll(candidate: DefaultScrollCandidate) -> Option<ScrollPolicyResult> {
        if !smooth_scroll_enabled() {
            return None;
        }

        Some(ScrollPolicyResult::Plan(ScrollPlan {
            total_amount: candidate.resolved.amount,
            line: candidate.resolved.line,
            column: candidate.resolved.column,
            frame_interval_ms: 16,
            curve: ScrollCurve::Linear,
            accumulation: ScrollAccumulationMode::Add,
        }))
    }
}

export!(SmoothScrollPlugin);
