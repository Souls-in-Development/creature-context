use crate::{
    ConflictId, EdgeId, EntityId, EntityKind, Evidence, GreenAssessment, RecordId, ScopeScale,
    SnapshotId,
    context::{ContextRecord, ContextSource},
    green::GreenCode,
    model::InferredSummary,
    socket::AtlasSocket,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipPlane {
    Declared,
    Observed,
    Inferred,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Contains,
    Imports,
    Calls,
    References,
    Implements,
    Conforms,
    Tests,
    Configures,
    Deploys,
    Produces,
    Consumes,
    Shares,
    Supersedes,
    Conflicts,
    Duplicates,
    Resembles,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub source_id: String,
    pub relative_path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub content_hash: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasEntity {
    pub id: EntityId,
    pub scale: ScopeScale,
    pub kind: EntityKind,
    pub canonical_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub relative_path: Option<String>,
    pub parent_id: Option<EntityId>,
    #[serde(default)]
    pub purpose_clauses: Vec<String>,
    #[serde(default)]
    pub protected_decision_ids: Vec<RecordId>,
    #[serde(default)]
    pub responsibilities: Vec<String>,
    #[serde(default)]
    pub interfaces: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub sockets: Vec<AtlasSocket>,
    #[serde(default)]
    pub source_spans: Vec<SourceSpan>,
    pub structural_fingerprint: String,
    #[serde(default)]
    pub local_evidence: Vec<Evidence>,
    #[serde(default)]
    pub inherited_evidence: Vec<Evidence>,
    pub green: Option<GreenAssessment>,
    #[serde(default)]
    pub open_conflict_ids: Vec<ConflictId>,
    pub deterministic_summary: String,
    #[serde(default)]
    pub inferred_summaries: Vec<InferredSummary>,
    #[serde(default)]
    pub uncertainty: Vec<String>,
    pub snapshot_id: SnapshotId,
    pub observed_at: String,
    #[serde(default)]
    pub fresh_until: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasEdge {
    pub id: EdgeId,
    pub source_entity_id: EntityId,
    pub target_entity_id: EntityId,
    pub kind: RelationshipKind,
    pub plane: RelationshipPlane,
    #[serde(default)]
    pub proof_record_ids: Vec<RecordId>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub source_id: String,
    pub confidence: f32,
    pub observed_at: String,
    #[serde(default)]
    pub fresh_until: Option<String>,
    pub required: bool,
    pub snapshot_id: SnapshotId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictState {
    Open,
    Resolved,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictRecord {
    pub id: ConflictId,
    pub left_record_id: RecordId,
    pub right_record_id: RecordId,
    pub state: ConflictState,
    pub severity: GreenCode,
    #[serde(default)]
    pub resolution_record_id: Option<RecordId>,
    pub created_at: String,
    pub snapshot_id: SnapshotId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtlasSnapshot {
    #[serde(rename = "snapshot_id")]
    pub id: SnapshotId,
    #[serde(default)]
    pub timestamp: String,
    pub entities: Vec<AtlasEntity>,
    pub edges: Vec<AtlasEdge>,
    #[serde(default)]
    pub records: Vec<ContextRecord>,
    #[serde(default)]
    pub conflicts: Vec<ConflictRecord>,
    #[serde(default)]
    pub sources: Vec<ContextSource>,
}
