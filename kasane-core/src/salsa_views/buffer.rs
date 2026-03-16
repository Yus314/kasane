use crate::element::Element;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::*;
use crate::salsa_queries;

/// Pure buffer element: a BufferRef spanning the available height.
#[salsa::tracked(no_eq)]
pub fn pure_buffer_element(db: &dyn KasaneDb, config: ConfigInput) -> Element {
    let height = salsa_queries::available_height(db, config) as usize;
    Element::buffer_ref(0..height)
}
