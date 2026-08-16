use crate::{EntityId, Evidence};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonDimension {
    Purpose,
    Responsibility,
    Architecture,
    Capabilities,
    Dependencies,
    Interfaces,
    Implementation,
    Verification,
    ProtectedDecisions,
    Risks,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonItem {
    pub dimension: ComparisonDimension,
    pub left: Option<String>,
    pub right: Option<String>,
    pub explanation: String,
    pub confidence: f32,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonResult {
    pub left_id: EntityId,
    pub right_id: EntityId,
    #[serde(default)]
    pub matches: Vec<ComparisonItem>,
    #[serde(default)]
    pub differences: Vec<ComparisonItem>,
    #[serde(default)]
    pub left_only: Vec<ComparisonItem>,
    #[serde(default)]
    pub right_only: Vec<ComparisonItem>,
    #[serde(default)]
    pub unresolved: Vec<String>,
}
