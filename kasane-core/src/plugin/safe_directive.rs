//! Non-destructive display directive projection (ADR-030 Level 4).
//!
//! `SafeDisplayDirective` is to `DisplayDirective` what `TransparentCommand`
//! is to `Command`: a newtype restricting construction to the non-destructive
//! subset. There is no constructor for `Hide`, making non-destructiveness a
//! compile-time property.

use std::ops::Range;

use crate::display::DisplayDirective;
use crate::protocol::Atom;

/// A display directive guaranteed not to be destructive.
///
/// Construction is restricted to non-destructive `DisplayDirective` variants.
/// `Hide` has no constructor on this type, making non-destructiveness a
/// compile-time property.
pub struct SafeDisplayDirective(DisplayDirective);

impl std::fmt::Debug for SafeDisplayDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SafeDisplayDirective({})", self.0.variant_name())
    }
}

impl SafeDisplayDirective {
    /// All variant names covered by this projection (sorted).
    pub const VARIANT_NAMES: &'static [&'static str] = &["Fold"];

    /// Collapse a range of buffer lines into a single summary line.
    pub fn fold(range: Range<usize>, summary: Vec<Atom>) -> Self {
        Self(DisplayDirective::Fold { range, summary })
    }
}

impl From<SafeDisplayDirective> for DisplayDirective {
    fn from(safe: SafeDisplayDirective) -> Self {
        safe.0
    }
}
