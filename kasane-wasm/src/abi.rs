//! Plugin ABI version compatibility check.
//!
//! Plugins declare the WIT package version they were built against in
//! their manifest's `[plugin] abi_version` field. The host validates this
//! against [`HOST_ABI_VERSION`] at load time, before any wasmtime
//! instantiation is attempted, so that an ABI mismatch produces a
//! human-readable diagnostic rather than the low-level wasmtime linker
//! error "component imports instance `kasane:plugin/host-state@X.Y.Z`,
//! but a matching implementation was not found in the linker".
//!
//! The compatibility rule mirrors the WIT / Component-Model convention:
//! - `0.x.y` and `0.x'.y'` are compatible iff `x == x'` (0.x is treated as
//!   pre-1.0 with breaking changes allowed at minor bumps).
//! - `≥1.0.0` versions are compatible iff the major matches AND the
//!   host's `(minor, patch)` is `>=` the plugin's. A host that adds a
//!   minor-version capability remains backward-compatible with plugins
//!   built against earlier minors of the same major.

/// Host's WIT package version. **Must match line 1 of `wit/plugin.wit`**
/// (`package kasane:plugin@X.Y.Z;`). The
/// [`host_abi_version_matches_wit_package`](self::tests::host_abi_version_matches_wit_package)
/// test enforces the link.
pub const HOST_ABI_VERSION: &str = "5.0.0";

/// Result of comparing a plugin's required ABI version against the host's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiCompat {
    Compatible,
    /// Plugin requires a different major (or, for 0.x, minor) version than
    /// the host. Carries both versions for diagnostic display.
    MajorMismatch {
        required: String,
        host: String,
    },
    /// Plugin's version string failed to parse as `X.Y.Z`.
    Malformed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
}

fn parse(version: &str) -> Option<SemVer> {
    let mut parts = version.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(SemVer {
        major,
        minor,
        patch,
    })
}

/// Check whether a plugin built against `plugin_abi` can run on a host
/// that provides [`HOST_ABI_VERSION`].
pub fn check_compat(plugin_abi: &str) -> AbiCompat {
    check_compat_against(plugin_abi, HOST_ABI_VERSION)
}

/// Same as [`check_compat`] but with an explicit host version, for tests.
pub fn check_compat_against(plugin_abi: &str, host_abi: &str) -> AbiCompat {
    let Some(plugin) = parse(plugin_abi) else {
        return AbiCompat::Malformed(plugin_abi.to_string());
    };
    let Some(host) = parse(host_abi) else {
        return AbiCompat::Malformed(host_abi.to_string());
    };
    let compatible = if plugin.major == 0 && host.major == 0 {
        plugin.minor == host.minor && host.patch >= plugin.patch
    } else {
        plugin.major == host.major && (host.minor, host.patch) >= (plugin.minor, plugin.patch)
    };
    if compatible {
        AbiCompat::Compatible
    } else {
        AbiCompat::MajorMismatch {
            required: plugin_abi.to_string(),
            host: host_abi.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_abi_version_matches_wit_package() {
        let wit = include_str!("../wit/plugin.wit");
        let line = wit.lines().next().expect("wit file is empty");
        let expected = format!("@{HOST_ABI_VERSION};");
        assert!(
            line.contains(&expected),
            "HOST_ABI_VERSION ({HOST_ABI_VERSION}) does not match wit/plugin.wit \
             line 1: {line:?}. Update kasane-wasm/src/abi.rs::HOST_ABI_VERSION \
             whenever wit/plugin.wit's package version changes."
        );
    }

    #[test]
    fn exact_match_is_compatible() {
        assert_eq!(
            check_compat_against("3.0.0", "3.0.0"),
            AbiCompat::Compatible
        );
    }

    #[test]
    fn host_minor_ahead_is_compatible() {
        assert_eq!(
            check_compat_against("3.0.0", "3.2.5"),
            AbiCompat::Compatible
        );
    }

    #[test]
    fn host_minor_behind_is_incompatible() {
        let result = check_compat_against("3.2.0", "3.1.0");
        assert!(matches!(result, AbiCompat::MajorMismatch { .. }));
    }

    #[test]
    fn major_mismatch_is_incompatible() {
        let result = check_compat_against("0.25.0", "3.0.0");
        match result {
            AbiCompat::MajorMismatch { required, host } => {
                assert_eq!(required, "0.25.0");
                assert_eq!(host, "3.0.0");
            }
            other => panic!("expected MajorMismatch, got {other:?}"),
        }
    }

    #[test]
    fn pre_1_0_minor_mismatch_is_incompatible() {
        // 0.x has no inner-major compat: 0.25.0 vs 0.26.0 must differ.
        let result = check_compat_against("0.25.0", "0.26.0");
        assert!(matches!(result, AbiCompat::MajorMismatch { .. }));
    }

    #[test]
    fn pre_1_0_same_minor_higher_patch_is_compatible() {
        assert_eq!(
            check_compat_against("0.25.0", "0.25.3"),
            AbiCompat::Compatible
        );
    }

    #[test]
    fn malformed_returns_malformed() {
        assert_eq!(
            check_compat_against("not-a-version", "3.0.0"),
            AbiCompat::Malformed("not-a-version".to_string())
        );
        assert_eq!(
            check_compat_against("3.0", "3.0.0"),
            AbiCompat::Malformed("3.0".to_string())
        );
        assert_eq!(
            check_compat_against("3.0.0.1", "3.0.0"),
            AbiCompat::Malformed("3.0.0.1".to_string())
        );
    }
}
