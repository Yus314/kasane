//! ADR-032 W5 spike — performance bench placeholder.
//!
//! The real bench will mirror `kasane-gui/benches/cpu_rendering.rs`
//! and produce `target/spike-report.json` for the W5 measurement
//! matrix (warm 80×24, cursor-only, color emoji DSSIM, etc.).
//!
//! Without `--features with-vello`, this is a no-op so CI / workspace
//! checks do not require Vello to be present.

fn main() {
    if !kasane_vello_spike::VelloBackend::is_active() {
        eprintln!(
            "kasane-vello-spike bench: 'with-vello' feature is off; \
             nothing to measure. Re-run with `--features with-vello` \
             once Vello adoption gates open (see ADR-032)."
        );
        return;
    }

    // ADR-032 W5 Day 1-5 fills this in with Criterion harness +
    // measurement matrix.
    eprintln!(
        "kasane-vello-spike bench: spike not yet implemented. \
         See docs/decisions.md ADR-032 §Spike Plan."
    );
}
