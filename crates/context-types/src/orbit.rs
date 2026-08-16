use std::collections::BTreeMap;

use crate::{
    ComparisonDimension, ComparisonResult, EntityId, SnapshotId,
    atlas::{AtlasEdge, AtlasEntity, ConflictRecord},
    context::{ContextRecord, PrivacyClass},
    evidence::ProofStrength,
};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum OrbitScale {
    Universe,
    Galaxy,
    System,
    Planet,
    Moon,
    #[default]
    Adaptive,
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum OrbitMode {
    Design,
    #[default]
    Focus,
    Trace,
    Compare,
    Health,
    Change,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotPreference {
    Current,
    Exact(SnapshotId),
    AtOrBefore(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InferredPolicy {
    Exclude,
    IncludeAttributed,
    PreferDeterministic,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntityReference {
    #[serde(default)]
    pub stable_id: Option<EntityId>,
    #[serde(default)]
    pub relative_path: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OrbitRequest {
    pub task: String,
    pub target_references: Vec<EntityReference>,
    pub exclusions: Vec<EntityId>,
    pub scale: OrbitScale,
    pub mode: OrbitMode,
    pub comparison_dimensions: Vec<ComparisonDimension>,
    pub snapshot_preference: SnapshotPreference,
    pub token_budget: usize,
    pub maximum_graph_depth: usize,
    pub required_proof_floor: ProofStrength,
    pub inferred_policy: InferredPolicy,
    pub privacy_ceiling: PrivacyClass,
    pub client_id: Option<String>,
    pub session_id: Option<String>,
}

impl Default for OrbitRequest {
    fn default() -> Self {
        Self {
            task: String::new(),
            target_references: Vec::new(),
            exclusions: Vec::new(),
            scale: OrbitScale::Adaptive,
            mode: OrbitMode::Focus,
            comparison_dimensions: Vec::new(),
            snapshot_preference: SnapshotPreference::Current,
            token_budget: 64_000,
            maximum_graph_depth: 2,
            required_proof_floor: ProofStrength::Metadata,
            inferred_policy: InferredPolicy::PreferDeterministic,
            privacy_ceiling: PrivacyClass::Project,
            client_id: None,
            session_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedReference {
    pub requested: EntityReference,
    pub entity_id: EntityId,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedContextRecord {
    pub record: ContextRecord,
    pub mandatory: bool,
    pub ring: u8,
    pub reasons: Vec<String>,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedEdge {
    pub edge: AtlasEdge,
    pub mandatory: bool,
    pub ring: u8,
    pub reasons: Vec<String>,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedEntity {
    pub entity: AtlasEntity,
    pub mandatory: bool,
    pub score: i64,
    pub reasons: Vec<String>,
    pub ring: u8,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrbitPacket {
    pub id: String,
    pub scale: OrbitScale,
    pub mode: OrbitMode,
    pub task: String,
    pub architectural_spine: Vec<AtlasEntity>,
    #[serde(default)]
    pub selected_entities: Vec<SelectedEntity>,
    pub comparison: Option<ComparisonResult>,
    #[serde(default)]
    pub uncertainty: Vec<String>,
    #[serde(default)]
    pub selection_reasons: Vec<String>,
    pub estimated_total_tokens: usize,
    pub budget: usize,
    pub request: OrbitRequest,
    #[serde(default)]
    pub resolved_references: Vec<ResolvedReference>,
    #[serde(default)]
    pub context_records: Vec<SelectedContextRecord>,
    #[serde(default)]
    pub conflicts: Vec<ConflictRecord>,
    #[serde(default)]
    pub relationships: Vec<SelectedEdge>,
    #[serde(default)]
    pub omission_counts: BTreeMap<String, usize>,
    #[serde(default)]
    pub minimum_required_tokens: Option<usize>,
}
