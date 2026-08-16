use crate::{EntityId, GreenAxis, SnapshotId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactSource {
    Declared,
    Parsed,
    Observed,
    Inferred,
    Human,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofStrength {
    Unknown,
    Metadata,
    Syntax,
    Lint,
    Typecheck,
    Build,
    Test,
    Human,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOutcome {
    Unknown,
    Pass,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub axis: GreenAxis,
    pub source: FactSource,
    pub proof: ProofStrength,
    pub outcome: EvidenceOutcome,
    pub confidence: f32,
    pub fingerprint: String,
    pub observed_at: String,
    pub producer: String,
    pub snapshot_id: SnapshotId,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedEvidence {
    pub entity_id: EntityId,
    pub evidence: Evidence,
}
