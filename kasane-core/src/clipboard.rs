/// Platform clipboard abstraction.
///
/// Wraps `arboard::Clipboard` with graceful fallback when the clipboard
/// subsystem is unavailable (e.g. headless CI, Wayland without a compositor).
pub struct SystemClipboard {
    inner: Option<arboard::Clipboard>,
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClipboard {
    /// Create a clipboard handle.  Returns a working instance when the
    /// platform clipboard is available; silently degrades to a no-op otherwise.
    pub fn new() -> Self {
        Self {
            inner: arboard::Clipboard::new().ok(),
        }
    }

    /// Create a no-op clipboard (for tests / headless environments).
    pub fn noop() -> Self {
        Self { inner: None }
    }

    /// Read text from the system clipboard.
    pub fn get(&mut self) -> Option<String> {
        self.inner.as_mut()?.get_text().ok()
    }
}
