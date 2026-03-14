pub mod config;
pub mod element;
pub mod event_loop;
pub mod input;
pub mod io;
pub mod layout;
pub mod pane;
mod perf;
pub mod plugin;
pub mod plugin_prelude;
pub mod protocol;
pub mod render;
pub mod state;
pub mod surface;
#[doc(hidden)]
pub mod test_support;
#[cfg(test)]
pub(crate) mod test_utils;
pub mod workspace;

pub use kasane_macros::kasane_component;
pub use kasane_macros::kasane_plugin;
