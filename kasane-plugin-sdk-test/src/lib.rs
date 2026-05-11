//! Test harness for Kasane WASM plugins (mock host).
//!
//! Plugin authors enable this crate via `kasane-plugin-sdk`'s `test-harness`
//! feature; it then provides a mock host environment for unit-testing plugins
//! without a live wasmtime + Kakoune.
//!
//! # Where to find the API
//!
//! The macros emitted by `kasane_plugin_sdk::define_plugin!` (and friends)
//! route `host_state::*` / `element_builder::*` / `host_log::*` calls into
//! this crate when the host plugin crate is compiled with the `test-harness`
//! feature on a non-wasm target. Tests typically just use:
//!
//! ```ignore
//! use kasane_plugin_sdk::test::TestHarness;  // re-export of this crate
//! ```
//!
//! # Architecture
//!
//! Thread-local storage holds three pieces of state:
//! - `MockHostState` — what `host_state::*` queries return.
//! - `MockElementArena` — what `element_builder::*` constructors record.
//! - `MockLogs` + `CommandLog` — output from `host_log::*` and the command
//!   records pushed by plugin tests observing `Effects`.
//!
//! Because the state is thread-local, tests using `TestHarness` cannot run
//! in parallel on the same thread. Either run with `--test-threads=1`
//! globally, or use a serialization crate like `serial_test`.
//!
//! # Quick example
//!
//! ```ignore
//! #[cfg(all(test, feature = "test-harness"))]
//! mod tests {
//!     use kasane_plugin_sdk::test::TestHarness;
//!
//!     #[test]
//!     fn badge_shows_cursor_count() {
//!         let mut h = TestHarness::new();
//!         h.set_cursor_count(3);
//!         let badge = my_plugin::build_badge(h.cursor_count());
//!         assert!(badge.is_some());
//!     }
//! }
//! ```
//!
//! See `docs/plugin-testing.md` for the full cookbook.

use std::cell::RefCell;
use std::collections::HashMap;

// =============================================================================
// Mock host state
// =============================================================================

/// Mock host state holding every value `host_state::*` queries can return.
///
/// Values that the harness cannot fully reify (anything returning a WIT
/// `style`, `atom`, or `coord`) are stored as their mock equivalents
/// (`MockStyle`, `MockAtom`, `MockCoord`). Plugin-side shims translate
/// these into WIT types via SDK helpers when consumed.
#[derive(Debug, Clone)]
pub struct MockHostState {
    // --- Cursor ---
    pub cursor_line: i32,
    pub cursor_col: i32,
    pub cursor_count: u32,
    pub cursor_mode: u8,
    pub editor_mode: u8,
    pub secondary_cursors: Vec<MockCoord>,

    // --- Buffer ---
    pub line_count: u32,
    pub lines: Vec<String>,
    pub line_atoms: Vec<Vec<MockAtom>>,
    pub dirty_lines: Vec<bool>,
    pub buffer_file_path: Option<String>,

    // --- Screen ---
    pub cols: u16,
    pub rows: u16,
    pub focused: bool,
    pub is_dragging: bool,

    // --- Status ---
    pub status_style: String,
    pub status_prompt: Vec<MockAtom>,
    pub status_content: Vec<MockAtom>,
    pub status_line: Vec<MockAtom>,
    pub status_mode_line: Vec<MockAtom>,
    pub status_default_style: MockStyle,

    // --- Menu ---
    pub has_menu: bool,
    pub menu_items: Vec<Vec<MockAtom>>,
    pub menu_selected: i32,
    pub menu_anchor: Option<MockCoord>,
    pub menu_mode: Option<String>,
    pub menu_style: Option<MockStyle>,
    pub menu_selected_style: Option<MockStyle>,

    // --- Info ---
    pub has_info: bool,
    pub info_entries: Vec<MockInfo>,

    // --- Config / UI options ---
    pub config: HashMap<String, String>,
    pub ui_options: HashMap<String, String>,

    // --- Typed Settings ---
    pub settings_bool: HashMap<String, bool>,
    pub settings_integer: HashMap<String, i64>,
    pub settings_float: HashMap<String, f64>,
    pub settings_string: HashMap<String, String>,

    // --- Theme ---
    pub dark_background: bool,
    pub theme_styles: HashMap<String, MockStyle>,
    pub default_style: MockStyle,
    pub padding_style: MockStyle,

    // --- Session ---
    pub session_count: u32,
    pub active_session_key: Option<String>,
    pub active_session_name: Option<String>,
    pub sessions: Vec<MockSession>,

    // --- Display units (DU) ---
    pub display_unit_count: u32,

    // --- Syntax ---
    pub syntax_generation: u64,
    pub fold_ranges: Vec<(u32, u32)>,
    pub indent_levels: HashMap<u32, u32>,
    pub scopes_at: HashMap<(u32, u32), Vec<String>>,
}

