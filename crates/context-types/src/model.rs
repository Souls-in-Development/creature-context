use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    CandidateId, EntityId, RecordId, SnapshotId,
    atlas::AtlasEdge,
    context::{ContextRecord, PrivacyClass},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferredSummary {
    pub value: String,
    pub producer: String,
    pub model_id: String,
    pub confidence: f32,
    pub source_record_ids: Vec<RecordId>,
    pub snapshot_id: SnapshotId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CandidatePayload {
    Context(ContextRecord),
    Summary {
        entity_id: EntityId,
        summary: InferredSummary,
    },
    Edge(AtlasEdge),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CandidateState {
    Pending,
    Admitted,
    Review,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateRecord {
    pub id: CandidateId,
    pub payload: CandidatePayload,
    pub provider_id: String,
    pub model_id: String,
    pub capability_profile_id: String,
    pub schema_version: u32,
    pub state: CandidateState,
    pub rejection_reasons: Vec<String>,
    pub created_at: String,
    pub snapshot_id: SnapshotId,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize)]
pub enum ModelRole {
    Contextual,
    Structural,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CapabilityState {
    Unavailable,
    ImplementedUnverified,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProfile {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub state: CapabilityState,
    pub privacy_class: PrivacyClass,
    pub role_scores: BTreeMap<ModelRole, f32>,
    pub structured_output_rate: f32,
    pub attribution_rate: f32,
    pub p95_latency_ms: u64,
    pub measured_input_limit: usize,
    pub measured_output_limit: usize,
    pub memory_mib: u64,
    pub storage_mib: u64,
    pub tested_languages: BTreeSet<String>,
    pub calibration_version: String,
    pub calibrated_at: String,
    pub evidence_locator: Option<String>,
}
