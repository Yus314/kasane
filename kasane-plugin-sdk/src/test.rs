//! Test harness for WASM plugins.
//!
//! Provides a mock host environment for unit-testing Kasane WASM plugins
//! without the full runtime. Enable via `features = ["test-harness"]`.
//!
//! # Architecture
//!
//! The harness uses thread-local storage to hold mock host state.
//! All `host_state::*`, `element_builder::*`, and `host_log::*` calls
//! read from / write to this thread-local. Because of this, tests that
//! use the harness must run serially (use `#[serial_test::serial]` or
//! similar if running tests in parallel).
//!
//! # Usage
//!
//! ```ignore
//! #[cfg(test)]
//! mod tests {
//!     use kasane_plugin_sdk::test::TestHarness;
//!
//!     #[test]
//!     fn my_plugin_shows_count() {
//!         let mut h = TestHarness::new();
//!         h.set_cursor_count(3);
//!         h.set_cursor_line(10);
//!         // ... call your plugin's handler functions ...
//!     }
//! }
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

// =============================================================================
// Mock host state
// =============================================================================

/// Mock host state holding all values that `host_state::*` functions return.
#[derive(Debug, Clone)]
pub struct MockHostState {
    // --- Cursor ---
    pub cursor_line: i32,
    pub cursor_col: i32,
    pub cursor_count: u32,
    pub cursor_mode: u8,
    pub editor_mode: u8,

    // --- Buffer ---
    pub line_count: u32,
    pub lines: Vec<String>,
    pub dirty_lines: Vec<bool>,
    pub buffer_file_path: Option<String>,

    // --- Screen ---
    pub cols: u16,
    pub rows: u16,
    pub focused: bool,

    // --- Status ---
    pub status_style: String,

    // --- Menu ---
    pub has_menu: bool,
    pub menu_items: Vec<Vec<MockAtom>>,
    pub menu_selected: i32,

    // --- Info ---
    pub has_info: bool,

    // --- Config ---
    pub config: HashMap<String, String>,
    pub ui_options: HashMap<String, String>,

    // --- Session ---
    pub session_count: u32,
    pub active_session_key: Option<String>,

    // --- Theme ---
    pub dark_background: bool,

    // --- Selection ---
    pub selection_count: u32,
}

/// A simplified atom for test purposes.
#[derive(Debug, Clone)]
pub struct MockAtom {
    pub contents: String,
}

impl Default for MockHostState {
    fn default() -> Self {
        Self {
            cursor_line: 1,
            cursor_col: 1,
            cursor_count: 1,
            cursor_mode: 0,
            editor_mode: 0,
            line_count: 1,
            lines: vec!["".to_string()],
            dirty_lines: vec![false],
            buffer_file_path: None,
            cols: 80,
            rows: 24,
            focused: true,
            status_style: "status".to_string(),
            has_menu: false,
            menu_items: vec![],
            menu_selected: -1,
            has_info: false,
            config: HashMap::new(),
            ui_options: HashMap::new(),
            session_count: 1,
            active_session_key: None,
            dark_background: true,
            selection_count: 1,
        }
    }
}

// =============================================================================
// Mock element arena
// =============================================================================

/// A mock element arena that tracks created elements.
#[derive(Debug, Default)]
pub struct MockElementArena {
    next_handle: u32,
    /// Map from handle to a debug description of the element.
    pub elements: HashMap<u32, String>,
}

impl MockElementArena {
    fn alloc(&mut self, description: String) -> u32 {
        let handle = self.next_handle;
        self.next_handle += 1;
        self.elements.insert(handle, description);
        handle
    }

    /// Number of elements created in this arena.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Whether the arena is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Get the debug description of an element by handle.
    pub fn get(&self, handle: u32) -> Option<&str> {
        self.elements.get(&handle).map(|s| s.as_str())
    }
}

// =============================================================================
// Mock log accumulator
// =============================================================================

/// Captured log message from a plugin.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: u8, // 0=debug, 1=info, 2=warn, 3=error
    pub message: String,
}

// =============================================================================
// Thread-local state
// =============================================================================

thread_local! {
    static MOCK_STATE: RefCell<MockHostState> = RefCell::new(MockHostState::default());
    static MOCK_ARENA: RefCell<MockElementArena> = RefCell::new(MockElementArena::default());
    static MOCK_LOGS: RefCell<Vec<LogEntry>> = RefCell::new(Vec::new());
}

// =============================================================================
// TestHarness
// =============================================================================