/// Simplified atom for tests (matches WIT `atom` shape).
#[derive(Debug, Clone, Default)]
pub struct MockAtom {
    pub contents: String,
    pub style: MockStyle,
}

impl MockAtom {
    /// Construct an atom with the default style.
    pub fn plain(contents: impl Into<String>) -> Self {
        Self {
            contents: contents.into(),
            style: MockStyle::default(),
        }
    }
}

/// Brush representation mirroring WIT `brush`.
///
/// Plugin shims convert this to the WIT-generated `Brush` variant.
/// Covers `default-color` and `rgb(rgb-color)`; the `named(named-color)`
/// arm is not exercised by the harness yet — tests that depend on named
/// colors should use the explicit RGB equivalent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MockBrush {
    #[default]
    Default,
    Rgb { r: u8, g: u8, b: u8 },
}

/// Simplified style for tests (covers fg/bg/attributes — the parts plugin
/// logic typically inspects). Plugin shims convert to the WIT `style` shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MockStyle {
    pub fg: MockBrush,
    pub bg: MockBrush,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub dim: bool,
}

impl MockStyle {
    /// Style with only the background set to an RGB brush.
    pub fn bg_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            bg: MockBrush::Rgb { r, g, b },
            ..Self::default()
        }
    }
    /// Style with only the foreground set to an RGB brush.
    pub fn fg_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            fg: MockBrush::Rgb { r, g, b },
            ..Self::default()
        }
    }
}

/// 2D coordinate matching WIT `coord` shape (`{ line: s32, column: s32 }`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MockCoord {
    pub line: i32,
    pub column: i32,
}

/// Info popup snapshot — matches WIT info getters' per-entry shape.
#[derive(Debug, Clone, Default)]
pub struct MockInfo {
    pub title: Vec<MockAtom>,
    pub content: Vec<Vec<MockAtom>>,
    pub style: Option<String>,
    pub anchor: Option<MockCoord>,
}

/// Session descriptor — matches WIT `session-descriptor`.
#[derive(Debug, Clone, Default)]
pub struct MockSession {
    pub key: String,
    pub name: String,
}

impl Default for MockHostState {
    fn default() -> Self {
        Self {
            cursor_line: 1,
            cursor_col: 1,
            cursor_count: 1,
            cursor_mode: 0,
            editor_mode: 0,
            secondary_cursors: vec![],
            line_count: 1,
            lines: vec!["".to_string()],
            line_atoms: vec![],
            dirty_lines: vec![false],
            buffer_file_path: None,
            cols: 80,
            rows: 24,
            focused: true,
            is_dragging: false,
            status_style: "status".to_string(),
            status_prompt: vec![],
            status_content: vec![],
            status_line: vec![],
            status_mode_line: vec![],
            status_default_style: MockStyle::default(),
            has_menu: false,
            menu_items: vec![],
            menu_selected: -1,
            menu_anchor: None,
            menu_mode: None,
            menu_style: None,
            menu_selected_style: None,
            has_info: false,
            info_entries: vec![],
            config: HashMap::new(),
            ui_options: HashMap::new(),
            settings_bool: HashMap::new(),
            settings_integer: HashMap::new(),
            settings_float: HashMap::new(),
            settings_string: HashMap::new(),
            dark_background: true,
            theme_styles: HashMap::new(),
            default_style: MockStyle::default(),
            padding_style: MockStyle::default(),
            session_count: 1,
            active_session_key: None,
            active_session_name: None,
            sessions: vec![],
            display_unit_count: 0,
            syntax_generation: 0,
            fold_ranges: vec![],
            indent_levels: HashMap::new(),
            scopes_at: HashMap::new(),
        }
    }
}

// =============================================================================
// Mock element arena
// =============================================================================

/// A mock element arena that tracks created elements as debug strings.
#[derive(Debug, Default, Clone)]
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

    /// Number of elements created.
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

    /// Find handles whose description contains `needle`.
    pub fn find(&self, needle: &str) -> Vec<u32> {
        let mut matches: Vec<u32> = self
            .elements
            .iter()
            .filter(|(_, d)| d.contains(needle))
            .map(|(h, _)| *h)
            .collect();
        matches.sort_unstable();
        matches
    }
}

// =============================================================================
// Logs & Command observation
// =============================================================================

/// Captured log message from `host_log::*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// 0=debug, 1=info, 2=warn, 3=error (matches WIT `log-level` ordinals).
    pub level: u8,
    pub message: String,
}

/// A command recorded by a plugin handler's returned Effects.
///
/// The harness doesn't know about WIT-generated `Command` variants directly,
/// so plugin tests record commands via [`TestHarness::push_command`]. Each
/// entry stores a `kind` string identifying the variant (e.g. `"EvalCommand"`,
/// `"SendKeys"`, `"PasteClipboard"`) plus the variant payload as a string.
///
/// For convenience, the SDK ships `Effects → Vec<CommandRecord>` translation
/// in `kasane_plugin_sdk::test::record_effects!` (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    pub kind: String,
    pub payload: String,
}

