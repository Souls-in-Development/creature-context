use serde::{Deserialize, Serialize};

use crate::{EventId, PermissionId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritySource {
    System,
    Human,
    Project,
    Tool,
    Model,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AuthorityMode {
    Observe,
    Maintain,
    Prepare,
    Act,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PermissionAction {
    Read,
    Index,
    Enrich,
    WriteContext,
    WriteSource,
    Move,
    Delete,
    Execute,
    Transmit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PermissionScope {
    Once,
    Session,
    Project,
    Ongoing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalOverlay {
    OrbitPrepared,
    DelegatedAuthority,
    NewlyDiscovered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionRule {
    pub id: PermissionId,
    pub subject: String,
    pub action: PermissionAction,
    pub resource: String,
    pub scope: PermissionScope,
    pub decision: PermissionDecision,
    pub authority_source: AuthoritySource,
    pub created_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub supersedes: Option<PermissionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionUse {
    pub id: EventId,
    pub permission_id: PermissionId,
    pub action_fingerprint: String,
    pub used_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionSupersession {
    pub id: EventId,
    pub superseded: PermissionId,
    pub replacement: PermissionId,
    pub recorded_at: String,
    pub authority_source: AuthoritySource,
}
