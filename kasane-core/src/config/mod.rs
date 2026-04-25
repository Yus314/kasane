use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use kasane_plugin_model::SettingValue;

pub mod kdl_parser;
pub mod kdl_writer;
pub mod unified;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub ui: UiConfig,
    pub scroll: ScrollConfig,
    pub log: LogConfig,
    pub theme: ThemeConfig,
    pub menu: MenuConfig,
    pub search: SearchConfig,
    pub clipboard: ClipboardConfig,
    pub mouse: MouseConfig,
    pub window: WindowConfig,
    pub font: FontConfig,
    pub colors: ColorsConfig,
    pub plugins: PluginsConfig,
    pub effects: EffectsConfig,
    /// Per-plugin typed settings: `settings { <plugin_id> { key value; ... } }`.
    pub settings: HashMap<String, HashMap<String, SettingValue>>,
}

/// Menu configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuConfig {
    pub position: MenuPosition,
    pub max_height: u16,
}

impl Default for MenuConfig {
    fn default() -> Self {
        MenuConfig {
            position: MenuPosition::Auto,
            max_height: 10,
        }
    }
}

/// Menu position: auto (default Kakoune behavior), above, or below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuPosition {
    #[default]
    Auto,
    Above,
    Below,
}

/// Search menu configuration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchConfig {
    /// When true, show search completions as a vertical dropdown instead of inline.
    pub dropdown: bool,
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct UiConfig {
    pub shadow: bool,
    pub padding_char: String,
    pub border_style: BorderStyleConfig,
    pub status_position: StatusPosition,
    pub backend: String,
    /// Enable the scene-based GPU renderer (bypasses CellGrid). `None` = auto (true for GUI).
    pub scene_renderer: Option<bool>,
    /// Image rendering protocol: "auto" (detect terminal), "halfblock", "kitty".
    pub image_protocol: ImageProtocolConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            shadow: true,
            padding_char: "~".to_string(),
            border_style: BorderStyleConfig::Rounded,
            status_position: StatusPosition::Bottom,
            backend: "tui".to_string(),
            scene_renderer: None,
            image_protocol: ImageProtocolConfig::Auto,
        }
    }
}

/// Status bar position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusPosition {
    Top,
    #[default]
    Bottom,
}

/// Border line style configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyleConfig {
    Single,
    #[default]
    Rounded,
    Double,
    Heavy,
    Ascii,
}

impl From<BorderStyleConfig> for crate::element::BorderLineStyle {
    fn from(config: BorderStyleConfig) -> Self {
        match config {
            BorderStyleConfig::Single => crate::element::BorderLineStyle::Single,
            BorderStyleConfig::Rounded => crate::element::BorderLineStyle::Rounded,
            BorderStyleConfig::Double => crate::element::BorderLineStyle::Double,
            BorderStyleConfig::Heavy => crate::element::BorderLineStyle::Heavy,
            BorderStyleConfig::Ascii => crate::element::BorderLineStyle::Ascii,
        }
    }
}

/// Image rendering protocol configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageProtocolConfig {
    #[default]
    Auto,
    Halfblock,
    Kitty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollConfig {
    pub lines_per_scroll: i32,
    pub smooth: bool,
    pub inertia: bool,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        ScrollConfig {
            lines_per_scroll: 3,
            smooth: false,
            inertia: false,
        }
    }
}

/// Clipboard configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardConfig {
    pub enabled: bool,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        ClipboardConfig { enabled: true }
    }
}

/// Mouse configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseConfig {
    pub drag_scroll: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        MouseConfig { drag_scroll: true }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogConfig {
    pub level: String,
    pub file: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: "warn".to_string(),
            file: None,
        }
    }
}

/// Window configuration for the GUI backend.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowConfig {
    pub initial_cols: u16,
    pub initial_rows: u16,
    pub fullscreen: bool,
    pub maximized: bool,
    /// Override GPU present mode (e.g. "Fifo", "Mailbox", "AutoVsync", "AutoNoVsync").
    pub present_mode: Option<String>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            initial_cols: 80,
            initial_rows: 24,
            fullscreen: false,
            maximized: false,
            present_mode: None,
        }
    }
}

