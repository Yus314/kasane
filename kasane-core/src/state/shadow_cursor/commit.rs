//! Commit pipeline: shadow-cursor edit → BufferEdit algebra → Kakoune commands.

use crate::history::VersionId;
use crate::plugin::Command;
use crate::state::selection::Selection;

use super::{EditProjection, EditableSpan, ShadowCursor, ShadowPhase};

/// Algebraic representation of a buffer edit produced by a shadow
/// cursor commit. ADR-035 §Migration ShadowCursor Phase 3 — the
/// `BufferEdit` shape is the algebraic source of truth; the
/// Kakoune `exec -draft` command is a thin serialization on top
/// (`edit_to_commands`). Phase 4 adds `base_version`, the
/// `VersionId` against which the edit was authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferEdit {
    /// Buffer-space target of the edit. For a Mirror projection,
    /// `target.anchor.line == target.cursor.line` and
    /// `min().column..max().column` is the byte range being
    /// replaced.
    pub target: Selection,
    /// Pre-edit content (what currently lives at `target`). Used
    /// for Hippocratic checks and round-tripping.
    pub original: String,
    /// Post-edit content (what `target` should contain after the
    /// commit). Empty string represents a pure deletion.
    pub replacement: String,
    /// `VersionId` at the moment the user activated the
    /// shadow cursor edit (stamped from
    /// `ShadowPhase::Editing.base_version`). Lets a downstream
    /// consumer detect a stale commit
    /// (`is_stale_against(current)`) or compose with `Time::At(v)`
    /// queries to materialise the buffer state the edit was
    /// authored against.
    pub base_version: VersionId,
}

/// Verdict returned by an `on_buffer_edit_intercept` plugin handler.
///
/// The dispatch loop folds verdicts in plugin-priority order:
/// `PassThrough` is identity; `Replace(new)` substitutes the running
/// edit; `Veto` short-circuits and drops the commit.
///
/// `Default` is `PassThrough` — used by host bindings (e.g.
/// `WasmPlugin::intercept_buffer_edit` via `call_synced`'s
/// `R::default()` fallback) when the plugin call fails or the
/// plugin doesn't override the handler.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BufferEditVerdict {
    #[default]
    /// Use the running edit unchanged. Equivalent to a plugin that
    /// did not register an intercept handler. Typical for plugins
    /// that observe edits without modifying them (e.g. logging).
    PassThrough,
    /// Replace the running edit with a transformed version. Use to
    /// rewrite the target / replacement before commit (e.g. snap
    /// indentation, auto-format the replacement, etc.). The
    /// replacement edit's `base_version` is preserved unless the
    /// handler explicitly overwrites it.
    Replace(BufferEdit),
    /// Veto the commit. No Kakoune commands are emitted; the shadow
    /// cursor still deactivates. Use sparingly — typical use is
    /// when the edit would violate a plugin-owned invariant.
    Veto,
}

impl BufferEdit {
    /// True when the edit would not change the buffer
    /// (`original == replacement`). Callers should skip command
    /// emission for hippocratic edits.
    pub fn is_hippocratic_noop(&self) -> bool {
        self.original == self.replacement
    }

    /// True when the buffer has advanced past the version this
    /// edit was authored against (`current > base_version`). A
    /// stale edit may still be safe to land — only adversarial
    /// concurrent edits to the *same byte range* break the
    /// projection — but the caller can use this signal to gate
    /// commit, prompt the user, or replay the edit on the new
    /// base.
    pub fn is_stale_against(&self, current: VersionId) -> bool {
        current > self.base_version
    }
}

/// Compute the `BufferEdit` for a Mirror-projection shadow cursor
/// commit, or `None` when no edit should be produced.
///
/// Returns `None` when:
/// - the shadow cursor is in `Navigating` phase (nothing to commit),
/// - the working text matches the original (Hippocratic noop),
/// - `span.projection_target.anchor.line >= line_count`
///   (anchor line out of range — graceful degradation),
/// - `span.projection != Mirror` (`PluginDefined` projections are
///   handled by `on_virtual_edit`, deferred to BDT-7).
pub fn mirror_edit(
    shadow: &ShadowCursor,
    span: &EditableSpan,
    line_count: usize,
) -> Option<BufferEdit> {
    let (working_text, original_text, base_version) = match &shadow.phase {
        ShadowPhase::Editing {
            working_text,
            original_text,
            base_version,
            ..
        } => (working_text.as_str(), original_text.as_str(), *base_version),
        ShadowPhase::Navigating => return None,
    };

    if working_text == original_text {
        return None;
    }

    if span.projection != EditProjection::Mirror {
        return None;
    }

    let target = span.projection_target;
    if (target.anchor.line as usize) >= line_count {
        return None;
    }

    Some(BufferEdit {
        target,
        original: original_text.to_string(),
        replacement: working_text.to_string(),
        base_version,
    })
}

/// Serialize a `BufferEdit` into the Kakoune `exec -draft` command(s)
/// that will land it. The single-command form selects the byte range
/// and either inserts (empty range) or substitutes (non-empty range).
pub fn edit_to_commands(edit: &BufferEdit) -> Vec<Command> {
    let line_1indexed = edit.target.anchor.line as usize + 1;
    let col_min = edit.target.min().column as usize;
    let col_max = edit.target.max().column as usize;
    let col_start = col_min + 1; // 1-indexed
    let col_end = col_max; // inclusive end in Kakoune

    let escaped = escape_for_kakoune_insert(&edit.replacement);

    let cmd = if col_min == col_max {
        format!("exec -draft {line_1indexed}g {col_start}li{escaped}<esc>")
    } else {
        format!("exec -draft {line_1indexed}g {col_start}l{col_end}lsc{escaped}<esc>")
    };

    vec![Command::kakoune_command(&cmd)]
}

/// Build mirror-projection commit commands from a shadow cursor and
/// its span. Thin composition of `mirror_edit` + `edit_to_commands`;
/// preserved as the public entry point for the update dispatch.
pub fn build_mirror_commit(
    shadow: &ShadowCursor,
    span: &EditableSpan,
    line_count: usize,
) -> Vec<Command> {
    mirror_edit(shadow, span, line_count)
        .as_ref()
        .map(edit_to_commands)
        .unwrap_or_default()
}

/// Escape text for Kakoune insert mode within `exec -draft`.
fn escape_for_kakoune_insert(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => result.push_str("<lt>"),
            '>' => result.push_str("<gt>"),
            '\n' => result.push_str("<ret>"),
            _ => result.push(c),
        }
    }
    result
}
