use creature_context_store::JournalStore;
use creature_context_types::{
    EntityId, EventId, RecordId, SnapshotId,
    activity::{ActivityEvent, ActivityKind},
    authority::AuthoritySource,
    context::{ContextRecord, ContextRecordType, PrivacyClass, RecordState},
};

/// Verifies that the journal accepts appends. It deliberately does **not**
/// claim to round-trip.
///
/// `JournalStore` is currently append-only: it exposes `open`, `in_memory`,
/// `append_activity` and `append_record`, and no read or replay API. Nothing
/// written here can be read back, so persistence cannot be asserted.
///
/// That is a Milestone 2 prerequisite, not a test defect. Specification 4.2
/// lists `journal.jsonl` as portable truth and 17 requires rebuilding the
/// database from it after corruption; a write-only store satisfies neither.
/// When replay lands, this becomes a genuine round-trip test.
#[test]
fn journal_accepts_appended_activity_and_records() {
    let mut store = JournalStore::in_memory().expect("open in-memory journal");
    let activity = ActivityEvent {
        id: EventId::new(),
        project_id: EntityId::new(),
        galaxy_id: EntityId::new(),
        kind: ActivityKind::FileAdded,
        source_locator: "src/main.rs".to_string(),
        observed_at: "2026-08-03T00:00:00Z".to_string(),
        snapshot_id: SnapshotId("snapshot-1".to_string()),
        privacy_class: PrivacyClass::Project,
        payload: serde_json::json!({"path": "src/main.rs"}),
    };
    let appended = store.append_activity(&activity);
    assert!(appended.is_ok(), "append_activity failed: {appended:?}");

    let record = ContextRecord {
        id: RecordId::new(),
        record_type: ContextRecordType::Decision,
        value: "Test Decision".to_string(),
        scope_id: EntityId::new(),
        source_id: "test".to_string(),
        authority: AuthoritySource::Human,
        confidence: 1.0,
        created_at: "2026-08-03T00:00:00Z".to_string(),
        observed_at: "2026-08-03T00:00:00Z".to_string(),
        expires_at: None,
        supersedes: vec![],
        contradicts: vec![],
        content_hash: "hash".to_string(),
        snapshot_id: SnapshotId("snapshot-1".to_string()),
        privacy_class: PrivacyClass::Project,
        state: RecordState::Active,
    };
    let appended = store.append_record(&record);
    assert!(appended.is_ok(), "append_record failed: {appended:?}");
}
