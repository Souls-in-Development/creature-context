//! Task 2: the IDX encoder must emit the complete record set from
//! specification 5.2, with a real legend, correct escaping, and no hardcoded
//! identity or timestamp.

use creature_context_store::{IdxScope, encode_atlas_idx};
use creature_context_types::{
    AtlasSocket, ConflictId, ConflictRecord, ConflictState, Evidence, EvidenceOutcome, FactSource,
    FitBasis, FitPlane, FitStatus, HoleReason, ProjectId, ProofPathState, ProofStrength, RecordId,
    ScopeScale, SnapshotId, SocketDirection, SocketFit, SocketHole, SocketId, SocketResolution,
    SocketShape,
    authority::AuthoritySource,
    context::{
        ContextRecord, ContextRecordType, ContextSource, PrivacyClass, RecordState, SourceKind,
    },
    green::{AxisAssessment, GreenAssessment, GreenAxis, GreenCode},
};
use std::collections::BTreeMap;

mod support;
use support::{SNAPSHOT, entity, snapshot_with};

fn project() -> ProjectId {
    ProjectId::new()
}

#[test]
fn header_carries_real_snapshot_and_project_identity() {
    let id = project();
    let out = encode_atlas_idx(&snapshot_with(vec![]), IdxScope::Galaxy, &id).expect("encode");
    let header = out.lines().next().expect("header");

    assert!(
        header.starts_with("@creature-context v:1 kind:atlas scale:galaxy"),
        "unexpected header: {header}"
    );
    assert!(
        header.contains(&format!("snapshot:{SNAPSHOT}")),
        "header must carry the real snapshot id: {header}"
    );
    assert!(
        header.contains(&id.to_string()),
        "header must carry the real project id: {header}"
    );
    assert!(
        !header.contains("project:default"),
        "project identity must come from the parameter, not a hardcoded default: {header}"
    );
}

#[test]
fn generated_line_uses_the_snapshot_timestamp() {
    let out =
        encode_atlas_idx(&snapshot_with(vec![]), IdxScope::Galaxy, &project()).expect("encode");
    let generated = out
        .lines()
        .find(|l| l.starts_with("@generated"))
        .expect("generated line");

    assert!(
        generated.contains("2026-08-04T00:00:00Z"),
        "must use the snapshot's timestamp, not a hardcoded date: {generated}"
    );
}

#[test]
fn legend_defines_every_abbreviation_used() {
    let out = encode_atlas_idx(
        &snapshot_with(vec![entity("a.rs", "src/a.rs", ScopeScale::Moon)]),
        IdxScope::Galaxy,
        &project(),
    )
    .expect("encode");

    let legend = out
        .lines()
        .find(|l| l.starts_with("@legend"))
        .expect("legend line");

    assert_ne!(
        legend.trim(),
        "@legend ...",
        "legend must not be a placeholder"
    );
    for axis in ["C", "S", "I", "V", "F", "H"] {
        assert!(
            legend.contains(axis),
            "legend must define Green axis {axis}: {legend}"
        );
    }
}