/// Font configuration for the GUI backend.
#[derive(Debug, Clone, PartialEq)]
pub struct FontConfig {
    pub family: String,
    pub size: f32,
    pub style: String,
    pub fallback_list: Vec<String>,
    pub line_height: f32,
    pub letter_spacing: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        FontConfig {
            family: "monospace".to_string(),
            size: 14.0,
            style: "Regular".to_string(),
            fallback_list: Vec::new(),
            line_height: 1.2,
            letter_spacing: 0.0,
        }
    }
}

/// Color palette for the GUI backend.
/// Kakoune's terminal UI uses `Color::Default` to mean "terminal default",
/// but the GUI has no terminal — these values define the concrete RGB fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorsConfig {
    pub default_fg: String,
    pub default_bg: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        // VS Code Dark+ inspired defaults
        ColorsConfig {
            default_fg: "#d4d4d4".to_string(),
            default_bg: "#1e1e1e".to_string(),
            black: "#000000".to_string(),
            red: "#cd3131".to_string(),
            green: "#0dbc79".to_string(),
            yellow: "#e5e510".to_string(),
            blue: "#2472c8".to_string(),
            magenta: "#bc3fbc".to_string(),
            cyan: "#11a8cd".to_string(),
            white: "#cccccc".to_string(),
            bright_black: "#666666".to_string(),
            bright_red: "#f14c4c".to_string(),
            bright_green: "#23d18b".to_string(),
            bright_yellow: "#f5f543".to_string(),
            bright_blue: "#3b8eea".to_string(),
            bright_magenta: "#d670d6".to_string(),
            bright_cyan: "#29b8db".to_string(),
            bright_white: "#e5e5e5".to_string(),
        }
    }
}

/// GPU visual effects configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectsConfig {
    /// Background gradient start color (top). `None` = disabled.
    pub gradient_start: Option<[f32; 4]>,
    /// Background gradient end color (bottom). `None` = disabled.
    pub gradient_end: Option<[f32; 4]>,
    /// Cursor line highlight mode.
    pub cursor_line_highlight: CursorLineHighlightMode,
    /// Overlay transition duration in milliseconds.
    pub overlay_transition_ms: u16,
    /// Enable backdrop blur (frosted glass) behind floating overlays.
    pub backdrop_blur: bool,
    /// Text post-processing effects (shadow, glow, outline).
    pub text_effects: TextEffectsConfig,
}

impl Default for EffectsConfig {
    fn default() -> Self {
        EffectsConfig {
            gradient_start: None,
            gradient_end: None,
            cursor_line_highlight: CursorLineHighlightMode::Off,
            overlay_transition_ms: 150,
            backdrop_blur: false,
            text_effects: TextEffectsConfig::default(),
        }
    }
}

/// Text post-processing effects: shadow, glow, outline.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEffectsConfig {
    /// Shadow offset in pixels (dx, dy). `None` = shadow disabled.
    pub shadow_offset: Option<(f32, f32)>,
    /// Shadow color (linear RGBA).
    pub shadow_color: [f32; 4],
    /// Shadow blur radius in pixels.
    pub shadow_blur: f32,
    /// Glow radius in pixels. 0.0 = disabled.
    pub glow_radius: f32,
    /// Glow color (linear RGBA).
    pub glow_color: [f32; 4],
}

impl Default for TextEffectsConfig {
    fn default() -> Self {
        Self {
            shadow_offset: None,
            shadow_color: [0.0, 0.0, 0.0, 0.6],
            shadow_blur: 2.0,
            glow_radius: 0.0,
            glow_color: [1.0, 1.0, 1.0, 0.3],
        }
    }
}

impl TextEffectsConfig {
    /// Returns `true` if any text effect is enabled.
    pub fn is_active(&self) -> bool {
        self.shadow_offset.is_some() || self.glow_radius > 0.0
    }
}

