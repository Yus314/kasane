//! ADR-052 load-time import scan.
//!
//! Walks a WASM component's import section and surfaces the names of
//! any `kasane:plugin/host-capabilities/*` functions the guest links
//! against. The host pairs the result with the plugin manifest's
//! `[[capabilities.services]]` declarations: a guest that imports
//! `open-buffer-view` but does not declare the `buffer` service is
//! rejected at load time with a clear error instead of receiving the
//! broker's runtime `open-error::denied`.
//!
//! Import-presence is conservative — it does not mean the plugin
//! actually *calls* the function. But static linkage is the only
//! signal available before instantiation; calling without declaring
//! is, by construction, a manifest bug that the developer wants to
//! see at load time, not at first-call.

use wasmparser::{Parser, Payload};

/// Map an imported `kasane:plugin/*` interface name to the service
/// capability authority required to call functions inside it.
///
/// Kept in lock-step with
/// `kasane_plugin_package::manifest::service_from_name`. Adding a new
/// capability resource means adding the matching interface here.
const INTERFACE_TO_SERVICE: &[(&str, &str)] = &[("host-capabilities", "buffer")];

/// Returns the set of capability service names whose host interface
/// this WASM component imports.
///
/// Names match what `service_from_name` accepts (e.g. `"buffer"`).
/// Component Model imports interfaces as opaque instances — a guest
/// that links `kasane:plugin/host-capabilities` can call any function
/// inside, so the conservative reading is *interface presence implies
/// authority needed*.
pub fn required_services(wasm_bytes: &[u8]) -> Result<Vec<String>, wasmparser::BinaryReaderError> {
    let mut required: Vec<String> = Vec::new();
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload?;
        if let Payload::ComponentImportSection(reader) = payload {
            for import in reader {
                let import = import?;
                let name = import.name.0;
                // Component-model interface imports take the form
                //   "kasane:plugin/host-capabilities@6.5.0"
                // or the unversioned
                //   "kasane:plugin/host-capabilities"
                // We extract the interface segment (between the last
                // `/` and any `@version` suffix) and look it up.
                let Some(after_slash) = name.rsplit('/').next() else {
                    continue;
                };
                let iface = match after_slash.split_once('@') {
                    Some((iface, _ver)) => iface,
                    None => after_slash,
                };
                if let Some(service) = service_for_interface(iface)
                    && !required.iter().any(|s| s == service)
                {
                    required.push(service.to_string());
                }
            }
        }
    }
    Ok(required)
}

fn service_for_interface(iface: &str) -> Option<&'static str> {
    INTERFACE_TO_SERVICE
        .iter()
        .find(|(name, _)| *name == iface)
        .map(|(_, service)| *service)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `bundled/color-preview.wasm` imports `open-buffer-view` after
    /// the chunk-4 migration; the scan must surface `"buffer"`.
    #[test]
    fn color_preview_requires_buffer() {
        let bytes = include_bytes!("../bundled/color-preview.wasm");
        let services = required_services(bytes).expect("scan succeeds");
        assert!(
            services.iter().any(|s| s == "buffer"),
            "expected 'buffer' in {services:?}"
        );
    }

    /// `bundled/cursor-line.wasm` does not import any capability-resource
    /// function (it only uses ambient `host-state` accessors). The scan
    /// must return an empty set.
    #[test]
    fn cursor_line_requires_nothing() {
        let bytes = include_bytes!("../bundled/cursor-line.wasm");
        let services = required_services(bytes).expect("scan succeeds");
        assert!(
            services.is_empty(),
            "expected no required services, got {services:?}"
        );
    }
}