/// Test harness for Kasane WASM plugins.
///
/// Manages thread-local mock state. Create one per test, set up the desired
/// host state, then call your plugin's handler functions. The harness cleans
/// up on drop.
///
/// **Important**: Tests using `TestHarness` must not run in parallel on the
/// same thread (thread-local state is shared). Use `#[serial_test::serial]`
/// or similar.
pub struct TestHarness {
    _private: (),
}

impl TestHarness {
    /// Create a new test harness with default host state.
    pub fn new() -> Self {
        // Reset all thread-local state.
        MOCK_STATE.with(|s| *s.borrow_mut() = MockHostState::default());
        MOCK_ARENA.with(|a| *a.borrow_mut() = MockElementArena::default());
        MOCK_LOGS.with(|l| l.borrow_mut().clear());
        Self { _private: () }
    }

    // --- Cursor setters ---

    pub fn set_cursor_line(&mut self, line: i32) {
        MOCK_STATE.with(|s| s.borrow_mut().cursor_line = line);
    }

    pub fn set_cursor_col(&mut self, col: i32) {
        MOCK_STATE.with(|s| s.borrow_mut().cursor_col = col);
    }

    pub fn set_cursor_count(&mut self, count: u32) {
        MOCK_STATE.with(|s| s.borrow_mut().cursor_count = count);
    }

    pub fn set_cursor_mode(&mut self, mode: u8) {
        MOCK_STATE.with(|s| s.borrow_mut().cursor_mode = mode);
    }

    pub fn set_editor_mode(&mut self, mode: u8) {
        MOCK_STATE.with(|s| s.borrow_mut().editor_mode = mode);
    }

    // --- Buffer setters ---

    pub fn set_lines(&mut self, lines: Vec<String>) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.line_count = lines.len() as u32;
            state.dirty_lines = vec![false; lines.len()];
            state.lines = lines;
        });
    }

    pub fn set_line_count(&mut self, count: u32) {
        MOCK_STATE.with(|s| s.borrow_mut().line_count = count);
    }

    pub fn set_buffer_file_path(&mut self, path: Option<String>) {
        MOCK_STATE.with(|s| s.borrow_mut().buffer_file_path = path);
    }

    // --- Screen setters ---

    pub fn set_screen_size(&mut self, cols: u16, rows: u16) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.cols = cols;
            state.rows = rows;
        });
    }

    pub fn set_focused(&mut self, focused: bool) {
        MOCK_STATE.with(|s| s.borrow_mut().focused = focused);
    }

    // --- Menu setters ---

    pub fn set_menu(&mut self, items: Vec<Vec<String>>, selected: i32) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.has_menu = true;
            state.menu_items = items
                .into_iter()
                .map(|cols| cols.into_iter().map(|c| MockAtom { contents: c }).collect())
                .collect();
            state.menu_selected = selected;
        });
    }

    pub fn clear_menu(&mut self) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.has_menu = false;
            state.menu_items.clear();
            state.menu_selected = -1;
        });
    }

    // --- Config setters ---

    pub fn set_config(&mut self, key: impl Into<String>, value: impl Into<String>) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().config.insert(key.into(), value.into());
        });
    }

    pub fn set_ui_option(&mut self, key: impl Into<String>, value: impl Into<String>) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().ui_options.insert(key.into(), value.into());
        });
    }

    // --- Session setters ---

    pub fn set_session_count(&mut self, count: u32) {
        MOCK_STATE.with(|s| s.borrow_mut().session_count = count);
    }

    pub fn set_active_session_key(&mut self, key: Option<String>) {
        MOCK_STATE.with(|s| s.borrow_mut().active_session_key = key);
    }

    // --- Selection setters ---

    pub fn set_selection_count(&mut self, count: u32) {
        MOCK_STATE.with(|s| s.borrow_mut().selection_count = count);
    }

    // --- Theme setters ---

    pub fn set_dark_background(&mut self, dark: bool) {
        MOCK_STATE.with(|s| s.borrow_mut().dark_background = dark);
    }

    // --- Assertions ---

    /// Access the element arena to inspect created elements.
    pub fn arena(&self) -> MockElementArena {
        MOCK_ARENA.with(|a| {
            let arena = a.borrow();
            MockElementArena {
                next_handle: arena.next_handle,
                elements: arena.elements.clone(),
            }
        })
    }

    /// Drain collected log messages.
    pub fn drain_logs(&mut self) -> Vec<LogEntry> {
        MOCK_LOGS.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    /// Read the current mock host state.
    pub fn state(&self) -> MockHostState {
        MOCK_STATE.with(|s| s.borrow().clone())
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        // Reset thread-local state to prevent leaking between tests.
        MOCK_STATE.with(|s| *s.borrow_mut() = MockHostState::default());
        MOCK_ARENA.with(|a| *a.borrow_mut() = MockElementArena::default());
        MOCK_LOGS.with(|l| l.borrow_mut().clear());
    }
}

