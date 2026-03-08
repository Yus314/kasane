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
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            shadow: true,
            padding_char: "~".to_string(),
            border_style: "rounded".to_string(),
            status_position: "bottom".to_string(),
            backend: "tui".to_string(),
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
