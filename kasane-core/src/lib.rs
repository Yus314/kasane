pub mod clipboard;
pub mod config;
pub mod display;
pub mod display_algebra;
pub mod element;
pub mod event_loop;
pub mod history;
pub mod input;
pub mod io;
pub mod layout;
pub mod lens;
mod perf;
pub mod plugin;
pub mod plugin_prelude;
pub mod protocol;
pub mod render;
pub mod salsa_db;
pub mod salsa_inputs;
pub mod salsa_queries;
pub mod salsa_sync;
pub mod salsa_views;
pub mod scroll;
pub mod session;
pub mod state;
pub mod surface;
pub mod syntax;
#[doc(hidden)]
pub mod test_support;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod widget;
pub mod workspace;

pub use kasane_macros::DirtyTracked;
pub use kasane_macros::kasane_component;
pub use kasane_macros::kasane_plugin;
