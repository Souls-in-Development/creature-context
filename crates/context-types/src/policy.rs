use crate::{
    ScopeScale, authority::AuthorityMode, context::PrivacyClass, evidence::ProofStrength,
    green::GreenAxis,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortabilityPolicy {
    pub commit_journals: bool,
    pub commit_permissions: bool,
    pub encrypted_sync: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyPolicy {
    pub source_ceiling: PrivacyClass,
    pub portable_ceiling: PrivacyClass,
    pub allow_transmission: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRoutingPolicy {
    pub rules_only: bool,
    pub allowed_provider_ids: BTreeSet<String>,
    pub allowed_endpoints: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionPolicy {
    pub minimum_confidence: f32,
    pub maximum_candidate_age_seconds: u64,
    pub human_review_for_protected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPolicy {
    pub schema_version: u32,
    pub authority_mode: AuthorityMode,
    pub compatibility_yaml: bool,
    pub portability: PortabilityPolicy,
    pub privacy: PrivacyPolicy,
    pub freshness: BTreeMap<String, u64>,
    pub redaction_patterns: Vec<String>,
    pub green_proof_floors: BTreeMap<ScopeScale, BTreeMap<GreenAxis, ProofStrength>>,
    pub model: ModelRoutingPolicy,
    pub admission: AdmissionPolicy,
    pub metadata_projections: BTreeSet<String>,
}

impl Default for ProjectPolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            authority_mode: AuthorityMode::Maintain,
            compatibility_yaml: false,
            portability: PortabilityPolicy {
                commit_journals: false,
                commit_permissions: false,
                encrypted_sync: false,
            },
            privacy: PrivacyPolicy {
                source_ceiling: PrivacyClass::Project,
                portable_ceiling: PrivacyClass::Project,
                allow_transmission: false,
            },
            freshness: BTreeMap::new(),
            redaction_patterns: vec![],
            green_proof_floors: BTreeMap::new(),
            model: ModelRoutingPolicy {
                rules_only: false,
                allowed_provider_ids: BTreeSet::new(),
                allowed_endpoints: BTreeSet::new(),
            },
            admission: AdmissionPolicy {
                minimum_confidence: 0.8,
                maximum_candidate_age_seconds: 3600,
                human_review_for_protected: true,
            },
            metadata_projections: BTreeSet::new(),
        }
    }
}
