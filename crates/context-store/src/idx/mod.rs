//! Canonical IDX: the primary representation Creature Context maintains for
//! coding agents.
//!
//! Compact UTF-8, LF endings, one typed record per line, stable field ordering,
//! self-describing via `@legend`. See specification section 5.

mod decode;
mod encode;
mod escape;

pub use decode::decode_atlas_idx;
pub use encode::{encode_atlas_idx, encode_orbit_idx};

use creature_context_types::{AtlasSnapshot, EntityId, orbit::OrbitPacket};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IdxError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Copy)]
pub enum IdxScope {
    /// The whole Galaxy, for the root `ATLAS.idx`.
    Galaxy,
    /// One entity's subtree, for a per-folder `ATLAS.idx`.
    Folder(EntityId),
}

pub struct DecodedIdx {
    pub snapshot: AtlasSnapshot,
    /// Records whose type this build does not recognise, retained verbatim so
    /// forward-compatible files survive a round trip (specification 5.1).
    pub opaque_records: Vec<String>,
}

pub trait IdxRenderable {
    fn render_idx(&self) -> Result<String, IdxError>;
}

impl IdxRenderable for OrbitPacket {
    fn render_idx(&self) -> Result<String, IdxError> {
        encode_orbit_idx(self)
    }
}
