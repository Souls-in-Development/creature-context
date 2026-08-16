use std::collections::{BTreeMap, BTreeSet};

use creature_context_types::{
    AtlasSnapshot, CandidateId, ConflictId, ConflictRecord, ConflictState, EntityId, EventId,
    GreenAxis, GreenCode, InferredPolicy, OrbitMode, OrbitPacket, OrbitRequest, OrbitScale,
    PermissionId, ProofStrength, RecordId, SnapshotId, SnapshotPreference,
    activity::{ActivityEvent, ActivityKind},
    authority::{
        AuthoritySource, PermissionAction, PermissionDecision, PermissionRule, PermissionScope,
    },
    context::{ContextRecord, ContextRecordType, PrivacyClass, RecordState},
    model::{
        CandidatePayload, CandidateRecord, CandidateState, CapabilityProfile, CapabilityState,
        ModelRole,
    },
};

#[test]
fn test_product_contracts_round_trip() {
    // 1. ContextRecord
    let context_record = ContextRecord {
        id: RecordId::new(),
        record_type: ContextRecordType::Decision,
        value: "Use Rust 2024".to_string(),
        scope_id: EntityId::new(),
        source_id: "docs/architecture.md".to_string(),
        authority: AuthoritySource::Human,
        confidence: 1.0,
        created_at: "2026-08-03T00:00:00Z".to_string(),
        observed_at: "2026-08-03T00:00:00Z".to_string(),
        expires_at: None,
        supersedes: vec![],
        contradicts: vec![],
        content_hash: "abcd".to_string(),
        snapshot_id: SnapshotId("snap1".to_string()),
        privacy_class: PrivacyClass::Project,
        state: RecordState::Active,
    };

    let json = serde_json::to_string(&context_record).unwrap();
    let _: ContextRecord = serde_json::from_str(&json).unwrap();
    assert!(
        serde_json::from_str::<ContextRecord>(&json.replace("}", ",\"unknown_field\":1}")).is_err()
    );

    // 2. PermissionRule
    let permission_rule = PermissionRule {
        id: PermissionId::new(),
        subject: "user:example".to_string(),
        action: PermissionAction::WriteContext,
        resource: "/workspace/example-project".to_string(),
        scope: PermissionScope::Ongoing,
        decision: PermissionDecision::Allow,
        authority_source: AuthoritySource::Human,
        created_at: "2026-08-03T00:00:00Z".to_string(),
        expires_at: None,
        supersedes: None,
    };

    let json = serde_json::to_string(&permission_rule).unwrap();
    let _: PermissionRule = serde_json::from_str(&json).unwrap();
    assert!(
        serde_json::from_str::<PermissionRule>(&json.replace("}", ",\"unknown_field\":1}"))
            .is_err()
    );

    // 3. ActivityEvent
    let activity_event = ActivityEvent {
        id: EventId::new(),
        project_id: EntityId::new(),
        galaxy_id: EntityId::new(),
        kind: ActivityKind::FileModified,
        source_locator: "src/main.rs".to_string(),
        observed_at: "2026-08-03T00:00:00Z".to_string(),
        snapshot_id: SnapshotId("snap1".to_string()),
        privacy_class: PrivacyClass::Project,
        payload: serde_json::json!({"hash": "1234"}),
    };

    let json = serde_json::to_string(&activity_event).unwrap();
    let _: ActivityEvent = serde_json::from_str(&json).unwrap();
    assert!(
        serde_json::from_str::<ActivityEvent>(&json.replace("}", ",\"unknown_field\":1}")).is_err()
    );

    // 4. CandidateRecord
    let candidate_record = CandidateRecord {
        id: CandidateId::new(),
        payload: CandidatePayload::Context(context_record.clone()),
        provider_id: "local".to_string(),
        model_id: "llama3".to_string(),
        capability_profile_id: "llama3-profile".to_string(),
        schema_version: 1,
        state: CandidateState::Pending,
        rejection_reasons: vec![],
        created_at: "2026-08-03T00:00:00Z".to_string(),
        snapshot_id: SnapshotId("snap1".to_string()),
    };

    let json = serde_json::to_string(&candidate_record).unwrap();
    let _: CandidateRecord = serde_json::from_str(&json).unwrap();
    assert!(
        serde_json::from_str::<CandidateRecord>(&json.replace("}", ",\"unknown_field\":1}"))
            .is_err()
    );

    // 5. CapabilityProfile
    let capability_profile = CapabilityProfile {
        id: "llama3-profile".to_string(),
        provider_id: "local".to_string(),
        model_id: "llama3".to_string(),
        state: CapabilityState::Verified,
        privacy_class: PrivacyClass::Project,
        role_scores: {
            let mut m = BTreeMap::new();
            m.insert(ModelRole::Contextual, 0.9);
            m
        },
        structured_output_rate: 0.95,
        attribution_rate: 0.85,
        p95_latency_ms: 1200,
        measured_input_limit: 8192,
        measured_output_limit: 1024,
        memory_mib: 4096,
        storage_mib: 4096,
        tested_languages: {
            let mut s = BTreeSet::new();
            s.insert("rust".to_string());
            s
        },
        calibration_version: "1.0".to_string(),
        calibrated_at: "2026-08-03T00:00:00Z".to_string(),
        evidence_locator: None,
    };

    let json = serde_json::to_string(&capability_profile).unwrap();
    let _: CapabilityProfile = serde_json::from_str(&json).unwrap();
    assert!(
        serde_json::from_str::<CapabilityProfile>(&json.replace("}", ",\"unknown_field\":1}"))
            .is_err()
    );

    // 6. AtlasSnapshot containing context/conflict records
    let snapshot = AtlasSnapshot {
        id: SnapshotId("snap1".to_string()),
        timestamp: "2026-08-03T00:00:00Z".to_string(),
        entities: vec![],
        edges: vec![],
        records: vec![context_record],
        conflicts: vec![ConflictRecord {
            id: ConflictId::new(),
            left_record_id: RecordId::new(),
            right_record_id: RecordId::new(),
            state: ConflictState::Open,
            severity: GreenCode::Red,
            resolution_record_id: None,
            created_at: "2026-08-03T00:00:00Z".to_string(),
            snapshot_id: SnapshotId("snap1".to_string()),
        }],
        sources: vec![],
    };

    let json = serde_json::to_string(&snapshot).unwrap();
    let _: AtlasSnapshot = serde_json::from_str(&json).unwrap();
    // AtlasSnapshot shouldn't accept unknown fields (except version-zero adapter structs which we handle)
    assert!(
        serde_json::from_str::<AtlasSnapshot>(&json.replace("}", ",\"unknown_field\":1}")).is_err()
    );

    // 7. GreenAxis::Coherence
    let coherence = GreenAxis::Coherence;
    let json = serde_json::to_string(&coherence).unwrap();
    let parsed: GreenAxis = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, GreenAxis::Coherence);

    // 8. OrbitPacket containing omission counts
    let packet = OrbitPacket {
        id: "orbit-1".to_string(),
        scale: OrbitScale::Galaxy,
        mode: OrbitMode::Focus,
        task: "Describe architecture".to_string(),
        architectural_spine: vec![],
        selected_entities: vec![],
        comparison: None,
        uncertainty: vec![],
        selection_reasons: vec![],
        estimated_total_tokens: 1000,
        budget: 50000,
        request: OrbitRequest {
            task: "Describe architecture".to_string(),
            target_references: vec![],
            exclusions: vec![],
            scale: OrbitScale::Galaxy,
            mode: OrbitMode::Focus,
            comparison_dimensions: vec![],
            snapshot_preference: SnapshotPreference::Current,
            token_budget: 50000,
            maximum_graph_depth: 5,
            required_proof_floor: ProofStrength::Human,
            inferred_policy: InferredPolicy::PreferDeterministic,
            privacy_ceiling: PrivacyClass::Project,
            client_id: None,
            session_id: None,
        },
        resolved_references: vec![],
        context_records: vec![],
        conflicts: vec![],
        relationships: vec![],
        omission_counts: {
            let mut m = BTreeMap::new();
            m.insert("ContextRecord".to_string(), 5);
            m
        },
        minimum_required_tokens: None,
    };

    let json = serde_json::to_string(&packet).unwrap();
    let _: OrbitPacket = serde_json::from_str(&json).unwrap();
    assert!(
        serde_json::from_str::<OrbitPacket>(&json.replace("}", ",\"unknown_field\":1}")).is_err()
    );
}
