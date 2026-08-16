//! Milestone 4 Task 5: deterministic socket unification.
//!
//! Extraction (the parser) proposes `provides`/`requires` shapes; this
//! reconciler decides fits — set intersection over shapes, decided
//! arithmetically, never inferred (specification 6.4, 7.3). The Milestone 2
//! evaluator then darkens the integration axis from the resolutions this
//! function writes; that path is exercised in `socket_green.rs`. Here we test
//! the reconciler in isolation.
//!
//! The load-bearing rule under test is that matching is by the item's
//! name, not its signature: "signature alone collides, and two functions with
//! identical signatures are routinely not interchangeable" (spec 6.4). So a
//! required shape fits a provider of the same name, is ambiguous among several,
//! and holes when nothing of that name is provided — regardless of signature.

use creature_context_core::sockets::resolve_sockets;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, AtlasSocket, EntityId, EntityKind, FitBasis, FitPlane, FitStatus,
    HoleReason, ScopeScale, SnapshotId, SocketDirection, SocketId, SocketResolution, SocketShape,
};

const SNAP: &str = "snap-socket-resolution";

fn shape(qualified_name: &str, signature: &str) -> SocketShape {
    SocketShape {
        qualified_name: qualified_name.to_string(),
        structural_signature: signature.to_string(),
        version: "1".to_string(),
        hash: format!("{qualified_name}|{signature}|1"),
    }
}

fn socket(direction: SocketDirection, owner: EntityId, shape: SocketShape) -> AtlasSocket {
    AtlasSocket {
        id: SocketId::new(),
        entity_id: owner,
        direction,
        shape,
        optional: false,
        resolution: SocketResolution::Unresolved,
        source_id: "fixture".to_string(),
        confidence: 1.0,
        observed_at: "2026-08-07T00:00:00Z".to_string(),
        snapshot_id: SnapshotId(SNAP.to_string()),
    }
}

fn entity(name: &str, sockets: Vec<AtlasSocket>) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Moon,
        kind: EntityKind::File,
        canonical_name: name.to_string(),
        aliases: vec![],
        relative_path: Some(format!("src/{name}.rs")),
        parent_id: None,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets,
        source_spans: vec![],
        structural_fingerprint: String::new(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.to_string()),
        observed_at: "2026-08-07T00:00:00Z".to_string(),
        fresh_until: None,
    }
}

