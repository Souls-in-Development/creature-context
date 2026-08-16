//! Task 7: unmatched required sockets must darken the integration axis.
//!
//! Before this, `crates/context-types/src/socket.rs` and the two IDX codecs
//! were the only files touching socket state — all serialization. The
//! integration branch folded in `required_edges` only, and a hole has no edge,
//! so every socket in a project could dangle without changing its assessment.
//!
//! Rules under test are specification 11.1.

use creature_context_core::green::evaluate_snapshot;
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, EntityId, EntityKind, Evidence, EvidenceOutcome, FactSource,
    FitBasis, FitPlane, FitProof, FitStatus, GreenPolicy, HoleReason, ProofPathState,
    ProofStrength, ScopeScale, SnapshotId, SocketDirection, SocketFit, SocketHole, SocketId,
    SocketResolution, SocketShape,
    green::{GreenAxis, GreenCode},
};

const SNAP: &str = "snap-socket-green";

fn shape() -> SocketShape {
    SocketShape {
        qualified_name: "payments::Authorizer".to_string(),
        structural_signature: "fn authorize(Request) -> Result<Grant, Error>".to_string(),
        version: "1".to_string(),
        hash: "shape-authorizer-v1".to_string(),
    }
}

fn entity(name: &str, scale: ScopeScale, parent: Option<EntityId>) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale,
        kind: EntityKind::File,
        canonical_name: name.to_string(),
        aliases: vec![],
        relative_path: Some(format!("src/{name}")),
        parent_id: parent,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
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
        observed_at: "2026-08-05T00:00:00Z".to_string(),
        fresh_until: None,
    }
}

fn required_socket(
    owner: EntityId,
    resolution: SocketResolution,
    optional: bool,
) -> creature_context_types::AtlasSocket {
    creature_context_types::AtlasSocket {
        id: SocketId::new(),
        entity_id: owner,
        direction: SocketDirection::Requires,
        shape: shape(),
        optional,
        resolution,
        source_id: "src/consumer.rs:12".to_string(),
        confidence: 1.0,
        observed_at: "2026-08-05T00:00:00Z".to_string(),
        snapshot_id: SnapshotId(SNAP.to_string()),
    }
}

/// Passing integration evidence, so the entity's baseline is Green and any
/// downgrade is attributable to the socket rather than to absent evidence.
/// Without this, `weakest(Unknown, Yellow)` is Unknown and a Yellow
/// contribution is masked — the test would pass while proving nothing.
fn passing_integration_evidence() -> Evidence {
    Evidence {
        axis: GreenAxis::Integration,
        source: FactSource::Observed,
        proof: ProofStrength::Test,
        outcome: EvidenceOutcome::Pass,
        confidence: 1.0,
        fingerprint: "fp".to_string(),
        observed_at: "2026-08-05T00:00:00Z".to_string(),
        producer: "cargo-test".to_string(),
        snapshot_id: SnapshotId(SNAP.to_string()),
        message: String::new(),
    }
}

fn hole(reason: HoleReason) -> SocketResolution {
    SocketResolution::Hole(SocketHole {
        reason,
        candidates: vec![],
        adapter_target: false,
    })
}

fn fit(status: FitStatus, proof_path: ProofPathState) -> SocketResolution {
    SocketResolution::Fit(SocketFit {
        provided_socket_id: SocketId::new(),
        basis: FitBasis::Unique,
        status,
        checked_by: match status {
            FitStatus::Confirmed => Some(FitProof::Build),
            _ => None,
        },
        proof_path,
        plane: FitPlane::Inferred,
        confidence: 1.0,
    })
}

