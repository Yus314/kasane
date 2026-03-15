use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize, Default, Clone)]
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
}

/// Menu configuration.
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct MenuConfig {
    pub position: String,
    pub max_height: u16,
}

impl Default for MenuConfig {
    fn default() -> Self {
        MenuConfig {
            position: "auto".to_string(),
            max_height: 10,
        }
    }
}

/// Menu position: auto (default Kakoune behavior), above, or below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPosition {
    Auto,
    Above,
    Below,
}

impl MenuConfig {
    pub fn menu_position(&self) -> MenuPosition {
        match self.position.as_str() {
            "above" => MenuPosition::Above,
            "below" => MenuPosition::Below,
            _ => MenuPosition::Auto,
        }
    }
}

/// Search menu configuration.
#[derive(Debug, Deserialize, Clone, Default)]
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
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct ThemeConfig {
    #[serde(flatten)]
    pub faces: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct UiConfig {
    pub shadow: bool,
    pub padding_char: String,
    pub border_style: String,
    pub status_position: String,
    pub backend: String,
    /// Enable the scene-based GPU renderer (bypasses CellGrid). `None` = auto (true for GUI).
    pub scene_renderer: Option<bool>,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            shadow: true,
            padding_char: "~".to_string(),
            border_style: "rounded".to_string(),
            status_position: "bottom".to_string(),
            backend: "tui".to_string(),
            scene_renderer: None,
        }
    }
}

/// Status bar position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusPosition {
    Top,
    Bottom,
}

impl UiConfig {
    /// Parse the configured border line style.
    pub fn border_line_style(&self) -> crate::element::BorderLineStyle {
        match self.border_style.as_str() {
            "single" => crate::element::BorderLineStyle::Single,
            "rounded" => crate::element::BorderLineStyle::Rounded,
            "double" => crate::element::BorderLineStyle::Double,
            "heavy" => crate::element::BorderLineStyle::Heavy,
            "ascii" => crate::element::BorderLineStyle::Ascii,
            _ => crate::element::BorderLineStyle::Rounded,
        }
    }

    pub fn status_position(&self) -> StatusPosition {
        match self.status_position.as_str() {
            "top" => StatusPosition::Top,
            _ => StatusPosition::Bottom,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct MouseConfig {
    pub drag_scroll: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        MouseConfig { drag_scroll: true }
    }
}

#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct WindowConfig {
    pub initial_cols: u16,
    pub initial_rows: u16,
    pub fullscreen: bool,
    pub maximized: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            initial_cols: 80,
            initial_rows: 24,
            fullscreen: false,
            maximized: false,
        }
    }
}

/// Font configuration for the GUI backend.
#[derive(Debug, Deserialize, Clone)]
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
#[derive(Debug, Deserialize, Clone)]
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
            white: "#e5e5e5".to_string(),
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
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct PluginsConfig {
    /// Automatically discover .wasm plugins from the plugins directory.
    pub auto_discover: bool,
    /// Custom path to the plugins directory. Defaults to XDG_DATA_HOME/kasane/plugins/.
    pub path: Option<String>,
    /// Bundled plugin IDs to enable (opt-in). Bundled plugins are NOT loaded unless
    /// listed here. Available: "cursor_line", "color_preview", "sel_badge", "fuzzy_finder".
    pub enabled: Vec<String>,
    /// Plugin IDs to disable (by plugin ID, e.g. "cursor_line").
    /// Applies to filesystem-discovered and user-registered plugins.
    pub disabled: Vec<String>,
    /// Per-plugin capability denials. Key: plugin ID, Value: list of denied capability names.
    /// Valid capability names: "filesystem", "environment", "monotonic-clock".
    pub deny_capabilities: HashMap<String, Vec<String>>,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        PluginsConfig {
            auto_discover: true,
            path: None,
            enabled: Vec::new(),
            disabled: Vec::new(),
            deny_capabilities: HashMap::new(),
        }
    }
}

impl PluginsConfig {
    /// Check if a bundled plugin should be loaded (opt-in via `enabled` list).
    pub fn is_bundled_enabled(&self, id: &str) -> bool {
        self.enabled.iter().any(|s| s == id)
    }

    /// Resolve the plugins directory path.
    pub fn plugins_dir(&self) -> std::path::PathBuf {
        if let Some(ref p) = self.path {
            std::path::PathBuf::from(p)
        } else {
            dirs_data_path().join("plugins")
        }
    }
}

fn dirs_data_path() -> std::path::PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        std::path::PathBuf::from(xdg).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("kasane")
    } else {
        std::path::PathBuf::from("kasane-data")
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = dirs_config_path();
        match std::fs::read_to_string(&config_path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }
}

fn dirs_config_path() -> std::path::PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        std::path::PathBuf::from(xdg)
            .join("kasane")
            .join("config.toml")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("kasane")
            .join("config.toml")
    } else {
        std::path::PathBuf::from("config.toml")
    }
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
        assert!(config.plugins.auto_discover);
        assert!(config.plugins.path.is_none());
        assert!(config.plugins.enabled.is_empty());
        assert!(config.plugins.disabled.is_empty());
    }

    #[test]
    fn test_plugins_config_custom() {
        let toml_str = r#"
[plugins]
auto_discover = false
path = "/custom/plugins"
enabled = ["cursor_line"]
disabled = ["line_numbers"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.plugins.auto_discover);
        assert_eq!(config.plugins.path.as_deref(), Some("/custom/plugins"));
        assert_eq!(config.plugins.enabled, vec!["cursor_line"]);
        assert_eq!(config.plugins.disabled, vec!["line_numbers"]);
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
    fn test_plugins_dir_custom_path() {
        let pc = PluginsConfig {
            path: Some("/my/plugins".to_string()),
            ..Default::default()
        };
        assert_eq!(pc.plugins_dir(), std::path::PathBuf::from("/my/plugins"));
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
}