/// Cursor line highlight rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorLineHighlightMode {
    /// No cursor line highlight.
    #[default]
    Off,
    /// Subtle foreground-tinted highlight (alpha=0.03).
    Subtle,
}

/// Plugin configuration.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PluginsConfig {
    /// Custom path to the plugins directory. Defaults to XDG_DATA_HOME/kasane/plugins/.
    pub path: Option<String>,
    /// Bundled plugin IDs to enable (opt-in). Bundled plugins are NOT loaded unless
    /// listed here, except for default-enabled plugins (e.g. "pane_manager").
    /// Available: "cursor_line", "color_preview", "sel_badge", "fuzzy_finder", "pane_manager".
    pub enabled: Vec<String>,
    /// Plugin IDs to disable (by plugin ID, e.g. "cursor_line").
    /// Applies to filesystem-discovered and user-registered plugins.
    pub disabled: Vec<String>,
    /// Per-plugin capability denials. Key: plugin ID, Value: list of denied capability names.
    /// Valid capability names: "filesystem", "environment", "monotonic-clock", "process".
    pub deny_capabilities: HashMap<String, Vec<String>>,
    /// Per-plugin authority denials. Key: plugin ID, Value: list of denied authority names.
    /// Valid authority names: "dynamic-surface", "pty-process".
    pub deny_authorities: HashMap<String, Vec<String>>,
    /// Per-plugin active-set selection policy.
    pub selection: HashMap<String, PluginSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PluginSelection {
    #[default]
    Auto,
    PinDigest {
        digest: String,
    },
    PinPackage {
        package: String,
        version: Option<String>,
    },
}

impl PluginsConfig {
    /// Check if a bundled plugin should be loaded (opt-in via `enabled` list).
    pub fn is_bundled_enabled(&self, id: &str) -> bool {
        self.enabled.iter().any(|s| s == id)
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.iter().any(|s| s == id)
    }

    pub fn selection_for(&self, id: &str) -> PluginSelection {
        self.selection.get(id).cloned().unwrap_or_default()
    }

    /// Resolve the plugins directory path.
    pub fn plugins_dir(&self) -> PathBuf {
        if let Some(ref p) = self.path {
            PathBuf::from(p)
        } else {
            dirs_data_path().join("plugins")
        }
    }
}

fn dirs_data_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("kasane")
    } else {
        PathBuf::from("kasane-data")
    }
}

impl Config {
    /// Compare two configs and return the names of fields that require a restart
    /// to take effect (i.e., cannot be hot-reloaded).
    pub fn restart_required_diff(&self, new: &Config) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.ui.backend != new.ui.backend {
            fields.push("ui.backend");
        }
        if self.ui.border_style != new.ui.border_style {
            fields.push("ui.border_style");
        }
        if self.ui.image_protocol != new.ui.image_protocol {
            fields.push("ui.image_protocol");
        }
        if self.scroll.lines_per_scroll != new.scroll.lines_per_scroll {
            fields.push("scroll.lines_per_scroll");
        }
        if self.window != new.window {
            fields.push("window");
        }
        if self.font != new.font {
            fields.push("font");
        }
        if self.log != new.log {
            fields.push("log");
        }
        if self.plugins != new.plugins {
            fields.push("plugins");
        }
        fields
    }

    pub fn load() -> Self {
        let config_path = config_path();
        let contents = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Config::default(),
            Err(e) => {
                eprintln!(
                    "warning: cannot read {}: {e}; using defaults",
                    config_path.display()
                );
                return Config::default();
            }
        };
        match self::unified::parse_unified(&contents) {
            Ok((config, config_errors, _widget_file, _widget_errors)) => {
                for err in &config_errors {
                    eprintln!("warning: config {err}");
                }
                config
            }
            Err(e) => {
                eprintln!(
                    "warning: config parse error in {}: {e}; using defaults",
                    config_path.display()
                );
                Config::default()
            }
        }
    }

    pub fn try_load() -> Result<Self> {
        Self::try_load_from_path(config_path())
    }

    pub fn try_load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let (config, _config_errors, _widget_file, _widget_errors) =
            self::unified::parse_unified(&contents)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = config_path();
        self.save_to_path(&path)?;
        Ok(path)
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        // Read existing file to preserve widget definitions and comments
        let existing = fs::read_to_string(path).unwrap_or_default();
        let contents = kdl_writer::patch_config_in_document(&existing, self)
            .map_err(|e| anyhow::anyhow!("KDL error: {e}"))
            .context("failed to serialize config")?;

        let temp_path = temp_config_path(path);
        fs::write(&temp_path, contents)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        Ok(())
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("kasane").join("kasane.kdl")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".config")
            .join("kasane")
            .join("kasane.kdl")
    } else {
        PathBuf::from("kasane.kdl")
    }
}