#[test]
fn fields_with_spaces_are_json_escaped() {
    let e = entity(
        "name with spaces",
        "src/dir with space/a.rs",
        ScopeScale::Moon,
    );
    let out =
        encode_atlas_idx(&snapshot_with(vec![e]), IdxScope::Galaxy, &project()).expect("encode");

    let line = out
        .lines()
        .find(|l| l.starts_with("@entity"))
        .expect("entity line");

    assert!(
        line.contains(r#"name:"name with spaces""#),
        "escaped name missing: {line}"
    );
    assert!(
        line.contains(r#"path:"src/dir with space/a.rs""#),
        "escaped path missing: {line}"
    );
    assert_eq!(out.lines().filter(|l| l.starts_with("@entity")).count(), 1);
}

#[test]
fn absent_optional_values_are_omitted_not_rendered_null() {
    // Specification 5.1: absent optional values are omitted rather than
    // represented by an ambiguous atom. The shipped encoder emits `path:null`.
    let mut e = entity("Local Universe", "unused", ScopeScale::Universe);
    e.relative_path = None;

    let out =
        encode_atlas_idx(&snapshot_with(vec![e]), IdxScope::Galaxy, &project()).expect("encode");
    let line = out
        .lines()
        .find(|l| l.starts_with("@entity"))
        .expect("entity line");

    assert!(
        !line.contains("null"),
        "absent optional fields must be omitted, never rendered as null: {line}"
    );
    assert!(
        !line.contains("path:"),
        "an entity with no path must emit no path field: {line}"
    );
}

#[test]
fn green_axes_render_as_single_character_codes() {
    let mut axes = BTreeMap::new();
    for axis in GreenAxis::ALL {
        axes.insert(
            axis,
            AxisAssessment {
                code: GreenCode::Yellow,
                required_proof: ProofStrength::Unknown,
                evidence: vec![],
                reasons: vec![],
            },
        );
    }

    let mut e = entity("a.rs", "src/a.rs", ScopeScale::Moon);
    e.green = Some(GreenAssessment {
        overall: GreenCode::Yellow,
        axes,
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
    });

    let out =
        encode_atlas_idx(&snapshot_with(vec![e]), IdxScope::Galaxy, &project()).expect("encode");
    let line = out
        .lines()
        .find(|l| l.starts_with("@green"))
        .expect("no @green record emitted");

    for axis in ["C", "S", "I", "V", "F", "H"] {
        assert!(
            line.contains(&format!("{axis}:Y")),
            "axis {axis} must render as a single-character code: {line}"
        );
    }
    assert!(line.contains("overall:Y"), "{line}");
}

#[test]
fn output_is_byte_stable_for_the_same_snapshot() {
    let id = project();
    let s = snapshot_with(vec![
        entity("b.rs", "src/b.rs", ScopeScale::Moon),
        entity("a.rs", "src/a.rs", ScopeScale::Moon),
    ]);

    let first = encode_atlas_idx(&s, IdxScope::Galaxy, &id).expect("encode");
    let second = encode_atlas_idx(&s, IdxScope::Galaxy, &id).expect("encode");

    assert_eq!(first, second, "same snapshot must produce identical bytes");
    assert!(!first.contains('\r'), "output must use LF endings only");
}

fn socket(entity_id: creature_context_types::EntityId, direction: SocketDirection) -> AtlasSocket {
    AtlasSocket {
        id: SocketId::new(),
        entity_id,
        direction,
        shape: SocketShape {
            qualified_name: "payments::Authorizer".to_string(),
            structural_signature: "fn authorize(Request) -> Result<Grant, Error>".to_string(),
            version: "1".to_string(),
            hash: "shape-authorizer-v1".to_string(),
        },
        optional: false,
        resolution: SocketResolution::Unresolved,
        source_id: "src/payments.rs:12".to_string(),
        confidence: 1.0,
        observed_at: "2026-08-04T00:00:00Z".to_string(),
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
    }
}

#[test]
fn sockets_fits_and_proof_paths_are_canonical_records() {
    let mut provider = entity("provider.rs", "src/provider.rs", ScopeScale::Moon);
    let provided = socket(provider.id, SocketDirection::Provides);
    let provided_id = provided.id;
    provider.sockets.push(provided);

    let mut consumer = entity("consumer.rs", "src/consumer.rs", ScopeScale::Moon);
    let mut required = socket(consumer.id, SocketDirection::Requires);
    required.resolution = SocketResolution::Fit(SocketFit {
        provided_socket_id: provided_id,
        basis: FitBasis::Unique,
        status: FitStatus::Unconfirmed,
        checked_by: None,
        proof_path: ProofPathState::Unavailable,
        plane: FitPlane::Inferred,
        confidence: 0.92,
    });
    let required_id = required.id;
    consumer.sockets.push(required);

    let out = encode_atlas_idx(
        &snapshot_with(vec![provider, consumer]),
        IdxScope::Galaxy,
        &project(),
    )
    .expect("encode");

    let fit = out
        .lines()
        .find(|line| line.starts_with("@fit"))
        .expect("fit record");
    assert!(fit.contains(&format!("require:{required_id}")), "{fit}");
    assert!(fit.contains(&format!("provide:{provided_id}")), "{fit}");
    assert!(fit.contains("status:unconfirmed"), "{fit}");
    assert!(fit.contains("checked_by:none"), "{fit}");
    assert!(fit.contains("proof_path:unavailable"), "{fit}");
    assert!(fit.contains("plane:inferred"), "{fit}");
}

#[test]
fn ambiguous_requirements_render_as_holes_without_choosing_a_fit() {
    let mut consumer = entity("consumer.rs", "src/consumer.rs", ScopeScale::Moon);
    let mut required = socket(consumer.id, SocketDirection::Requires);
    let candidate_a = SocketId::new();
    let candidate_b = SocketId::new();
    required.resolution = SocketResolution::Hole(SocketHole {
        reason: HoleReason::Ambiguous,
        candidates: vec![candidate_a, candidate_b],
        adapter_target: false,
    });
    consumer.sockets.push(required);

    let out = encode_atlas_idx(&snapshot_with(vec![consumer]), IdxScope::Galaxy, &project())
        .expect("encode");

    let hole = out
        .lines()
        .find(|line| line.starts_with("@hole"))
        .expect("hole record");
    assert!(hole.contains("reason:ambiguous"), "{hole}");
    assert!(hole.contains(&candidate_a.to_string()), "{hole}");
    assert!(hole.contains(&candidate_b.to_string()), "{hole}");
    assert!(!out.lines().any(|line| line.starts_with("@fit")));
}

fn context_record(
    record_type: ContextRecordType,
    scope_id: creature_context_types::EntityId,
    value: &str,
) -> ContextRecord {
    ContextRecord {
        id: RecordId::new(),
        record_type,
        value: value.to_string(),
        scope_id,
        source_id: "source-doc".to_string(),
        authority: AuthoritySource::Human,
        confidence: 1.0,
        created_at: "2026-08-04T00:00:00Z".to_string(),
        observed_at: "2026-08-04T00:00:00Z".to_string(),
        expires_at: None,
        supersedes: vec![],
        contradicts: vec![],
        content_hash: "record-hash".to_string(),
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
        privacy_class: PrivacyClass::Project,
        state: RecordState::Active,
    }
}

fn evidence(producer: &str) -> Evidence {
    Evidence {
        axis: GreenAxis::Integration,
        source: FactSource::Observed,
        proof: ProofStrength::Test,
        outcome: EvidenceOutcome::Pass,
        confidence: 1.0,
        fingerprint: format!("fingerprint-{producer}"),
        observed_at: "2026-08-04T00:00:00Z".to_string(),
        producer: producer.to_string(),
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
        message: String::new(),
    }
}

#[test]
fn complete_record_set_uses_canonical_type_order() {
    let mut e = entity("module", "src", ScopeScale::System);
    e.deterministic_summary = "Module summary".to_string();
    e.local_evidence.push(evidence("local-check"));
    let entity_id = e.id;

    let mut snapshot = snapshot_with(vec![e]);
    snapshot.records = vec![
        context_record(ContextRecordType::Task, entity_id, "finish encoder"),
        context_record(ContextRecordType::Constraint, entity_id, "stay portable"),
        context_record(ContextRecordType::Decision, entity_id, "use IDX"),
        context_record(ContextRecordType::Purpose, entity_id, "orient agents"),
    ];
    let conflict = ConflictRecord {
        id: ConflictId::new(),
        left_record_id: snapshot.records[1].id,
        right_record_id: snapshot.records[2].id,
        state: ConflictState::Open,
        severity: GreenCode::Yellow,
        resolution_record_id: None,
        created_at: "2026-08-04T00:00:00Z".to_string(),
        snapshot_id: SnapshotId(SNAPSHOT.to_string()),
    };
    snapshot.conflicts.push(conflict);
    snapshot.sources.push(ContextSource {
        id: "source-doc".to_string(),
        kind: SourceKind::File,
        locator: "docs/design notes.md".to_string(),
    });

    let out = encode_atlas_idx(&snapshot, IdxScope::Galaxy, &project()).expect("encode");
    let tags: Vec<&str> = out
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|tag| {
            matches!(
                *tag,
                "@purpose"
                    | "@entity"
                    | "@summary"
                    | "@decision"
                    | "@constraint"
                    | "@task"
                    | "@evidence"
                    | "@conflict"
                    | "@source"
            )
        })
        .collect();

    assert_eq!(
        tags,
        vec![
            "@purpose",
            "@entity",
            "@summary",
            "@decision",
            "@constraint",
            "@task",
            "@evidence",
            "@conflict",
            "@source",
        ]
    );
    assert!(out.contains("authority:human"), "{out}");
    assert!(out.contains(r#"locator:"docs/design notes.md""#), "{out}");
}

#[test]
fn folder_scope_emits_direct_child_pointer() {
    let root = entity("root", ".", ScopeScale::Galaxy);
    let mut child = entity("src", "src", ScopeScale::System);
    child.parent_id = Some(root.id);
    child.deterministic_summary = "Source modules".to_string();
    let mut grandchild = entity("feature", "src/feature", ScopeScale::Planet);
    grandchild.parent_id = Some(child.id);

    let snapshot = snapshot_with(vec![root.clone(), child.clone(), grandchild]);
    let out = encode_atlas_idx(&snapshot, IdxScope::Folder(root.id), &project()).expect("encode");
    let children: Vec<_> = out
        .lines()
        .filter(|line| line.starts_with("@child"))
        .collect();

    assert_eq!(children.len(), 1, "only direct children belong here: {out}");
    assert!(children[0].contains(&format!("id:{}", child.id)));
    assert!(children[0].contains("path:src/ATLAS.idx"));
    assert!(children[0].contains("summary:\"Source modules\""));
}

#[test]
fn edge_proofs_and_all_evidence_are_emitted() {
    let mut left = entity("left", "src/left.rs", ScopeScale::Moon);
    let right = entity("right", "src/right.rs", ScopeScale::Moon);
    left.local_evidence.push(evidence("local"));
    left.inherited_evidence.push(evidence("inherited"));
    let mut edge = support::edge(
        &left,
        &right,
        creature_context_types::RelationshipKind::Calls,
    );
    edge.proof_record_ids.push(RecordId::new());
    edge.evidence.push(evidence("edge"));

    let mut snapshot = snapshot_with(vec![left, right]);
    snapshot.edges.push(edge);
    let out = encode_atlas_idx(&snapshot, IdxScope::Galaxy, &project()).expect("encode");
    let edge_line = out
        .lines()
        .find(|line| line.starts_with("@edge"))
        .expect("edge record");
    let evidence_lines: Vec<_> = out
        .lines()
        .filter(|line| line.starts_with("@evidence"))
        .collect();

    assert!(edge_line.contains("proof:["), "{edge_line}");
    assert_eq!(evidence_lines.len(), 3, "{out}");
    assert!(evidence_lines.iter().all(|line| line.contains(" id:")));
}

#[test]
fn folder_scope_excludes_sibling_records_and_sources() {
    let root = entity("root", ".", ScopeScale::Galaxy);
    let mut left = entity("left", "src/left", ScopeScale::System);
    left.parent_id = Some(root.id);
    let mut right = entity("right", "src/right", ScopeScale::System);
    right.parent_id = Some(root.id);

    let mut snapshot = snapshot_with(vec![root, left.clone(), right.clone()]);
    snapshot.records = vec![
        context_record(ContextRecordType::Task, left.id, "left-only task"),
        context_record(ContextRecordType::Task, right.id, "right-only task"),
    ];
    snapshot.records[0].source_id = "left-source".to_string();
    snapshot.records[1].source_id = "right-source".to_string();
    snapshot.sources = vec![
        ContextSource {
            id: "left-source".to_string(),
            kind: SourceKind::File,
            locator: "src/left/task.md".to_string(),
        },
        ContextSource {
            id: "right-source".to_string(),
            kind: SourceKind::File,
            locator: "src/right/task.md".to_string(),
        },
    ];

    let out = encode_atlas_idx(&snapshot, IdxScope::Folder(left.id), &project()).expect("encode");

    assert!(out.contains("left-only task"), "{out}");
    assert!(out.contains("left-source"), "{out}");
    assert!(!out.contains("right-only task"), "{out}");
    assert!(!out.contains("right-source"), "{out}");
}

#[test]
fn identical_inherited_evidence_is_emitted_once() {
    let mut e = entity("module", "src/module", ScopeScale::System);
    let shared = evidence("shared-check");
    e.local_evidence.push(shared.clone());
    e.inherited_evidence.push(shared);

    let out =
        encode_atlas_idx(&snapshot_with(vec![e]), IdxScope::Galaxy, &project()).expect("encode");

    assert_eq!(
        out.lines()
            .filter(|line| line.starts_with("@evidence"))
            .count(),
        1,
        "the same evidence claim must not be duplicated: {out}"
    );
}