impl CommandRecord {
    /// Shortcut for `EvalCommand` records.
    pub fn eval(cmd: impl Into<String>) -> Self {
        Self {
            kind: "EvalCommand".into(),
            payload: cmd.into(),
        }
    }
    /// Shortcut for `SendKeys` records (joined keys).
    pub fn send_keys(keys: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let joined: Vec<String> = keys.into_iter().map(Into::into).collect();
        Self {
            kind: "SendKeys".into(),
            payload: joined.join(" "),
        }
    }
}

// =============================================================================
// Thread-local state
// =============================================================================

thread_local! {
    static MOCK_STATE: RefCell<MockHostState> = RefCell::new(MockHostState::default());
    static MOCK_ARENA: RefCell<MockElementArena> = RefCell::new(MockElementArena::default());
    static MOCK_LOGS: RefCell<Vec<LogEntry>> = const { RefCell::new(Vec::new()) };
    static MOCK_COMMANDS: RefCell<Vec<CommandRecord>> = const { RefCell::new(Vec::new()) };
}

// =============================================================================
// TestHarness
// =============================================================================

/// Test harness for Kasane WASM plugins.
///
/// Manages thread-local mock state, element arena, log capture, and command
/// observation. Create one per test, configure the host state via the
/// setters, invoke plugin handler functions, then drain logs/commands to
/// assert.
///
/// **Important**: Tests using `TestHarness` must not run in parallel on the
/// same thread (thread-local state is shared). Use `--test-threads=1` or a
/// serialization crate.
pub struct TestHarness {
    _private: (),
}

impl TestHarness {
    /// Create a new test harness with default host state.
    ///
    /// Resets all thread-local mock state, the arena, logs, and command
    /// records. The previous harness's state is overwritten.
    pub fn new() -> Self {
        MOCK_STATE.with(|s| *s.borrow_mut() = MockHostState::default());
        MOCK_ARENA.with(|a| *a.borrow_mut() = MockElementArena::default());
        MOCK_LOGS.with(|l| l.borrow_mut().clear());
        MOCK_COMMANDS.with(|c| c.borrow_mut().clear());
        Self { _private: () }
    }

    // -------------------------------------------------------------------------
    // Cursor setters
    // -------------------------------------------------------------------------

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
    pub fn set_secondary_cursors(&mut self, cursors: Vec<MockCoord>) {
        MOCK_STATE.with(|s| s.borrow_mut().secondary_cursors = cursors);
    }

    // -------------------------------------------------------------------------
    // Buffer setters
    // -------------------------------------------------------------------------

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
    pub fn set_line_atoms(&mut self, atoms: Vec<Vec<MockAtom>>) {
        MOCK_STATE.with(|s| s.borrow_mut().line_atoms = atoms);
    }
    pub fn set_buffer_file_path(&mut self, path: Option<String>) {
        MOCK_STATE.with(|s| s.borrow_mut().buffer_file_path = path);
    }

    // -------------------------------------------------------------------------
    // Screen setters
    // -------------------------------------------------------------------------

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
    pub fn set_dragging(&mut self, dragging: bool) {
        MOCK_STATE.with(|s| s.borrow_mut().is_dragging = dragging);
    }

    // -------------------------------------------------------------------------
    // Status setters
    // -------------------------------------------------------------------------

    pub fn set_status_prompt(&mut self, atoms: Vec<MockAtom>) {
        MOCK_STATE.with(|s| s.borrow_mut().status_prompt = atoms);
    }
    pub fn set_status_content(&mut self, atoms: Vec<MockAtom>) {
        MOCK_STATE.with(|s| s.borrow_mut().status_content = atoms);
    }
    pub fn set_status_line(&mut self, atoms: Vec<MockAtom>) {
        MOCK_STATE.with(|s| s.borrow_mut().status_line = atoms);
    }
    pub fn set_status_mode_line(&mut self, atoms: Vec<MockAtom>) {
        MOCK_STATE.with(|s| s.borrow_mut().status_mode_line = atoms);
    }
    pub fn set_status_default_style(&mut self, style: MockStyle) {
        MOCK_STATE.with(|s| s.borrow_mut().status_default_style = style);
    }

    // -------------------------------------------------------------------------
    // Menu setters
    // -------------------------------------------------------------------------

