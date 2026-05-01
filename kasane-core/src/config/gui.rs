//! GUI-backend-specific configuration: window dimensions and font selection.

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
