//! GUI color palette: concrete RGB fallback for `Color::Default`.

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
