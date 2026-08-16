use crate::{EntityId, SnapshotId, SocketId};
use serde::{Deserialize, Serialize};

/// Whether an entity offers a capability or needs one supplied by another
/// entity. Direction belongs to the socket rather than to a relationship: a
/// required socket remains meaningful even when no relationship exists.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketDirection {
    Requires,
    Provides,
}

/// The complete identity used for deterministic socket matching. The hash is
/// cached for compact IDX output; matching must be based on all three source
/// fields, never on a structural signature alone.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SocketShape {
    pub qualified_name: String,
    pub structural_signature: String,
    pub version: String,
    pub hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitBasis {
    Unique,
    Ranked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitStatus {
    Unconfirmed,
    Confirmed,
    Rejected,
}

/// Only evidence strong enough to settle a connection may confirm or reject a
/// fit. Metadata, syntax and lint remain useful evidence elsewhere, but they
/// cannot establish that two components genuinely connect.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitProof {
    Typecheck,
    Build,
    Test,
    Human,
}

/// A fit is a proposed or observed connection. Declared architecture may rank
/// candidates, but cannot itself be the plane of a fit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitPlane {
    Inferred,
    Observed,
}

/// Whether current verification infrastructure can settle a proposed fit.
/// This is deliberately separate from FitStatus: an unconfirmed fit with no
/// proof path is different from one that has not been checked yet.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofPathState {
    Unchecked,
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoleReason {
    NoMatch,
    Ambiguous,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SocketFit {
    pub provided_socket_id: SocketId,
    pub basis: FitBasis,
    pub status: FitStatus,
    #[serde(default)]
    pub checked_by: Option<FitProof>,
    pub proof_path: ProofPathState,
    pub plane: FitPlane,
    pub confidence: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SocketHole {
    pub reason: HoleReason,
    #[serde(default)]
    pub candidates: Vec<SocketId>,
    pub adapter_target: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SocketResolution {
    #[default]
    Unresolved,
    Fit(SocketFit),
    Hole(SocketHole),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasSocket {
    pub id: SocketId,
    pub entity_id: EntityId,
    pub direction: SocketDirection,
    pub shape: SocketShape,
    pub optional: bool,
    #[serde(default)]
    pub resolution: SocketResolution,
    pub source_id: String,
    pub confidence: f32,
    pub observed_at: String,
    pub snapshot_id: SnapshotId,
}
