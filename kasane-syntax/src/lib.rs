//! Tree-sitter based syntax analysis for Kasane.
//!
//! Provides [`TreeSitterProvider`] (implements `kasane_core::syntax::SyntaxProvider`)
//! and [`SyntaxManager`] (lifecycle management for per-buffer providers).

mod fs_class;
mod grammar;
mod manager;
mod provider;
mod watcher;

pub use grammar::GrammarRegistry;
pub use manager::SyntaxManager;
pub use provider::TreeSitterProvider;
pub use watcher::{FileWatcher, FileWatcherError};
