pub mod atlas;
pub mod authority;
pub mod config;
pub mod context;
pub mod green;
pub mod identity;
pub mod orbit;
pub mod overview;
pub mod project;
pub mod purpose;
pub mod scan;
pub mod sockets;
pub mod sources;
pub mod universe;

use creature_context_macros::context_enforce;

#[context_enforce]
pub fn active_metadata_enforcement() {}