    pub fn set_menu(&mut self, items: Vec<Vec<String>>, selected: i32) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.has_menu = true;
            state.menu_items = items
                .into_iter()
                .map(|cols| cols.into_iter().map(MockAtom::plain).collect())
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
            state.menu_anchor = None;
            state.menu_mode = None;
        });
    }
    pub fn set_menu_anchor(&mut self, anchor: Option<MockCoord>) {
        MOCK_STATE.with(|s| s.borrow_mut().menu_anchor = anchor);
    }
    pub fn set_menu_mode(&mut self, mode: Option<String>) {
        MOCK_STATE.with(|s| s.borrow_mut().menu_mode = mode);
    }
    pub fn set_menu_style(&mut self, style: Option<MockStyle>) {
        MOCK_STATE.with(|s| s.borrow_mut().menu_style = style);
    }
    pub fn set_menu_selected_style(&mut self, style: Option<MockStyle>) {
        MOCK_STATE.with(|s| s.borrow_mut().menu_selected_style = style);
    }

    // -------------------------------------------------------------------------
    // Info setters
    // -------------------------------------------------------------------------

    pub fn set_info(&mut self, entries: Vec<MockInfo>) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.has_info = !entries.is_empty();
            state.info_entries = entries;
        });
    }
    pub fn clear_info(&mut self) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.has_info = false;
            state.info_entries.clear();
        });
    }

    // -------------------------------------------------------------------------
    // Config / UI options setters
    // -------------------------------------------------------------------------

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

    // -------------------------------------------------------------------------
    // Typed Settings setters
    // -------------------------------------------------------------------------

    pub fn set_setting_bool(&mut self, key: impl Into<String>, value: bool) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().settings_bool.insert(key.into(), value);
        });
    }
    pub fn set_setting_integer(&mut self, key: impl Into<String>, value: i64) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().settings_integer.insert(key.into(), value);
        });
    }
    pub fn set_setting_float(&mut self, key: impl Into<String>, value: f64) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().settings_float.insert(key.into(), value);
        });
    }
    pub fn set_setting_string(&mut self, key: impl Into<String>, value: impl Into<String>) {
        MOCK_STATE.with(|s| {
            s.borrow_mut()
                .settings_string
                .insert(key.into(), value.into());
        });
    }

    // -------------------------------------------------------------------------
    // Theme setters
    // -------------------------------------------------------------------------

    pub fn set_dark_background(&mut self, dark: bool) {
        MOCK_STATE.with(|s| s.borrow_mut().dark_background = dark);
    }
    pub fn set_theme_style(&mut self, token: impl Into<String>, style: MockStyle) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().theme_styles.insert(token.into(), style);
        });
    }
    pub fn set_default_style(&mut self, style: MockStyle) {
        MOCK_STATE.with(|s| s.borrow_mut().default_style = style);
    }
    pub fn set_padding_style(&mut self, style: MockStyle) {
        MOCK_STATE.with(|s| s.borrow_mut().padding_style = style);
    }

    // -------------------------------------------------------------------------
    // Session setters
    // -------------------------------------------------------------------------

    pub fn set_session_count(&mut self, count: u32) {
        MOCK_STATE.with(|s| s.borrow_mut().session_count = count);
    }
    pub fn set_active_session_key(&mut self, key: Option<String>) {
        MOCK_STATE.with(|s| s.borrow_mut().active_session_key = key);
    }
    pub fn set_active_session_name(&mut self, name: Option<String>) {
        MOCK_STATE.with(|s| s.borrow_mut().active_session_name = name);
    }
    pub fn set_sessions(&mut self, sessions: Vec<MockSession>) {
        MOCK_STATE.with(|s| {
            let mut state = s.borrow_mut();
            state.session_count = sessions.len() as u32;
            state.sessions = sessions;
        });
    }

    // -------------------------------------------------------------------------
    // Display unit / syntax setters
    // -------------------------------------------------------------------------

    pub fn set_display_unit_count(&mut self, count: u32) {
        MOCK_STATE.with(|s| s.borrow_mut().display_unit_count = count);
    }
    pub fn set_syntax_generation(&mut self, generation: u64) {
        MOCK_STATE.with(|s| s.borrow_mut().syntax_generation = generation);
    }
    pub fn set_fold_ranges(&mut self, ranges: Vec<(u32, u32)>) {
        MOCK_STATE.with(|s| s.borrow_mut().fold_ranges = ranges);
    }
    pub fn set_indent_level(&mut self, line: u32, level: u32) {
        MOCK_STATE.with(|s| {
            s.borrow_mut().indent_levels.insert(line, level);
        });
    }
    pub fn set_scopes_at(&mut self, line: u32, byte_offset: u32, scopes: Vec<String>) {
        MOCK_STATE.with(|s| {
            s.borrow_mut()
                .scopes_at
                .insert((line, byte_offset), scopes);
        });
    }

    // -------------------------------------------------------------------------
    // Observation: arena, logs, commands
    // -------------------------------------------------------------------------

    /// Snapshot the element arena.
    pub fn arena(&self) -> MockElementArena {
        MOCK_ARENA.with(|a| a.borrow().clone())
    }

    /// Drain captured log messages.
    pub fn drain_logs(&mut self) -> Vec<LogEntry> {
        MOCK_LOGS.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    /// Drain captured command records.
    pub fn drain_commands(&mut self) -> Vec<CommandRecord> {
        MOCK_COMMANDS.with(|c| std::mem::take(&mut *c.borrow_mut()))
    }

    /// Record a command observed from a plugin handler's `Effects`.
    ///
    /// Plugins typically call this through the `record_effects!` macro
    /// rather than directly.
    pub fn push_command(&mut self, record: CommandRecord) {
        MOCK_COMMANDS.with(|c| c.borrow_mut().push(record));
    }

    /// Read the current mock host state.
    pub fn state(&self) -> MockHostState {
        MOCK_STATE.with(|s| s.borrow().clone())
    }

    // -------------------------------------------------------------------------
    // Read-only conveniences (mirror common host_state::* getters)
    // -------------------------------------------------------------------------

    pub fn cursor_line(&self) -> i32 {
        MOCK_STATE.with(|s| s.borrow().cursor_line)
    }
    pub fn cursor_count(&self) -> u32 {
        MOCK_STATE.with(|s| s.borrow().cursor_count)
    }
    pub fn cols(&self) -> u16 {
        MOCK_STATE.with(|s| s.borrow().cols)
    }
    pub fn rows(&self) -> u16 {
        MOCK_STATE.with(|s| s.borrow().rows)
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        MOCK_STATE.with(|s| *s.borrow_mut() = MockHostState::default());
        MOCK_ARENA.with(|a| *a.borrow_mut() = MockElementArena::default());
        MOCK_LOGS.with(|l| l.borrow_mut().clear());
        MOCK_COMMANDS.with(|c| c.borrow_mut().clear());
    }
}

