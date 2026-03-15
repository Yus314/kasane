//! Salsa database definition for Kasane's incremental computation layer.
//!
//! This module defines the core database trait and struct that all Salsa
//! tracked functions operate against.

/// The Salsa database trait for Kasane.
///
/// All tracked functions take `&dyn KasaneDb` as their first argument.
#[salsa::db]
pub trait KasaneDb: salsa::Database {}

/// Concrete database implementation.
#[salsa::db]
#[derive(Default)]
pub struct KasaneDatabase {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for KasaneDatabase {}

#[salsa::db]
impl KasaneDb for KasaneDatabase {}
