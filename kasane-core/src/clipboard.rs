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

    /// Write text to the system clipboard. Silently no-ops when the
    /// platform clipboard is unavailable; logs at debug level on
    /// transient failures (e.g. headless CI sessions where arboard
    /// initialised but `set_text` rejects the write).
    pub fn set(&mut self, text: &str) {
        let Some(inner) = self.inner.as_mut() else {
            return;
        };
        if let Err(err) = inner.set_text(text.to_string()) {
            tracing::debug!(?err, "system clipboard set_text failed");
        }
    }
}