// =============================================================================
// Mock host function implementations
// =============================================================================

/// Mock implementations of `host_state::*` functions.
///
/// Plugin-side macro shims (emitted when compiled natively with
/// `feature = "test-harness"`) route `host_state::foo()` calls here.
/// Complex-return functions (those returning `Style`, `Atom`, `Coord`)
/// surface `MockStyle` / `MockAtom` / `MockCoord` — the plugin shim is
/// responsible for translating into the WIT-generated type.
pub mod mock_host_state {
    use super::{MOCK_STATE, MockAtom, MockCoord, MockInfo, MockSession, MockStyle};

    // --- Cursor ---
    pub fn get_cursor_line() -> i32 {
        MOCK_STATE.with(|s| s.borrow().cursor_line)
    }
    pub fn get_cursor_col() -> i32 {
        MOCK_STATE.with(|s| s.borrow().cursor_col)
    }
    pub fn get_cursor_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().cursor_count)
    }
    pub fn get_secondary_cursor_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().secondary_cursors.len() as u32)
    }
    pub fn get_secondary_cursor(idx: u32) -> Option<MockCoord> {
        MOCK_STATE.with(|s| s.borrow().secondary_cursors.get(idx as usize).copied())
    }
    pub fn get_all_secondary_cursors() -> Vec<MockCoord> {
        MOCK_STATE.with(|s| s.borrow().secondary_cursors.clone())
    }
    pub fn get_cursor_mode() -> u8 {
        MOCK_STATE.with(|s| s.borrow().cursor_mode)
    }
    pub fn get_editor_mode() -> u8 {
        MOCK_STATE.with(|s| s.borrow().editor_mode)
    }

    // --- Buffer ---
    pub fn get_line_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().line_count)
    }
    pub fn get_line_text(line: u32) -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().lines.get(line as usize).cloned())
    }
    pub fn get_lines_text(start: u32, end: u32) -> Vec<String> {
        MOCK_STATE.with(|s| {
            let state = s.borrow();
            let start = start as usize;
            let end = (end as usize).min(state.lines.len());
            if start >= end {
                return vec![];
            }
            state.lines[start..end].to_vec()
        })
    }
    pub fn get_lines_atoms(start: u32, end: u32) -> Vec<Vec<MockAtom>> {
        MOCK_STATE.with(|s| {
            let state = s.borrow();
            let start = start as usize;
            let end = (end as usize).min(state.line_atoms.len());
            if start >= end {
                return vec![];
            }
            state.line_atoms[start..end].to_vec()
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
    pub fn get_buffer_file_path() -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().buffer_file_path.clone())
    }

    // --- Screen ---
    pub fn get_cols() -> u16 {
        MOCK_STATE.with(|s| s.borrow().cols)
    }
    pub fn get_rows() -> u16 {
        MOCK_STATE.with(|s| s.borrow().rows)
    }
    pub fn get_widget_columns() -> u16 {
        MOCK_STATE.with(|s| s.borrow().cols)
    }
    pub fn is_focused() -> bool {
        MOCK_STATE.with(|s| s.borrow().focused)
    }
    pub fn is_dragging() -> bool {
        MOCK_STATE.with(|s| s.borrow().is_dragging)
    }

    // --- Status ---
    pub fn get_status_prompt() -> Vec<MockAtom> {
        MOCK_STATE.with(|s| s.borrow().status_prompt.clone())
    }
    pub fn get_status_content() -> Vec<MockAtom> {
        MOCK_STATE.with(|s| s.borrow().status_content.clone())
    }
    pub fn get_status_line() -> Vec<MockAtom> {
        MOCK_STATE.with(|s| s.borrow().status_line.clone())
    }
    pub fn get_status_mode_line() -> Vec<MockAtom> {
        MOCK_STATE.with(|s| s.borrow().status_mode_line.clone())
    }
    pub fn get_status_default_style() -> MockStyle {
        MOCK_STATE.with(|s| s.borrow().status_default_style.clone())
    }
    pub fn get_status_style() -> String {
        MOCK_STATE.with(|s| s.borrow().status_style.clone())
    }

    // --- Menu ---
    pub fn has_menu() -> bool {
        MOCK_STATE.with(|s| s.borrow().has_menu)
    }
    pub fn get_menu_item_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().menu_items.len() as u32)
    }
    pub fn get_menu_item(idx: u32) -> Option<Vec<MockAtom>> {
        MOCK_STATE.with(|s| s.borrow().menu_items.get(idx as usize).cloned())
    }
    pub fn get_menu_selected() -> i32 {
        MOCK_STATE.with(|s| s.borrow().menu_selected)
    }
    pub fn get_menu_anchor() -> Option<MockCoord> {
        MOCK_STATE.with(|s| s.borrow().menu_anchor)
    }
    pub fn get_menu_mode() -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().menu_mode.clone())
    }
    pub fn get_menu_style() -> Option<MockStyle> {
        MOCK_STATE.with(|s| s.borrow().menu_style.clone())
    }
    pub fn get_menu_selected_style() -> Option<MockStyle> {
        MOCK_STATE.with(|s| s.borrow().menu_selected_style.clone())
    }

    // --- Info ---
    pub fn has_info() -> bool {
        MOCK_STATE.with(|s| s.borrow().has_info)
    }
    pub fn get_info_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().info_entries.len() as u32)
    }
    pub fn get_info_title(idx: u32) -> Option<Vec<MockAtom>> {
        MOCK_STATE.with(|s| {
            s.borrow()
                .info_entries
                .get(idx as usize)
                .map(|i: &MockInfo| i.title.clone())
        })
    }
    pub fn get_info_content(idx: u32) -> Option<Vec<Vec<MockAtom>>> {
        MOCK_STATE.with(|s| {
            s.borrow()
                .info_entries
                .get(idx as usize)
                .map(|i| i.content.clone())
        })
    }
    pub fn get_info_style(idx: u32) -> Option<String> {
        MOCK_STATE.with(|s| {
            s.borrow()
                .info_entries
                .get(idx as usize)
                .and_then(|i| i.style.clone())
        })
    }
    pub fn get_info_anchor(idx: u32) -> Option<MockCoord> {
        MOCK_STATE.with(|s| {
            s.borrow()
                .info_entries
                .get(idx as usize)
                .and_then(|i| i.anchor)
        })
    }

    // --- Config / UI options ---
    pub fn get_config_string(key: &str) -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().config.get(key).cloned())
    }
    pub fn get_ui_option(key: &str) -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().ui_options.get(key).cloned())
    }

    // --- Typed settings ---
    pub fn get_setting_bool(key: &str) -> Option<bool> {
        MOCK_STATE.with(|s| s.borrow().settings_bool.get(key).copied())
    }
    pub fn get_setting_integer(key: &str) -> Option<i64> {
        MOCK_STATE.with(|s| s.borrow().settings_integer.get(key).copied())
    }
    pub fn get_setting_float(key: &str) -> Option<f64> {
        MOCK_STATE.with(|s| s.borrow().settings_float.get(key).copied())
    }
    pub fn get_setting_string(key: &str) -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().settings_string.get(key).cloned())
    }

    // --- Theme ---
    pub fn is_dark_background() -> bool {
        MOCK_STATE.with(|s| s.borrow().dark_background)
    }
    pub fn get_theme_style(token: &str) -> Option<MockStyle> {
        MOCK_STATE.with(|s| s.borrow().theme_styles.get(token).cloned())
    }
    pub fn get_default_style() -> MockStyle {
        MOCK_STATE.with(|s| s.borrow().default_style.clone())
    }
    pub fn get_padding_style() -> MockStyle {
        MOCK_STATE.with(|s| s.borrow().padding_style.clone())
    }

    // --- Session ---
    pub fn get_session_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().session_count)
    }
    pub fn get_session(idx: u32) -> Option<MockSession> {
        MOCK_STATE.with(|s| s.borrow().sessions.get(idx as usize).cloned())
    }
    pub fn get_active_session_key() -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().active_session_key.clone())
    }
    pub fn get_active_session_name() -> Option<String> {
        MOCK_STATE.with(|s| s.borrow().active_session_name.clone())
    }

    // --- Display units ---
    pub fn get_display_unit_count() -> u32 {
        MOCK_STATE.with(|s| s.borrow().display_unit_count)
    }

    // --- Syntax ---
    pub fn get_syntax_generation() -> u64 {
        MOCK_STATE.with(|s| s.borrow().syntax_generation)
    }
    pub fn get_fold_ranges() -> Vec<(u32, u32)> {
        MOCK_STATE.with(|s| s.borrow().fold_ranges.clone())
    }
    pub fn get_scopes_at(line: u32, byte_offset: u32) -> Vec<String> {
        MOCK_STATE.with(|s| {
            s.borrow()
                .scopes_at
                .get(&(line, byte_offset))
                .cloned()
                .unwrap_or_default()
        })
    }
    pub fn get_indent_level(line: u32) -> u32 {
        MOCK_STATE.with(|s| {
            s.borrow()
                .indent_levels
                .get(&line)
                .copied()
                .unwrap_or(0)
        })
    }
}

