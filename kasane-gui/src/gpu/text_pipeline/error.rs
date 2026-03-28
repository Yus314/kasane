// Derived from glyphon 0.10.0 (https://github.com/grovesNL/glyphon)
// Original license: MIT OR Apache-2.0 OR Zlib

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

/// An error that occurred while preparing text for rendering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrepareError {
    AtlasFull,
}

impl Display for PrepareError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Prepare error: glyph texture atlas is full")
    }
}

impl Error for PrepareError {}

/// An error that occurred while rendering text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum RenderError {
    RemovedFromAtlas,
    ScreenResolutionChanged,
}

impl Display for RenderError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            RenderError::RemovedFromAtlas => {
                write!(
                    f,
                    "Render error: glyph no longer exists within the texture atlas"
                )
            }
            RenderError::ScreenResolutionChanged => write!(
                f,
                "Render error: screen resolution changed since last `prepare` call"
            ),
        }
    }
}

impl Error for RenderError {}