fn snapshot(entities: Vec<AtlasEntity>) -> AtlasSnapshot {
    AtlasSnapshot {
        id: SnapshotId(SNAP.to_string()),
        timestamp: "2026-08-07T00:00:00Z".to_string(),
        entities,
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

/// Find the single required socket in the snapshot and return its resolution.
fn only_requires(snapshot: &AtlasSnapshot) -> SocketResolution {
    snapshot
        .entities
        .iter()
        .flat_map(|e| &e.sockets)
        .find(|s| s.direction == SocketDirection::Requires)
        .expect("a required socket")
        .resolution
        .clone()
}

#[test]
fn a_lone_provider_of_the_required_name_is_an_unconfirmed_fit() {
    let provider = EntityId::new();
    let consumer = EntityId::new();
    let mut snap = snapshot(vec![
        entity(
            "widget",
            vec![socket(
                SocketDirection::Provides,
                provider,
                shape("Widget", "struct"),
            )],
        ),
        entity(
            "consumer",
            vec![socket(
                SocketDirection::Requires,
                consumer,
                shape("crate::widget::Widget", ""),
            )],
        ),
    ]);
    resolve_sockets(&mut snap);

    match only_requires(&snap) {
        SocketResolution::Fit(fit) => {
            assert_eq!(fit.status, FitStatus::Unconfirmed, "structural, not proven");
            assert_eq!(fit.basis, FitBasis::Unique);
            assert_eq!(
                fit.plane,
                FitPlane::Inferred,
                "a structural match is inferred until a proof settles it"
            );
        }
        other => panic!("expected a unique unconfirmed fit, got {other:?}"),
    }
}

#[test]
fn nothing_of_the_required_name_is_a_no_match_hole() {
    let consumer = EntityId::new();
    let mut snap = snapshot(vec![
        entity(
            "widget",
            vec![socket(
                SocketDirection::Provides,
                EntityId::new(),
                shape("Widget", "struct"),
            )],
        ),
        entity(
            "consumer",
            vec![socket(
                SocketDirection::Requires,
                consumer,
                shape("crate::widget::Absent", ""),
            )],
        ),
    ]);
    resolve_sockets(&mut snap);

    match only_requires(&snap) {
        SocketResolution::Hole(hole) => assert_eq!(
            hole.reason,
            HoleReason::NoMatch,
            "nothing provides a shape of that name — proof of absence"
        ),
        other => panic!("expected a no_match hole, got {other:?}"),
    }
}

#[test]
fn several_providers_of_the_required_name_are_ambiguous_never_chosen() {
    let consumer = EntityId::new();
    let mut snap = snapshot(vec![
        entity(
            "a",
            vec![socket(
                SocketDirection::Provides,
                EntityId::new(),
                shape("a::Widget", "struct"),
            )],
        ),
        entity(
            "b",
            vec![socket(
                SocketDirection::Provides,
                EntityId::new(),
                shape("b::Widget", "struct"),
            )],
        ),
        entity(
            "consumer",
            vec![socket(
                SocketDirection::Requires,
                consumer,
                shape("crate::Widget", ""),
            )],
        ),
    ]);
    resolve_sockets(&mut snap);

    match only_requires(&snap) {
        SocketResolution::Hole(hole) => {
            assert_eq!(hole.reason, HoleReason::Ambiguous);
            assert_eq!(
                hole.candidates.len(),
                2,
                "both same-named providers are surfaced as candidates, and neither is chosen"
            );
        }
        other => panic!("expected an ambiguous hole with candidates, got {other:?}"),
    }
}

#[test]
fn a_same_signature_provider_of_a_different_name_does_not_fit() {
    // The defense in spec 6.4: signature alone collides. A required `Widget`
    // must not fit a provided `Gadget` of identical structure — the name is the
    // cheapest guard against wiring an internal identifier where an external one
    // belongs.
    let consumer = EntityId::new();
    let mut snap = snapshot(vec![
        entity(
            "gadget",
            vec![socket(
                SocketDirection::Provides,
                EntityId::new(),
                shape("Gadget", "struct"),
            )],
        ),
        entity(
            "consumer",
            vec![socket(
                SocketDirection::Requires,
                consumer,
                shape("crate::widget::Widget", "struct"),
            )],
        ),
    ]);
    resolve_sockets(&mut snap);

    assert!(
        matches!(only_requires(&snap), SocketResolution::Hole(hole) if hole.reason == HoleReason::NoMatch),
        "identical signature, different name: not a fit"
    );
}

#[test]
fn resolution_is_deterministic_across_runs() {
    // One snapshot with fixed socket ids, resolved twice: identical input must
    // give identical output. (Production ids are derived deterministically, so
    // the same repository always resolves the same way.)
    let base = snapshot(vec![
        entity(
            "a",
            vec![socket(
                SocketDirection::Provides,
                EntityId::new(),
                shape("a::Widget", "struct"),
            )],
        ),
        entity(
            "consumer",
            vec![socket(
                SocketDirection::Requires,
                EntityId::new(),
                shape("crate::a::Widget", ""),
            )],
        ),
    ]);
    let mut first = base.clone();
    let mut second = base.clone();
    resolve_sockets(&mut first);
    resolve_sockets(&mut second);
    assert_eq!(
        format!("{:?}", only_requires(&first)),
        format!("{:?}", only_requires(&second)),
        "the reconciler is a pure function of the shapes"
    );
}
