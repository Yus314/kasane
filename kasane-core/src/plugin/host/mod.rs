//! Host-side runtime context exposed to plugins.
//!
//! `app_view` is the read-only `AppView` projection a plugin sees per
//! method call. `context` carries the per-method context objects
//! (`TransformContext`, `ContributeContext`, etc.). `variable_store`
//! and `setting` host the typed per-plugin value stores.

pub mod app_view;
pub mod context;
pub mod setting;
pub mod variable_store;
