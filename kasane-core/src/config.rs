use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[serde(default)]
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
    /// Per-plugin typed settings: `[settings.<plugin_id>]` sections.
    #[serde(default)]
    pub settings: HashMap<String, toml::Table>,
}

/// Menu configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MenuPosition {
    #[default]
    Auto,
    Above,
    Below,
}

/// Search menu configuration.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(default)]
pub struct SearchConfig {
    /// When true, show search completions as a vertical dropdown instead of inline.
    pub dropdown: bool,
}

/// Theme configuration: maps style token names to face specifications.
///
/// Example in config.toml:
/// ```toml
/// [theme]
/// menu_item_normal = "white,blue"
/// menu_item_selected = "blue,white"
/// info_border = "cyan,default"
/// ```
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
#[serde(default)]
pub struct ThemeConfig {
    #[serde(flatten)]
    pub faces: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusPosition {
    Top,
    #[default]
    Bottom,
}

/// Border line style configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageProtocolConfig {
    #[default]
    Auto,
    Halfblock,
    Kitty,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct ClipboardConfig {
    pub enabled: bool,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        ClipboardConfig { enabled: true }
    }
}

/// Mouse configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct MouseConfig {
    pub drag_scroll: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        MouseConfig { drag_scroll: true }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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

/// Plugin configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
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

impl Default for PluginsConfig {
    fn default() -> Self {
        PluginsConfig {
            path: None,
            enabled: Vec::new(),
            disabled: Vec::new(),
            deny_capabilities: HashMap::new(),
            deny_authorities: HashMap::new(),
            selection: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Default)]
#[serde(tag = "mode", rename_all = "kebab-case")]
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
    pub fn load() -> Self {
        let config_path = config_path();
        match fs::read_to_string(&config_path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Config::default(),
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
        toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
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

        let contents = toml::to_string_pretty(self).context("failed to serialize config")?;
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
        PathBuf::from(xdg).join("kasane").join("config.toml")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".config")
            .join("kasane")
            .join("config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}

fn temp_config_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
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
    fn test_partial_toml() {
        let toml_str = r#"
[scroll]
lines_per_scroll = 5
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.scroll.lines_per_scroll, 5);
        assert!(config.ui.shadow); // default preserved
    }

    #[test]
    fn test_new_config_sections() {
        let toml_str = r#"
[scroll]
lines_per_scroll = 5
smooth = true
inertia = true

[clipboard]
enabled = false

[mouse]
drag_scroll = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
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
        let toml_str = r#"
[window]
fullscreen = true
maximized = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
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
        let toml_str = r#"
[plugins]
path = "/custom/plugins"
enabled = ["cursor_line"]
disabled = ["line_numbers"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.plugins.path.as_deref(), Some("/custom/plugins"));
        assert_eq!(config.plugins.enabled, vec!["cursor_line"]);
        assert_eq!(config.plugins.disabled, vec!["line_numbers"]);
    }

    #[test]
    fn test_plugins_selection_config() {
        let toml_str = r#"
[plugins.selection.sel_badge]
mode = "pin-digest"
digest = "sha256:abc"

[plugins.selection.cursor_line]
mode = "pin-package"
package = "builtin/cursor-line"
version = "0.3.0"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
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
        let toml_str = r#"
[plugins]
disabled = ["some_plugin"]

[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
another_plugin = ["monotonic-clock"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
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
        let toml_str = r#"
[plugins]
disabled = ["some_plugin"]

[plugins.deny_authorities]
untrusted_plugin = ["dynamic-surface"]
another_plugin = ["pty-process"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
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
        let path = tmp.path().join("config").join("config.toml");
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
        let toml_str = r##"
[window]
initial_cols = 120

[font]
size = 16.0
family = "JetBrains Mono"

[colors]
default_bg = "#282828"
"##;
        let config: Config = toml::from_str(toml_str).unwrap();
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
        let toml_str = r#"
[theme]
menu_item_normal = "cyan,blue"
info_border = "white,default+b"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.theme.faces.len(), 2);
        assert_eq!(
            config.theme.faces.get("menu_item_normal"),
            Some(&"cyan,blue".to_string())
        );
    }

    #[test]
    fn test_menu_position_enum() {
        let toml_str = r#"
[menu]
position = "above"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.menu.position, MenuPosition::Above);
    }

    #[test]
    fn test_status_position_enum() {
        let toml_str = r#"
[ui]
status_position = "top"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.status_position, StatusPosition::Top);
    }

    #[test]
    fn test_border_style_enum() {
        let toml_str = r#"
[ui]
border_style = "double"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.border_style, BorderStyleConfig::Double);
    }

    #[test]
    fn test_image_protocol_enum() {
        let toml_str = r#"
[ui]
image_protocol = "kitty"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.image_protocol, ImageProtocolConfig::Kitty);
    }

    #[test]
    fn test_invalid_enum_value_fails() {
        let toml_str = r#"
[menu]
position = "invalid_position"
"#;
        let result: std::result::Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_status_position_fails() {
        let toml_str = r#"
[ui]
status_position = "middle"
"#;
        let result: std::result::Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    /// Snapshot test for Config::default() serialized to TOML.
    /// If a field is added/removed or a default changes, this snapshot breaks,
    /// signaling that docs/config.md needs a corresponding update.
    #[test]
    fn config_defaults_snapshot() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).expect("Config must serialize to TOML");
        insta::assert_snapshot!("config_defaults", toml_str);
    }
}
