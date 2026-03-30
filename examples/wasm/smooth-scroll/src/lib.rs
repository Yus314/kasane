kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    settings {
        enabled: bool = false,
    }

    handle_default_scroll(candidate) {
        if !__setting_enabled() {
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
    },
}
