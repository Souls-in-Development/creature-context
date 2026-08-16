use serde::{Deserialize, Serialize};

use crate::{EntityId, RecordId, SnapshotId, authority::AuthoritySource};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRecordType {
    Purpose,
    Requirement,
    Decision,
    Constraint,
    Task,
    Question,
    Finding,
    Permission,
    Activity,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Public,
    Project,
    Private,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordState {
    Active,
    Superseded,
    Contested,
    Resolved,
}

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    File,
    Git,
    Build,
    Test,
    Issue,
    Conversation,
    Terminal,
    Agent,
    Human,
}

/// A typed provenance locator referenced by ContextRecord::source_id and by
/// canonical `@source` IDX records. The locator is portable project metadata;
/// it is never interpreted as authority by itself.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSource {
    pub id: String,
    pub kind: SourceKind,
    pub locator: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextRecord {
    pub id: RecordId,
    pub record_type: ContextRecordType,
    pub value: String,
    pub scope_id: EntityId,
    pub source_id: String,
    pub authority: AuthoritySource,
    pub confidence: f32,
    pub created_at: String,
    pub observed_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub supersedes: Vec<RecordId>,
    #[serde(default)]
    pub contradicts: Vec<RecordId>,
    pub content_hash: String,
    pub snapshot_id: SnapshotId,
    pub privacy_class: PrivacyClass,
    pub state: RecordState,
}