/// Legacy config.toml path (v0.4.0 and earlier).
///
/// Used only for migration detection. Kasane no longer reads TOML configs.
pub fn legacy_config_path() -> PathBuf {
    config_path().with_file_name("config.toml")
}

/// Detects an orphaned v0.4.0 config.toml and returns a warning message.
///
/// Returns `Some(msg)` if `config.toml` exists but `kasane.kdl` does not.
/// Kasane should print this to stderr and continue with defaults — the
/// user's old TOML config is silently ignored otherwise.
pub fn legacy_config_warning() -> Option<String> {
    legacy_config_warning_for_paths(&config_path(), &legacy_config_path())
}

fn legacy_config_warning_for_paths(kdl: &Path, toml: &Path) -> Option<String> {
    if kdl.exists() || !toml.exists() {
        return None;
    }
    Some(format!(
        "warning: found {toml} but {kdl} is missing.\n\
         \n  Kasane 0.5.0 uses KDL (kasane.kdl) instead of TOML (config.toml).\n  Your config.toml is being ignored.\n\
         \n  To migrate:\n    1. Run `kasane init` to generate a starter kasane.kdl\n    2. Port settings from config.toml by hand\n    3. Delete config.toml when done\n\
         \n  Migration guide: https://github.com/Yus314/kasane/blob/master/docs/config.md#migrating-from-v040",
        toml = toml.display(),
        kdl = kdl.display(),
    ))
}