/// Mock implementations of `element_builder::*` functions.
///
/// Each call records a debug description in the arena and returns a fresh
/// handle. The arena can be snapshotted via [`TestHarness::arena`].
pub mod mock_element_builder {
    use super::MOCK_ARENA;

    pub fn create_text(content: &str, _style_desc: &str) -> u32 {
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
    pub fn create_container(child: u32) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("container({child})")))
    }
    pub fn create_grid(columns: usize, children: &[u32]) -> u32 {
        MOCK_ARENA.with(|a| {
            a.borrow_mut()
                .alloc(format!("grid(cols={columns}, {children:?})"))
        })
    }
    pub fn create_column_flex(children: &[u32]) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("column_flex({children:?})")))
    }
    pub fn create_row_flex(children: &[u32]) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("row_flex({children:?})")))
    }
    pub fn create_scrollable(child: u32, offset: u16, vertical: bool) -> u32 {
        MOCK_ARENA.with(|a| {
            a.borrow_mut()
                .alloc(format!("scrollable({child}, off={offset}, v={vertical})"))
        })
    }
    pub fn create_stack(base: u32) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("stack({base})")))
    }
    pub fn create_interactive(child: u32, id: u32) -> u32 {
        MOCK_ARENA.with(|a| {
            a.borrow_mut()
                .alloc(format!("interactive({child}, id={id})"))
        })
    }
    pub fn create_image(w: u16, h: u16) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("image({w}x{h})")))
    }
    pub fn create_text_panel(lines: usize) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("text_panel({lines}L)")))
    }
    pub fn create_canvas(w: u16, h: u16, ops: usize) -> u32 {
        MOCK_ARENA.with(|a| {
            a.borrow_mut()
                .alloc(format!("canvas({w}x{h}, {ops} ops)"))
        })
    }
    pub fn create_slot_placeholder(slot: &str) -> u32 {
        MOCK_ARENA.with(|a| a.borrow_mut().alloc(format!("slot({slot})")))
    }

    /// Reset the arena. Called by `TestHarness::new`.
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

