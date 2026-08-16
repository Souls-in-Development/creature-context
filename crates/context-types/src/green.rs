use crate::{Evidence, ProofStrength, ScopeScale, SnapshotId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GreenCode {
    Red,
    Unknown,
    Yellow,
    Green,
}

impl GreenCode {
    pub const fn short(self) -> char {
        match self {
            Self::Unknown => 'U',
            Self::Red => 'R',
            Self::Yellow => 'Y',
            Self::Green => 'G',
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GreenAxis {
    Content,
    Structure,
    Integration,
    Verification,
    Freshness,
    Coherence,
}

impl GreenAxis {
    pub const ALL: [Self; 6] = [
        Self::Content,
        Self::Structure,
        Self::Integration,
        Self::Verification,
        Self::Freshness,
        Self::Coherence,
    ];
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AxisAssessment {
    pub code: GreenCode,
    pub required_proof: ProofStrength,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GreenAssessment {
    pub overall: GreenCode,
    pub axes: BTreeMap<GreenAxis, AxisAssessment>,
    pub snapshot_id: SnapshotId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GreenPolicy {
    pub proof_floors: BTreeMap<ScopeScale, BTreeMap<GreenAxis, ProofStrength>>,
}

impl Default for GreenPolicy {
    fn default() -> Self {
        let mut proof_floors = BTreeMap::new();
        for scale in [
            ScopeScale::Universe,
            ScopeScale::Galaxy,
            ScopeScale::System,
            ScopeScale::Planet,
            ScopeScale::Moon,
        ] {
            let mut axes = BTreeMap::new();
            for axis in GreenAxis::ALL {
                axes.insert(axis, ProofStrength::Metadata);
            }
            proof_floors.insert(scale, axes);
        }
        Self { proof_floors }
    }
}