/// The full containment chain — the hierarchy rejects skipped scales — so the
/// rollup has several levels to propagate through.
fn assess(resolution: SocketResolution, optional: bool) -> (GreenCode, Vec<String>, GreenCode) {
    let universe = entity("universe", ScopeScale::Universe, None);
    let galaxy = entity("galaxy", ScopeScale::Galaxy, Some(universe.id));
    let system = entity("system", ScopeScale::System, Some(galaxy.id));
    let planet = entity("planet", ScopeScale::Planet, Some(system.id));
    let mut leaf = entity("consumer.rs", ScopeScale::Moon, Some(planet.id));
    leaf.local_evidence.push(passing_integration_evidence());
    leaf.sockets
        .push(required_socket(leaf.id, resolution, optional));
    let leaf_id = leaf.id;
    let galaxy_id = galaxy.id;

    let mut snapshot = AtlasSnapshot {
        id: SnapshotId(SNAP.to_string()),
        timestamp: "2026-08-05T00:00:00Z".to_string(),
        entities: vec![universe, galaxy, system, planet, leaf],
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default()).expect("evaluate");

    let leaf = snapshot
        .entities
        .iter()
        .find(|e| e.id == leaf_id)
        .expect("leaf");
    let green = leaf.green.as_ref().expect("leaf assessment");
    let integration = &green.axes[&GreenAxis::Integration];
    let parent_overall = snapshot
        .entities
        .iter()
        .find(|e| e.id == galaxy_id)
        .and_then(|e| e.green.as_ref())
        .map(|g| g.overall)
        .expect("galaxy assessment");

    (
        integration.code,
        integration.reasons.clone(),
        parent_overall,
    )
}

#[test]
fn an_unmatched_required_socket_is_red_on_integration() {
    let (code, reasons, _) = assess(hole(HoleReason::NoMatch), false);
    assert_eq!(
        code,
        GreenCode::Red,
        "nothing provides the required shape — that is proof of absence, not missing proof"
    );
    assert!(
        reasons.iter().any(|r| r.contains("payments::Authorizer")),
        "the reason must name the socket that darkened the axis, got {reasons:?}"
    );
}

#[test]
fn an_ambiguous_socket_is_yellow_not_red() {
    let (code, _, _) = assess(hole(HoleReason::Ambiguous), false);
    assert_eq!(
        code,
        GreenCode::Yellow,
        "candidates exist and choosing among them is forbidden — that is incompleteness, \
         not a verified failure"
    );
}

#[test]
fn an_unresolved_socket_is_unknown_not_red() {
    let (code, _, _) = assess(SocketResolution::Unresolved, false);
    assert_eq!(
        code,
        GreenCode::Unknown,
        "matching has not run; an unscanned project must not be indistinguishable from a broken one"
    );
}

#[test]
fn an_unconfirmed_fit_is_yellow_and_cannot_lift_integration() {
    let (code, _, _) = assess(
        fit(FitStatus::Unconfirmed, ProofPathState::Unchecked),
        false,
    );
    assert_eq!(
        code,
        GreenCode::Yellow,
        "an unconfirmed fit is a claim about a connection, not a connection"
    );
}

#[test]
fn a_rejected_fit_is_red() {
    let (code, _, _) = assess(fit(FitStatus::Rejected, ProofPathState::Available), false);
    assert_eq!(code, GreenCode::Red, "a proof was attempted and failed");
}

#[test]
fn an_optional_socket_never_lowers_the_axis() {
    let (code, _, _) = assess(hole(HoleReason::NoMatch), true);
    assert_eq!(
        code,
        GreenCode::Green,
        "optional sockets remain visible without blocking Green, as optional relationships do"
    );
}

#[test]
fn a_dark_socket_propagates_to_the_parent() {
    // Moon -> Planet -> System -> Galaxy: three levels above the owner.
    let (_, _, galaxy) = assess(hole(HoleReason::NoMatch), false);
    assert_ne!(
        galaxy,
        GreenCode::Green,
        "weakest-required-axis rollup must carry a dark socket upward, not stop at its owner"
    );
}

#[test]
fn an_unprovable_fit_is_distinguishable_from_a_disproven_one() {
    let (unprovable, reasons, _) = assess(
        fit(FitStatus::Unconfirmed, ProofPathState::Unavailable),
        false,
    );
    let (disproven, _, _) = assess(fit(FitStatus::Rejected, ProofPathState::Available), false);

    assert_ne!(
        unprovable, disproven,
        "'nothing would tell us if this were wrong' is a different finding from 'this is wrong'"
    );
    assert!(
        reasons.iter().any(|r| r.contains("no proof path")),
        "an unprovable fit must say so — it identifies a gap in the tests, not in the code, \
         got {reasons:?}"
    );
}