/// Push a [`CommandRecord`] from plugin test code.
///
/// Used by the `record_effects!` macro internally; can also be called
/// directly by tests that observe effects through a custom path.
pub fn push_command(record: CommandRecord) {
    MOCK_COMMANDS.with(|c| c.borrow_mut().push(record));
}

// =============================================================================
// Self-tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_resets_on_new() {
        {
            let mut h = TestHarness::new();
            h.set_cursor_line(99);
            mock_element_builder::create_text("temp", "default");
            mock_host_log::log_message(2, "warn");
            push_command(CommandRecord::eval("write"));
        }
        // New harness wipes everything.
        let mut h = TestHarness::new();
        assert_eq!(h.cursor_line(), 1);
        assert!(h.arena().is_empty());
        assert!(h.drain_logs().is_empty());
        assert!(h.drain_commands().is_empty());
    }

    #[test]
    fn cursor_setters_round_trip() {
        let mut h = TestHarness::new();
        h.set_cursor_line(42);
        h.set_cursor_col(10);
        h.set_cursor_count(3);
        h.set_secondary_cursors(vec![MockCoord { line: 5, column: 2 }]);
        assert_eq!(mock_host_state::get_cursor_line(), 42);
        assert_eq!(mock_host_state::get_cursor_col(), 10);
        assert_eq!(mock_host_state::get_cursor_count(), 3);
        assert_eq!(mock_host_state::get_secondary_cursor_count(), 1);
        assert_eq!(
            mock_host_state::get_secondary_cursor(0),
            Some(MockCoord { line: 5, column: 2 })
        );
    }

    #[test]
    fn buffer_lines_text_and_atoms() {
        let mut h = TestHarness::new();
        h.set_lines(vec!["abc".into(), "def".into(), "ghi".into()]);
        assert_eq!(mock_host_state::get_line_count(), 3);
        assert_eq!(mock_host_state::get_line_text(1), Some("def".into()));
        assert_eq!(
            mock_host_state::get_lines_text(0, 2),
            vec!["abc".to_string(), "def".to_string()]
        );
        // Out-of-range clamps.
        assert_eq!(mock_host_state::get_lines_text(2, 100).len(), 1);
        assert!(mock_host_state::get_lines_text(5, 10).is_empty());

        h.set_line_atoms(vec![vec![MockAtom::plain("hello")]]);
        let atoms = mock_host_state::get_lines_atoms(0, 1);
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0][0].contents, "hello");
    }

    #[test]
    fn info_entries_indexed() {
        let mut h = TestHarness::new();
        h.set_info(vec![
            MockInfo {
                title: vec![MockAtom::plain("title-0")],
                content: vec![vec![MockAtom::plain("body")]],
                style: Some("info".into()),
                anchor: Some(MockCoord { line: 1, column: 0 }),
            },
            MockInfo {
                title: vec![MockAtom::plain("title-1")],
                ..MockInfo::default()
            },
        ]);
        assert!(mock_host_state::has_info());
        assert_eq!(mock_host_state::get_info_count(), 2);
        assert_eq!(
            mock_host_state::get_info_title(0).unwrap()[0].contents,
            "title-0"
        );
        assert_eq!(mock_host_state::get_info_style(0), Some("info".into()));
        assert_eq!(
            mock_host_state::get_info_anchor(0),
            Some(MockCoord { line: 1, column: 0 })
        );
        // Out-of-range returns None.
        assert!(mock_host_state::get_info_title(5).is_none());
    }

    #[test]
    fn theme_token_lookup() {
        let mut h = TestHarness::new();
        let s = MockStyle::bg_rgb(40, 40, 50);
        h.set_theme_style("cursor.line.bg", s.clone());
        assert_eq!(
            mock_host_state::get_theme_style("cursor.line.bg"),
            Some(s)
        );
        assert!(mock_host_state::get_theme_style("missing").is_none());
    }

    #[test]
    fn syntax_indent_and_folds() {
        let mut h = TestHarness::new();
        h.set_syntax_generation(7);
        h.set_fold_ranges(vec![(3, 10), (15, 20)]);
        h.set_indent_level(0, 0);
        h.set_indent_level(1, 2);
        h.set_scopes_at(1, 4, vec!["meta.function".into(), "rust".into()]);

        assert_eq!(mock_host_state::get_syntax_generation(), 7);
        assert_eq!(mock_host_state::get_fold_ranges(), vec![(3, 10), (15, 20)]);
        assert_eq!(mock_host_state::get_indent_level(1), 2);
        assert_eq!(mock_host_state::get_indent_level(99), 0); // default
        assert_eq!(
            mock_host_state::get_scopes_at(1, 4),
            vec!["meta.function".to_string(), "rust".to_string()]
        );
        assert!(mock_host_state::get_scopes_at(0, 0).is_empty());
    }

    #[test]
    fn settings_typed() {
        let mut h = TestHarness::new();
        h.set_setting_bool("enabled", true);
        h.set_setting_integer("count", 42);
        h.set_setting_float("ratio", 1.5);
        h.set_setting_string("name", "foo");
        assert_eq!(mock_host_state::get_setting_bool("enabled"), Some(true));
        assert_eq!(mock_host_state::get_setting_integer("count"), Some(42));
        assert_eq!(mock_host_state::get_setting_float("ratio"), Some(1.5));
        assert_eq!(
            mock_host_state::get_setting_string("name"),
            Some("foo".into())
        );
        assert!(mock_host_state::get_setting_bool("missing").is_none());
    }

    #[test]
    fn command_log_collects_and_drains() {
        let mut h = TestHarness::new();
        push_command(CommandRecord::eval("write"));
        push_command(CommandRecord::send_keys(["<esc>", ":", "q", "<ret>"]));
        let cmds = h.drain_commands();
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0], CommandRecord::eval("write"));
        assert_eq!(cmds[1].kind, "SendKeys");
        // Drain leaves the buffer empty.
        assert!(h.drain_commands().is_empty());
    }

    #[test]
    fn arena_find_by_substring() {
        let _h = TestHarness::new();
        mock_element_builder::create_text("hello", "default");
        mock_element_builder::create_text("world", "default");
        mock_element_builder::create_text("hello world", "default");
        let h = _h;
        let matches = h.arena().find("hello");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn logs_round_trip() {
        let mut h = TestHarness::new();
        mock_host_log::log_message(0, "debug message");
        mock_host_log::log_message(3, "error message");
        let logs = h.drain_logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, 0);
        assert_eq!(logs[1].message, "error message");
    }
}
