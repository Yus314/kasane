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
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig {
            shadow: true,
            padding_char: "~".to_string(),
            border_style: "rounded".to_string(),
            status_position: "bottom".to_string(),
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
}

impl Default for ScrollConfig {
    fn default() -> Self {
        ScrollConfig {
            lines_per_scroll: 3,
        }
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
