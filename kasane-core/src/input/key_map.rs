//! Framework-side compiled key map for declarative key binding dispatch.
//!
//! [`CompiledKeyMap`] holds the binding tables built from a plugin's key map
//! declaration. The framework uses it to resolve key events to action IDs
//! without crossing plugin boundaries, and manages chord state centrally.

use std::time::Instant;

use super::{KeyEvent, KeyPattern};

// ---------------------------------------------------------------------------
// Binding declarations
// ---------------------------------------------------------------------------

/// A single key → action mapping.
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub pattern: KeyPattern,
    pub action_id: &'static str,
}

/// A chord (leader → follower) → action mapping.
#[derive(Debug, Clone)]
pub struct ChordBinding {
    pub leader: KeyEvent,
    pub follower: KeyPattern,
    pub action_id: &'static str,
}

/// A named group of bindings that can be conditionally active.
#[derive(Debug, Clone)]
pub struct KeyGroup {
    pub name: &'static str,
    pub active: bool,
    pub bindings: Vec<KeyBinding>,
    pub chords: Vec<ChordBinding>,
}

// ---------------------------------------------------------------------------
// Chord state
// ---------------------------------------------------------------------------

/// Framework-managed chord pending state.
#[derive(Debug, Clone, Default)]
pub struct ChordState {
    pub pending_leader: Option<KeyEvent>,
    pub pending_since: Option<Instant>,
}

impl ChordState {
    /// Whether a chord leader is pending.
    pub fn is_pending(&self) -> bool {
        self.pending_leader.is_some()
    }

    /// Set a new pending leader.
    pub fn set_pending(&mut self, leader: KeyEvent) {
        self.pending_leader = Some(leader);
        self.pending_since = Some(Instant::now());
    }

    /// Cancel the pending chord and return the leader that was pending, if any.
    pub fn cancel(&mut self) -> Option<KeyEvent> {
        self.pending_since = None;
        self.pending_leader.take()
    }

    /// Check if the chord has timed out (default: 500ms).
    pub fn is_timed_out(&self, timeout_ms: u64) -> bool {
        self.pending_since
            .is_some_and(|since| since.elapsed().as_millis() as u64 >= timeout_ms)
    }
}

// ---------------------------------------------------------------------------
// CompiledKeyMap
// ---------------------------------------------------------------------------

/// Default chord timeout in milliseconds.
pub const DEFAULT_CHORD_TIMEOUT_MS: u64 = 500;

/// Compiled key map for a single plugin, built from its key map declaration.
///
/// Used by the framework to:
/// 1. Check if a key event matches any binding (skipping WASM call if not)
/// 2. Manage chord leader/follower state centrally
/// 3. Resolve matched bindings to action IDs for `invoke_action` dispatch
#[derive(Debug, Clone)]
pub struct CompiledKeyMap {
    pub groups: Vec<KeyGroup>,
    pub chord_timeout_ms: u64,
}

impl Default for CompiledKeyMap {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            chord_timeout_ms: DEFAULT_CHORD_TIMEOUT_MS,
        }
    }
}

