//! Compile-fail witness that `Truth<'a>` denies mutation.
//!
//! `Truth` wraps `&AppState` and exposes only by-value / shared-reference
//! accessors. Attempting to obtain a unique reference or to assign through
//! any accessor must be rejected by the borrow checker / type system.

use kasane_core::state::{AppState, Truth};

fn main() {
    let state = AppState::default();
    let truth: Truth<'_> = state.truth();

    // `cursor_pos()` returns `Coord` by value; you cannot assign to it.
    truth.cursor_pos() = kasane_core::protocol::Coord { line: 0, column: 0 };

    // `lines()` returns `&[Line]`; indexing returns a shared reference.
    // Attempting to take `&mut` through a shared projection is rejected.
    let _mut_ref: &mut kasane_core::protocol::Line = &mut truth.lines()[0];
}
