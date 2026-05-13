//! Default `ReloadOrchestrator` impl that bridges `kasane-core` hot-reload
//! events into the WASM plugin resolve pipeline.
//!
//! Lives in the binary crate so `kasane-core` doesn't have to depend on
//! `kasane_wasm` or `kasane_plugin_package`.

use anyhow::Result;
use kasane_core::config::Config;
use kasane_core::event_loop::{ReloadOrchestrator, ResolveOutcome};

#[cfg(feature = "wasm-plugins")]
use kasane_core::plugin::{PluginDiagnostic, PluginId};

#[cfg(feature = "wasm-plugins")]
use crate::plugin_cmd::resolve::{ResolveOptions, resolve_and_save};

pub struct DefaultReloadOrchestrator;

#[cfg(feature = "wasm-plugins")]
impl ReloadOrchestrator for DefaultReloadOrchestrator {
    fn resolve_and_signal_reload(&self, config: &Config) -> Result<ResolveOutcome> {
        let saved = resolve_and_save(config, ResolveOptions::reconcile())?;
        let diagnostics = saved
            .result
            .issues
            .into_iter()
            .map(|issue| {
                PluginDiagnostic::runtime_error(
                    PluginId::from(issue.plugin_id),
                    "resolve",
                    issue.reason,
                )
            })
            .collect();
        Ok(ResolveOutcome {
            diagnostics,
            // resolve_and_save touches the reload sentinel internally,
            // which the existing sentinel watcher will pick up to emit
            // Event::PluginReload on the next tick.
            touched_sentinel: true,
        })
    }
}

#[cfg(not(feature = "wasm-plugins"))]
impl ReloadOrchestrator for DefaultReloadOrchestrator {
    fn resolve_and_signal_reload(&self, _config: &Config) -> Result<ResolveOutcome> {
        // Without WASM plugin support there's nothing to resolve.
        Ok(ResolveOutcome::default())
    }
}
