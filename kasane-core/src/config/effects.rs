//! GPU visual effects: gradients, cursor-line highlight, text post-processing.

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
