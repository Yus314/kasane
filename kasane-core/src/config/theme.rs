//! Theme configuration: face specs and token references with dark/light variants.

use std::collections::HashMap;

/// A theme value: either a direct face spec or a reference to another token.
#[derive(Debug, Clone, PartialEq)]
pub enum ThemeValue {
    /// A direct face specification (e.g., `"cyan,blue+b"`).
    FaceSpec(String),
    /// A reference to another theme token (the `@` prefix is stripped).
    TokenRef(String),
}

/// Theme configuration: maps style token names to face specs or token references.
///
/// Supports `@token_name` references and dark/light variants.
///
/// Example in kasane.kdl:
/// ```kdl
/// theme {
///     accent "green"
///     status_line "white,rgb:303030"
///     status_mode "@accent"
///
///     variant "dark" {
///         accent "cyan"
///     }
///     variant "light" {
///         accent "blue"
///     }
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct ThemeConfig {
    pub faces: HashMap<String, ThemeValue>,
    pub variants: HashMap<String, HashMap<String, ThemeValue>>,
}