fn temp_config_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("kasane.kdl");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    path.with_file_name(format!(".{file_name}.{pid}.{stamp}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.ui.shadow);
        assert_eq!(config.scroll.lines_per_scroll, 3);
        assert_eq!(config.log.level, "warn");
    }

    #[test]
    fn test_partial_kdl() {
        let kdl_str = r#"
scroll {
    lines_per_scroll 5
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.scroll.lines_per_scroll, 5);
        assert!(config.ui.shadow); // default preserved
    }

    #[test]
    fn test_new_config_sections() {
        let kdl_str = r#"
scroll {
    lines_per_scroll 5
    smooth #true
    inertia #true
}

clipboard {
    enabled #false
}

mouse {
    drag_scroll #false
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.scroll.lines_per_scroll, 5);
        assert!(config.scroll.smooth);
        assert!(config.scroll.inertia);
        assert!(!config.clipboard.enabled);
        assert!(!config.mouse.drag_scroll);
    }

    #[test]
    fn test_new_config_defaults() {
        let config = Config::default();
        assert!(!config.scroll.smooth);
        assert!(!config.scroll.inertia);
        assert!(config.clipboard.enabled);
        assert!(config.mouse.drag_scroll);
    }

    #[test]
    fn test_window_config_defaults() {
        let config = Config::default();
        assert_eq!(config.window.initial_cols, 80);
        assert_eq!(config.window.initial_rows, 24);
        assert!(!config.window.fullscreen);
        assert!(!config.window.maximized);
    }

    #[test]
    fn test_window_config_fullscreen() {
        let kdl_str = r#"
window {
    fullscreen #true
    maximized #true
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert!(config.window.fullscreen);
        assert!(config.window.maximized);
        assert_eq!(config.window.initial_cols, 80); // default preserved
    }

    #[test]
    fn test_font_config_defaults() {
        let config = Config::default();
        assert_eq!(config.font.family, "monospace");
        assert_eq!(config.font.size, 14.0);
        assert_eq!(config.font.line_height, 1.2);
        assert_eq!(config.font.letter_spacing, 0.0);
    }

    #[test]
    fn test_colors_config_defaults() {
        let config = Config::default();
        assert_eq!(config.colors.default_fg, "#d4d4d4");
        assert_eq!(config.colors.default_bg, "#1e1e1e");
        assert_eq!(config.colors.red, "#cd3131");
    }

    #[test]
    fn test_plugins_config_defaults() {
        let config = Config::default();
        assert!(config.plugins.path.is_none());
        assert!(config.plugins.enabled.is_empty());
        assert!(config.plugins.disabled.is_empty());
        assert!(config.plugins.selection.is_empty());
    }

    #[test]
    fn test_plugins_config_custom() {
        let kdl_str = r#"
plugins {
    path "/custom/plugins"
    enabled "cursor_line"
    disabled "line_numbers"
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.plugins.path.as_deref(), Some("/custom/plugins"));
        assert_eq!(config.plugins.enabled, vec!["cursor_line"]);
        assert_eq!(config.plugins.disabled, vec!["line_numbers"]);
    }

    #[test]
    fn test_plugins_selection_config() {
        let kdl_str = r#"
plugins {
    selection {
        sel_badge mode="pin-digest" digest="sha256:abc"
        cursor_line mode="pin-package" package="builtin/cursor-line" version="0.3.0"
    }
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(
            config.plugins.selection.get("sel_badge"),
            Some(&PluginSelection::PinDigest {
                digest: "sha256:abc".to_string()
            })
        );
        assert_eq!(
            config.plugins.selection.get("cursor_line"),
            Some(&PluginSelection::PinPackage {
                package: "builtin/cursor-line".to_string(),
                version: Some("0.3.0".to_string()),
            })
        );
        assert_eq!(
            config.plugins.selection_for("missing"),
            PluginSelection::Auto
        );
    }

    #[test]
    fn test_plugins_bundled_enabled_check() {
        let pc = PluginsConfig {
            enabled: vec!["cursor_line".to_string(), "sel_badge".to_string()],
            ..Default::default()
        };
        assert!(pc.is_bundled_enabled("cursor_line"));
        assert!(pc.is_bundled_enabled("sel_badge"));
        assert!(!pc.is_bundled_enabled("color_preview"));
        assert!(!pc.is_bundled_enabled("fuzzy_finder"));
    }

    #[test]
    fn test_plugins_deny_capabilities() {
        let kdl_str = r#"
plugins {
    disabled "some_plugin"

    deny_capabilities {
        untrusted_plugin "filesystem" "environment"
        another_plugin "monotonic-clock"
    }
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(
            config.plugins.deny_capabilities.get("untrusted_plugin"),
            Some(&vec!["filesystem".to_string(), "environment".to_string()])
        );
        assert_eq!(
            config.plugins.deny_capabilities.get("another_plugin"),
            Some(&vec!["monotonic-clock".to_string()])
        );
        assert!(config.plugins.deny_capabilities.get("missing").is_none());
    }

    #[test]
    fn test_plugins_deny_capabilities_default_empty() {
        let config = Config::default();
        assert!(config.plugins.deny_capabilities.is_empty());
    }

    #[test]
    fn test_plugins_deny_authorities() {
        let kdl_str = r#"
plugins {
    disabled "some_plugin"

    deny_authorities {
        untrusted_plugin "dynamic-surface"
        another_plugin "pty-process"
    }
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(
            config.plugins.deny_authorities.get("untrusted_plugin"),
            Some(&vec!["dynamic-surface".to_string()])
        );
        assert_eq!(
            config.plugins.deny_authorities.get("another_plugin"),
            Some(&vec!["pty-process".to_string()])
        );
        assert!(config.plugins.deny_authorities.get("missing").is_none());
    }

    #[test]
    fn test_plugins_deny_authorities_default_empty() {
        let config = Config::default();
        assert!(config.plugins.deny_authorities.is_empty());
    }

    #[test]
    fn test_plugins_dir_custom_path() {
        let pc = PluginsConfig {
            path: Some("/my/plugins".to_string()),
            ..Default::default()
        };
        assert_eq!(pc.plugins_dir(), std::path::PathBuf::from("/my/plugins"));
    }

    #[test]
    fn test_config_save_and_try_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config").join("kasane.kdl");
        let mut config = Config::default();
        config.plugins.selection.insert(
            "sel_badge".to_string(),
            PluginSelection::PinDigest {
                digest: "sha256:abc".to_string(),
            },
        );

        config.save_to_path(&path).unwrap();
        let loaded = Config::try_load_from_path(&path).unwrap();
        assert_eq!(
            loaded.plugins.selection.get("sel_badge"),
            Some(&PluginSelection::PinDigest {
                digest: "sha256:abc".to_string(),
            })
        );
    }

    #[test]
    fn test_partial_gui_config() {
        let kdl_str = r##"
window {
    initial_cols 120
}

font {
    size 16.0
    family "JetBrains Mono"
}

colors {
    default_bg "#282828"
}
"##;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.window.initial_cols, 120);
        assert_eq!(config.window.initial_rows, 24); // default
        assert_eq!(config.font.size, 16.0);
        assert_eq!(config.font.family, "JetBrains Mono");
        assert_eq!(config.font.line_height, 1.2); // default
        assert_eq!(config.colors.default_bg, "#282828");
        assert_eq!(config.colors.default_fg, "#d4d4d4"); // default
    }

    #[test]
    fn test_theme_config() {
        let kdl_str = r#"
theme {
    menu_item_normal "cyan,blue"
    info_border "white,default+b"
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.theme.faces.len(), 2);
        assert_eq!(
            config.theme.faces.get("menu_item_normal"),
            Some(&ThemeValue::FaceSpec("cyan,blue".to_string()))
        );
    }

    #[test]
    fn test_menu_position_enum() {
        let kdl_str = r#"
menu {
    position "above"
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.menu.position, MenuPosition::Above);
    }

    #[test]
    fn test_status_position_enum() {
        let kdl_str = r#"
ui {
    status_position "top"
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.ui.status_position, StatusPosition::Top);
    }

    #[test]
    fn test_border_style_enum() {
        let kdl_str = r#"
ui {
    border_style "double"
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.ui.border_style, BorderStyleConfig::Double);
    }

    #[test]
    fn test_image_protocol_enum() {
        let kdl_str = r#"
ui {
    image_protocol "kitty"
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert_eq!(config.ui.image_protocol, ImageProtocolConfig::Kitty);
    }

    #[test]
    fn test_invalid_enum_value_uses_default_and_reports_error() {
        let kdl_str = r#"
menu {
    position "invalid_position"
}
"#;
        let (config, config_errors, _, _) = unified::parse_unified(kdl_str).unwrap();
        // Unknown enum values fall back to default (lenient parsing)
        assert_eq!(config.menu.position, MenuPosition::Auto);
        // But also report an error
        assert_eq!(config_errors.len(), 1);
        assert_eq!(config_errors[0].section, "menu");
        assert_eq!(config_errors[0].field, "position");
        assert!(config_errors[0].message.contains("invalid_position"));
    }

    #[test]
    fn test_unknown_field_reports_error() {
        let kdl_str = r#"
ui {
    shadow #true
    nonexistent_field "hello"
}
"#;
        let (config, config_errors, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert!(config.ui.shadow);
        assert_eq!(config_errors.len(), 1);
        assert_eq!(config_errors[0].section, "ui");
        assert_eq!(config_errors[0].field, "nonexistent_field");
        assert!(config_errors[0].message.contains("unknown field"));
    }

    #[test]
    fn test_valid_config_has_no_errors() {
        let kdl_str = r#"
ui {
    shadow #false
    border_style "double"
}
scroll {
    smooth #true
}
"#;
        let (_config, config_errors, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert!(config_errors.is_empty());
    }

    #[test]
    fn test_syntax_error_rejects_file() {
        let kdl_str = "this is { not valid } kdl {{{";
        let result = unified::parse_unified(kdl_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_unified_config_and_widgets() {
        let kdl_str = r#"
ui {
    shadow #false
}

widgets {
    mode slot="status-left" text=" {editor_mode} " face="@status_mode"
    position slot="status-right" text=" {cursor_line}:{cursor_col} "
}
"#;
        let (config, _, widget_file, errors) = unified::parse_unified(kdl_str).unwrap();
        assert!(!config.ui.shadow);
        assert_eq!(widget_file.widgets.len(), 2);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_shorthand_form() {
        let kdl_str = r#"clipboard enabled=#false"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        assert!(!config.clipboard.enabled);
    }

    #[test]
    fn test_save_preserves_widgets() {
        let initial_kdl = r#"
ui {
    shadow #false
}

widgets {
    mode slot="status-left" text=" {editor_mode} "
}
"#;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("kasane.kdl");
        std::fs::write(&path, initial_kdl).unwrap();

        let mut config = Config::try_load_from_path(&path).unwrap();
        config.plugins.selection.insert(
            "test".to_string(),
            PluginSelection::PinDigest {
                digest: "sha256:test".to_string(),
            },
        );
        config.save_to_path(&path).unwrap();

        // Re-read and verify widgets are preserved
        let saved_source = std::fs::read_to_string(&path).unwrap();
        let (loaded_config, _, widget_file, _) = unified::parse_unified(&saved_source).unwrap();
        assert_eq!(
            loaded_config.plugins.selection.get("test"),
            Some(&PluginSelection::PinDigest {
                digest: "sha256:test".to_string(),
            })
        );
        assert_eq!(widget_file.widgets.len(), 1);
    }

    #[test]
    fn test_settings_parsing() {
        let kdl_str = r#"
settings {
    cursor_line {
        highlight_color "rgb:303030"
        blend_mode "replace"
        enabled #true
        intensity 42
    }
}
"#;
        let (config, _, _, _) = unified::parse_unified(kdl_str).unwrap();
        let cl = config.settings.get("cursor_line").unwrap();
        assert_eq!(
            cl.get("highlight_color"),
            Some(&SettingValue::Str(compact_str::CompactString::from(
                "rgb:303030"
            )))
        );
        assert_eq!(
            cl.get("blend_mode"),
            Some(&SettingValue::Str(compact_str::CompactString::from(
                "replace"
            )))
        );
        assert_eq!(cl.get("enabled"), Some(&SettingValue::Bool(true)));
        assert_eq!(cl.get("intensity"), Some(&SettingValue::Integer(42)));
    }

    #[test]
    fn test_restart_required_diff_detects_backend_change() {
        let old = Config::default();
        let mut new = Config::default();
        new.ui.backend = "gui".to_string();
        let diff = old.restart_required_diff(&new);
        assert!(diff.contains(&"ui.backend"));
    }

    #[test]
    fn test_restart_required_diff_empty_for_theme_change() {
        let old = Config::default();
        let mut new = Config::default();
        new.theme.faces.insert(
            "accent".to_string(),
            ThemeValue::FaceSpec("green".to_string()),
        );
        let diff = old.restart_required_diff(&new);
        assert!(diff.is_empty());
    }

    #[test]
    fn test_restart_required_diff_detects_font_change() {
        let old = Config::default();
        let mut new = Config::default();
        new.font.size = 20.0;
        let diff = old.restart_required_diff(&new);
        assert!(diff.contains(&"font"));
    }

    #[test]
    fn test_restart_required_diff_detects_multiple_changes() {
        let old = Config::default();
        let mut new = Config::default();
        new.ui.backend = "gui".to_string();
        new.log.level = "debug".to_string();
        let diff = old.restart_required_diff(&new);
        assert!(diff.contains(&"ui.backend"));
        assert!(diff.contains(&"log"));
    }

    #[test]
    fn legacy_warning_none_when_neither_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let kdl = tmp.path().join("kasane.kdl");
        let toml = tmp.path().join("config.toml");
        assert!(legacy_config_warning_for_paths(&kdl, &toml).is_none());
    }

    #[test]
    fn legacy_warning_none_when_kdl_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let kdl = tmp.path().join("kasane.kdl");
        let toml = tmp.path().join("config.toml");
        std::fs::write(&kdl, "").unwrap();
        std::fs::write(&toml, "[ui]\nshadow = false\n").unwrap();
        assert!(legacy_config_warning_for_paths(&kdl, &toml).is_none());
    }

    #[test]
    fn legacy_warning_some_when_only_toml_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let kdl = tmp.path().join("kasane.kdl");
        let toml = tmp.path().join("config.toml");
        std::fs::write(&toml, "[ui]\nshadow = false\n").unwrap();
        let msg = legacy_config_warning_for_paths(&kdl, &toml).unwrap();
        assert!(msg.contains("config.toml"));
        assert!(msg.contains("kasane.kdl"));
        assert!(msg.contains("kasane init"));
        assert!(msg.contains("Migration guide"));
    }

    /// Snapshot test for Config::default() serialized to KDL.
    /// If a field is added/removed or a default changes, this snapshot breaks,
    /// signaling that docs/config.md needs a corresponding update.
    #[test]
    fn config_defaults_snapshot() {
        let config = Config::default();
        let nodes = kdl_writer::config_to_kdl_nodes(&config);
        // Default config has all defaults so no nodes are emitted
        assert!(
            nodes.is_empty(),
            "default Config should produce no KDL nodes (all values are defaults)"
        );
    }

    #[test]
    fn test_effects_config_defaults() {
        let e = EffectsConfig::default();
        assert_eq!(e.gradient_start, None);
        assert_eq!(e.gradient_end, None);
        assert_eq!(e.cursor_line_highlight, CursorLineHighlightMode::Off);
        assert_eq!(e.overlay_transition_ms, 150);
        assert!(!e.backdrop_blur);
    }

    #[test]
    fn test_parse_effects_gradient() {
        let kdl = r##"effects {
            background-gradient {
                start "#1a1a2e"
                end "#16213e"
            }
            cursor-line-highlight "subtle"
            overlay-transition-ms 200
            backdrop-blur #true
        }"##;
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let (config, errors) = kdl_parser::parse_config_from_nodes(doc.nodes());
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let e = &config.effects;
        let start = e.gradient_start.unwrap();
        assert!((start[0] - 0x1a as f32 / 255.0).abs() < 0.01);
        assert!((start[1] - 0x1a as f32 / 255.0).abs() < 0.01);
        assert!((start[2] - 0x2e as f32 / 255.0).abs() < 0.01);

        let end = e.gradient_end.unwrap();
        assert!((end[0] - 0x16 as f32 / 255.0).abs() < 0.01);
        assert!((end[1] - 0x21 as f32 / 255.0).abs() < 0.01);
        assert!((end[2] - 0x3e as f32 / 255.0).abs() < 0.01);

        assert_eq!(e.cursor_line_highlight, CursorLineHighlightMode::Subtle);
        assert_eq!(e.overlay_transition_ms, 200);
        assert!(e.backdrop_blur);
    }

    #[test]
    fn test_parse_effects_empty() {
        let kdl = "effects {\n}";
        let doc: kdl::KdlDocument = kdl.parse().unwrap();
        let (config, errors) = kdl_parser::parse_config_from_nodes(doc.nodes());
        assert!(errors.is_empty());
        assert_eq!(config.effects, EffectsConfig::default());
    }
}
