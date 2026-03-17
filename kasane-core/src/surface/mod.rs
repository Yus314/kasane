//! Surface abstraction: first-class rectangular screen regions.
//!
//! A Surface owns a rectangular area of the screen and is responsible for
//! building its Element tree and handling events within that region.
//! Both core components (buffer, status bar) and plugins can implement Surface,
//! enabling symmetric extensibility.

pub mod buffer;
pub mod info;
pub mod menu;
mod registry;
pub mod resolve;
pub mod status;
mod traits;
mod types;

pub use registry::*;
pub use traits::*;
pub use types::*;

pub use resolve::{
    ContributorIssue, ContributorIssueKind, OwnerValidationError, OwnerValidationErrorKind,
    ResolvedSlotContentKind, ResolvedSlotRecord, ResolvedTree, SurfaceComposeResult,
    SurfaceRenderOutcome, SurfaceRenderReport,
};