// =============================================================================
// Mock host function implementations
// =============================================================================

/// Mock implementations of `host_state::*` functions.
///
/// These are called by the generated WIT mock bindings (produced by
/// `kasane_generate!` when compiled on non-wasm targets with `test-harness`).
pub mod mock_host_state {
    use super::MOCK_STATE;

    pub fn get_cursor_line() -> i32 {
        MOCK_STATE.with(|s| s.borrow().cursor_line)
    }
    pub fn get_cursor_col() -> i32 {
        MOCK_STATE.with(|s| s.borrow().cursor_col)
    }
    pub fn get_line_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().line_count)
    }
    pub fn get_cols() -> u16 {
        MOCK_STATE.with(|s| s.borrow().cols)
    }
    pub fn get_rows() -> u16 {
        MOCK_STATE.with(|s| s.borrow().rows)
    }
    pub fn is_focused() -> bool {
        MOCK_STATE.with(|s| s.borrow().focused)
    }
    pub fn get_line_text(line: u32) -> Option<String> {
        MOCK_STATE.with(|s| {
            let state = s.borrow();
            state.lines.get(line as usize).cloned()
        })
    }
    pub fn is_line_dirty(line: u32) -> bool {
        MOCK_STATE.with(|s| {
            let state = s.borrow();
            state
                .dirty_lines
                .get(line as usize)
                .copied()
                .unwrap_or(false)
        })
    }
    pub fn get_cursor_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().cursor_count)
    }
    pub fn get_secondary_cursor_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().cursor_count.saturating_sub(1))
    }
    pub fn get_cursor_mode() -> u8 {
        MOCK_STATE.with(|s| s.borrow().cursor_mode)
    }
    pub fn get_editor_mode() -> u8 {
        MOCK_STATE.with(|s| s.borrow().editor_mode)
    }
    pub fn has_menu() -> bool {
        MOCK_STATE.with(|s| s.borrow().has_menu)
    }
    pub fn get_menu_item_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().menu_items.len() as u32)
    }
    pub fn get_menu_selected() -> i32 {
        MOCK_STATE.with(|s| s.borrow().menu_selected)
    }
    pub fn has_info() -> bool {
        MOCK_STATE.with(|s| s.borrow().has_info)
    }
    pub fn get_config_string(key: &str) -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().config.get(key).cloned())
    }
    pub fn get_ui_option(key: &str) -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().ui_options.get(key).cloned())
    }
    pub fn get_session_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().session_count)
    }
    pub fn get_active_session_key() -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().active_session_key.clone())
    }
    pub fn is_dark_background() -> bool {
        MOCK_STATE.with(|s| s.borrow().dark_background)
    }
    pub fn get_selection_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().selection_count)
    }
    pub fn get_buffer_file_path() -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().buffer_file_path.clone())
    }
    pub fn get_widget_columns() -> u16 {
        MOCK_STATE.with(|s| s.borrow().cols)
    }
}

/// Mock implementations of `element_builder::*` functions.
pub mod mock_element_builder {
    use super::MOCK_ARENA;

    pub fn create_text(content: &str, _face_desc: &str) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("text({content:?})")))
    }
    pub fn create_styled_line(desc: &str) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("styled_line({desc:?})")))
    }
    pub fn create_column(children: &[u32]) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("column({children:?})")))
    }
    pub fn create_row(children: &[u32]) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("row({children:?})")))
    }
    pub fn create_empty() -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc("empty".to_string()))
    }
    #[allow(unused_variables)]
    pub fn create_container(child: u32, border: bool, shadow: bool) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("container({child})")))
    }

    // Reset the arena (called by TestHarness::new)
    pub fn reset() {
        MOCK_ARENA.with(|a| *a.borrow_mut() = super::MockElementArena::default());
    }
}

/// Mock implementations of `host_log::*` functions.
pub mod mock_host_log {
    use super::{LogEntry, MOCK_LOGS};

    pub fn log_message(level: u8, message: &str) {
        MOCK_LOGS.with(|l| {
            l.borrow_mut().push(LogEntry {
                level,
                message: message.to_string(),
            });
        });
    }
}