impl CompiledKeyMap {
    /// Try to match a key event against single-key bindings in active groups.
    ///
    /// Returns the action ID of the first matching binding, or `None`.
    pub fn match_key(&self, event: &KeyEvent) -> Option<&'static str> {
        for group in &self.groups {
            if !group.active {
                continue;
            }
            for binding in &group.bindings {
                if binding.pattern.matches(event) {
                    return Some(binding.action_id);
                }
            }
        }
        None
    }

    /// Check if the key event matches any chord leader in active groups.
    pub fn match_chord_leader(&self, event: &KeyEvent) -> bool {
        for group in &self.groups {
            if !group.active {
                continue;
            }
            for chord in &group.chords {
                if &chord.leader == event {
                    return true;
                }
            }
        }
        false
    }

    /// Try to match a chord follower given the pending leader.
    ///
    /// Returns the action ID of the first matching chord binding, or `None`.
    pub fn match_chord_follower(
        &self,
        leader: &KeyEvent,
        follower: &KeyEvent,
    ) -> Option<&'static str> {
        for group in &self.groups {
            if !group.active {
                continue;
            }
            for chord in &group.chords {
                if &chord.leader == leader && chord.follower.matches(follower) {
                    return Some(chord.action_id);
                }
            }
        }
        None
    }

    /// Whether any active group has a catch-all (`KeyPattern::Any`) binding.
    pub fn has_catch_all(&self) -> bool {
        self.groups.iter().any(|g| {
            g.active
                && g.bindings
                    .iter()
                    .any(|b| matches!(b.pattern, KeyPattern::Any))
        })
    }

    /// Whether this key map has any bindings at all (in any group).
    pub fn is_empty(&self) -> bool {
        self.groups
            .iter()
            .all(|g| g.bindings.is_empty() && g.chords.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::{Key, Modifiers};
    use super::*;

    fn plain(c: char) -> KeyEvent {
        KeyEvent::char_plain(c)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::ctrl(c)
    }

    fn escape() -> KeyEvent {
        KeyEvent {
            key: Key::Escape,
            modifiers: Modifiers::empty(),
        }
    }

    fn make_map(groups: Vec<KeyGroup>) -> CompiledKeyMap {
        CompiledKeyMap {
            groups,
            chord_timeout_ms: DEFAULT_CHORD_TIMEOUT_MS,
        }
    }

    // --- KeyPattern matching ---

    #[test]
    fn exact_pattern_matches_identical_key() {
        let pat = KeyPattern::Exact(ctrl('p'));
        assert!(pat.matches(&ctrl('p')));
    }

    #[test]
    fn exact_pattern_rejects_different_key() {
        let pat = KeyPattern::Exact(ctrl('p'));
        assert!(!pat.matches(&ctrl('q')));
        assert!(!pat.matches(&plain('p')));
    }

    #[test]
    fn any_char_matches_any_character_key() {
        let pat = KeyPattern::AnyChar;
        assert!(pat.matches(&plain('a')));
        assert!(pat.matches(&ctrl('a'))); // AnyChar ignores modifiers
        assert!(!pat.matches(&escape()));
    }

    #[test]
    fn any_char_plain_matches_unmodified_char() {
        let pat = KeyPattern::AnyCharPlain;
        assert!(pat.matches(&plain('a')));
        assert!(!pat.matches(&ctrl('a')));
        assert!(!pat.matches(&KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::ALT,
        }));
        // Shift is allowed
        assert!(pat.matches(&KeyEvent {
            key: Key::Char('A'),
            modifiers: Modifiers::SHIFT,
        }));
    }

    #[test]
    fn any_pattern_matches_everything() {
        let pat = KeyPattern::Any;
        assert!(pat.matches(&plain('a')));
        assert!(pat.matches(&ctrl('z')));
        assert!(pat.matches(&escape()));
        assert!(pat.matches(&KeyEvent {
            key: Key::F(1),
            modifiers: Modifiers::empty(),
        }));
    }

    // --- KeyEvent constructors and matchers ---

    #[test]
    fn char_plain_constructor() {
        let e = KeyEvent::char_plain('x');
        assert_eq!(e.key, Key::Char('x'));
        assert_eq!(e.modifiers, Modifiers::empty());
    }

    #[test]
    fn ctrl_constructor() {
        let e = KeyEvent::ctrl('w');
        assert_eq!(e.key, Key::Char('w'));
        assert_eq!(e.modifiers, Modifiers::CTRL);
    }

    #[test]
    fn matches_ctrl_positive() {
        assert!(KeyEvent::ctrl('p').matches_ctrl('p'));
    }

    #[test]
    fn matches_ctrl_negative() {
        assert!(!KeyEvent::char_plain('p').matches_ctrl('p'));
        assert!(!KeyEvent::ctrl('q').matches_ctrl('p'));
    }

    #[test]
    fn matches_char_plain_positive() {
        assert!(KeyEvent::char_plain('a').matches_char_plain('a'));
    }

    #[test]
    fn matches_char_plain_negative() {
        assert!(!KeyEvent::ctrl('a').matches_char_plain('a'));
        assert!(!KeyEvent::char_plain('b').matches_char_plain('a'));
    }

    #[test]
    fn plain_char_extracts_char() {
        assert_eq!(KeyEvent::char_plain('z').plain_char(), Some('z'));
    }

    #[test]
    fn plain_char_none_for_modified() {
        assert_eq!(KeyEvent::ctrl('z').plain_char(), None);
    }

    #[test]
    fn plain_char_none_for_special() {
        assert_eq!(escape().plain_char(), None);
    }

    // --- CompiledKeyMap matching ---

    #[test]
    fn match_key_finds_binding_in_active_group() {
        let map = make_map(vec![KeyGroup {
            name: "active",
            active: true,
            bindings: vec![KeyBinding {
                pattern: KeyPattern::Exact(ctrl('p')),
                action_id: "activate",
            }],
            chords: vec![],
        }]);
        assert_eq!(map.match_key(&ctrl('p')), Some("activate"));
        assert_eq!(map.match_key(&ctrl('q')), None);
    }

    #[test]
    fn match_key_skips_inactive_group() {
        let map = make_map(vec![KeyGroup {
            name: "inactive",
            active: false,
            bindings: vec![KeyBinding {
                pattern: KeyPattern::Exact(ctrl('p')),
                action_id: "activate",
            }],
            chords: vec![],
        }]);
        assert_eq!(map.match_key(&ctrl('p')), None);
    }

    #[test]
    fn match_key_first_group_wins() {
        let map = make_map(vec![
            KeyGroup {
                name: "first",
                active: true,
                bindings: vec![KeyBinding {
                    pattern: KeyPattern::Exact(ctrl('p')),
                    action_id: "first_action",
                }],
                chords: vec![],
            },
            KeyGroup {
                name: "second",
                active: true,
                bindings: vec![KeyBinding {
                    pattern: KeyPattern::Exact(ctrl('p')),
                    action_id: "second_action",
                }],
                chords: vec![],
            },
        ]);
        assert_eq!(map.match_key(&ctrl('p')), Some("first_action"));
    }

    #[test]
    fn match_chord_leader_found() {
        let map = make_map(vec![KeyGroup {
            name: "chords",
            active: true,
            bindings: vec![],
            chords: vec![ChordBinding {
                leader: ctrl('w'),
                follower: KeyPattern::Exact(plain('v')),
                action_id: "split_v",
            }],
        }]);
        assert!(map.match_chord_leader(&ctrl('w')));
        assert!(!map.match_chord_leader(&ctrl('x')));
    }

    #[test]
    fn match_chord_follower_found() {
        let map = make_map(vec![KeyGroup {
            name: "chords",
            active: true,
            bindings: vec![],
            chords: vec![
                ChordBinding {
                    leader: ctrl('w'),
                    follower: KeyPattern::Exact(plain('v')),
                    action_id: "split_v",
                },
                ChordBinding {
                    leader: ctrl('w'),
                    follower: KeyPattern::Exact(plain('s')),
                    action_id: "split_h",
                },
            ],
        }]);
        assert_eq!(
            map.match_chord_follower(&ctrl('w'), &plain('v')),
            Some("split_v")
        );
        assert_eq!(
            map.match_chord_follower(&ctrl('w'), &plain('s')),
            Some("split_h")
        );
        assert_eq!(map.match_chord_follower(&ctrl('w'), &plain('x')), None);
    }

    #[test]
    fn has_catch_all_true() {
        let map = make_map(vec![KeyGroup {
            name: "catch",
            active: true,
            bindings: vec![KeyBinding {
                pattern: KeyPattern::Any,
                action_id: "consume_all",
            }],
            chords: vec![],
        }]);
        assert!(map.has_catch_all());
    }

    #[test]
    fn has_catch_all_false_when_inactive() {
        let map = make_map(vec![KeyGroup {
            name: "catch",
            active: false,
            bindings: vec![KeyBinding {
                pattern: KeyPattern::Any,
                action_id: "consume_all",
            }],
            chords: vec![],
        }]);
        assert!(!map.has_catch_all());
    }

    #[test]
    fn is_empty_true_for_no_bindings() {
        let map = make_map(vec![KeyGroup {
            name: "empty",
            active: true,
            bindings: vec![],
            chords: vec![],
        }]);
        assert!(map.is_empty());
    }

    #[test]
    fn is_empty_false_with_bindings() {
        let map = make_map(vec![KeyGroup {
            name: "has_bindings",
            active: true,
            bindings: vec![KeyBinding {
                pattern: KeyPattern::Exact(ctrl('p')),
                action_id: "test",
            }],
            chords: vec![],
        }]);
        assert!(!map.is_empty());
    }

    // --- ChordState ---

    #[test]
    fn chord_state_default_not_pending() {
        let state = ChordState::default();
        assert!(!state.is_pending());
    }

    #[test]
    fn chord_state_set_pending() {
        let mut state = ChordState::default();
        state.set_pending(ctrl('w'));
        assert!(state.is_pending());
        assert_eq!(state.pending_leader.as_ref(), Some(&ctrl('w')));
    }

    #[test]
    fn chord_state_cancel() {
        let mut state = ChordState::default();
        state.set_pending(ctrl('w'));
        let leader = state.cancel();
        assert_eq!(leader, Some(ctrl('w')));
        assert!(!state.is_pending());
    }

    #[test]
    fn chord_state_cancel_when_empty() {
        let mut state = ChordState::default();
        assert_eq!(state.cancel(), None);
    }

    #[test]
    fn chord_state_not_timed_out_initially() {
        let mut state = ChordState::default();
        state.set_pending(ctrl('w'));
        // Just set — should not be timed out
        assert!(!state.is_timed_out(500));
    }

    // --- AnyChar with catch-all priority ---

    #[test]
    fn any_char_plain_binding_matches_plain_chars() {
        let map = make_map(vec![KeyGroup {
            name: "input",
            active: true,
            bindings: vec![
                KeyBinding {
                    pattern: KeyPattern::Exact(escape()),
                    action_id: "close",
                },
                KeyBinding {
                    pattern: KeyPattern::AnyCharPlain,
                    action_id: "append_char",
                },
            ],
            chords: vec![],
        }]);
        assert_eq!(map.match_key(&escape()), Some("close"));
        assert_eq!(map.match_key(&plain('a')), Some("append_char"));
        assert_eq!(map.match_key(&ctrl('a')), None); // Ctrl+a not matched by AnyCharPlain
    }
}
