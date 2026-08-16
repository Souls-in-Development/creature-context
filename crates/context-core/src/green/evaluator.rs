use crate::atlas::AtlasHierarchy;
use creature_context_types::*;
use std::collections::BTreeMap;

pub fn evaluate_snapshot(
    snapshot: &mut AtlasSnapshot,
    policy: &GreenPolicy,
) -> Result<(), crate::atlas::HierarchyError> {
    let hierarchy = AtlasHierarchy::from_entities(&snapshot.entities)?;
    let active = snapshot.id.clone();
    let mut assessments = BTreeMap::new();
    let mut ordered: Vec<_> = snapshot.entities.iter().map(|e| e.id).collect();
    ordered.sort_by_key(|id| {
        std::cmp::Reverse(hierarchy.entity(*id).map(|e| e.scale.rank()).unwrap_or(0))
    });

    // Group required edges by their source once, rather than rescanning every
    // edge for every entity. The per-entity filter was O(entities × edges),
    // which walls a large snapshot; this is O(edges) to build and O(1) per
    // lookup, and preserves per-source edge order because edges are visited in
    // their existing order.
    let mut required_by_source: BTreeMap<EntityId, Vec<&AtlasEdge>> = BTreeMap::new();
    for edge in &snapshot.edges {
        if edge.required {
            required_by_source
                .entry(edge.source_entity_id)
                .or_default()
                .push(edge);
        }
    }
    const NO_EDGES: &[&AtlasEdge] = &[];

    for id in ordered {
        let entity = hierarchy.entity(id).expect("validated hierarchy entity");
        let child_states: Vec<_> = hierarchy
            .children_of(id)
            .into_iter()
            .filter_map(|child| {
                assessments
                    .get(&child.id)
                    .map(|a: &GreenAssessment| a.overall)
            })
            .collect();
        let required_edges: &[&AtlasEdge] = required_by_source
            .get(&id)
            .map(Vec::as_slice)
            .unwrap_or(NO_EDGES);
        let assessment = evaluate_entity(
            entity,
            &active,
            policy,
            &child_states,
            required_edges,
            &snapshot.conflicts,
        );
        assessments.insert(id, assessment);
    }
    for entity in &mut snapshot.entities {
        entity.green = assessments.remove(&entity.id);
    }
    Ok(())
}

/// What each non-optional required socket contributes to the integration axis,
/// per specification 11.1, paired with a reason naming the socket.
///
/// Optional sockets remain visible without blocking Green, exactly as optional
/// relationships do, so they contribute nothing here.
///
/// `Unresolved` is Unknown rather than Red: matching has not run, so nothing is
/// claimed either way. Collapsing it into `NoMatch` would make an unscanned
/// project indistinguishable from a broken one.
fn socket_contributions(
    entity: &AtlasEntity,
    active: &SnapshotId,
    floor: ProofStrength,
) -> Vec<(GreenCode, String)> {
    let _ = (active, floor);
    entity
        .sockets
        .iter()
        .filter(|socket| socket.direction == SocketDirection::Requires && !socket.optional)
        .filter_map(|socket| {
            let name = &socket.shape.qualified_name;
            match &socket.resolution {
                SocketResolution::Unresolved => Some((
                    GreenCode::Unknown,
                    format!("required socket {name} has not been matched"),
                )),
                SocketResolution::Hole(hole) => match hole.reason {
                    HoleReason::NoMatch => Some((
                        GreenCode::Red,
                        format!("required socket {name} is unmatched: nothing provides its shape"),
                    )),
                    HoleReason::Ambiguous => Some((
                        GreenCode::Yellow,
                        format!(
                            "required socket {name} has {} candidates and none is selected",
                            hole.candidates.len()
                        ),
                    )),
                },
                SocketResolution::Fit(fit) => match fit.status {
                    FitStatus::Rejected => Some((
                        GreenCode::Red,
                        format!("fit for required socket {name} was proven wrong"),
                    )),
                    FitStatus::Unconfirmed => {
                        let detail = if fit.proof_path == ProofPathState::Unavailable {
                            // Distinct and more actionable than low confidence:
                            // it names where being wrong would go unnoticed.
                            format!("fit for required socket {name} is unconfirmed and has no proof path")
                        } else {
                            format!("fit for required socket {name} is unconfirmed")
                        };
                        Some((GreenCode::Yellow, detail))
                    }
                    // A confirmed fit is settled; it contributes nothing beyond
                    // the evidence already folded in above.
                    FitStatus::Confirmed => None,
                },
            }
        })
        .collect()
}

/// Evaluate one entity's Green in isolation from its own evidence, its children's
/// overall codes, the `required` edges sourced from it, and the open conflicts.
/// The assessment depends on nothing else (no ancestors, no siblings), so a caller
/// that already knows an entity's children's codes — the layered scan's folder
/// roll-up — can produce the exact assessment the whole-snapshot pass would, without
/// holding the whole snapshot.
pub fn evaluate_entity(
    entity: &AtlasEntity,
    active: &SnapshotId,
    policy: &GreenPolicy,
    child_states: &[GreenCode],
    required_edges: &[&AtlasEdge],
    conflicts: &[ConflictRecord],
) -> GreenAssessment {
    let floors = policy
        .proof_floors
        .get(&entity.scale)
        .cloned()
        .unwrap_or_default();
    let mut axes = BTreeMap::new();
    for axis in GreenAxis::ALL {
        let floor = floors
            .get(&axis)
            .copied()
            .unwrap_or(ProofStrength::Metadata);
        let relevant: Vec<_> = entity
            .local_evidence
            .iter()
            .filter(|e| e.axis == axis)
            .cloned()
            .collect();
        let mut code = if axis == GreenAxis::Freshness {
            if entity.snapshot_id == *active {
                GreenCode::Green
            } else {
                GreenCode::Unknown
            }
        } else {
            code_from_evidence(&relevant, active, floor)
        };
        let mut reasons = Vec::new();
        if axis == GreenAxis::Structure {
            for state in child_states {
                code = weakest(code, *state);
            }
            if child_states.iter().any(|s| *s != GreenCode::Green) {
                reasons.push("required child is not green".into());
            }
        }
        if axis == GreenAxis::Integration {
            for edge in required_edges {
                let edge_code = code_from_evidence(&edge.evidence, active, floor);
                code = weakest(code, edge_code);
            }
            if required_edges
                .iter()
                .any(|edge| code_from_evidence(&edge.evidence, active, floor) != GreenCode::Green)
            {
                reasons.push("required relationship lacks green evidence".into());
            }
            // Specification 11.1: an edge records a connection that exists; a
            // socket can record one that should exist and does not. Without
            // this, absence is invisible — an entity whose every required
            // socket is unmatched would assess exactly as one fully wired.
            for (socket_code, reason) in socket_contributions(entity, active, floor) {
                code = weakest(code, socket_code);
                reasons.push(reason);
            }
        }
        if axis == GreenAxis::Coherence {
            // Specification 11: H is agreement between authoritative intent,
            // structure, observation and tools. An open contradiction is a
            // recorded disagreement; it darkens H by its severity. Without this
            // an entity with an open contradiction would assess exactly as one
            // in full agreement. A model-suspected contradiction is capped at
            // Yellow at creation, so it can never redden H on its own.
            for (coherence_code, reason) in
                super::coherence::coherence_contributions(entity, conflicts)
            {
                code = weakest(code, coherence_code);
                reasons.push(reason);
            }
        }
        axes.insert(
            axis,
            AxisAssessment {
                code,
                required_proof: floor,
                evidence: relevant,
                reasons,
            },
        );
    }
    let overall = axes.values().fold(GreenCode::Green, |current, assessment| {
        weakest(current, assessment.code)
    });
    GreenAssessment {
        overall,
        axes,
        snapshot_id: active.clone(),
    }
}

fn code_from_evidence(
    evidence: &[Evidence],
    active: &SnapshotId,
    floor: ProofStrength,
) -> GreenCode {
    let fresh: Vec<_> = evidence
        .iter()
        .filter(|e| e.snapshot_id == *active)
        .collect();
    if fresh.is_empty() {
        return GreenCode::Unknown;
    }
    if fresh.iter().any(|e| e.outcome == EvidenceOutcome::Fail) {
        return GreenCode::Red;
    }
    if fresh.iter().any(|e| e.outcome == EvidenceOutcome::Warning) {
        return GreenCode::Yellow;
    }
    let qualifying = fresh.iter().any(|e| {
        e.outcome == EvidenceOutcome::Pass && e.proof >= floor && e.source != FactSource::Inferred
    });
    if qualifying {
        GreenCode::Green
    } else {
        GreenCode::Unknown
    }
}

fn weakest(left: GreenCode, right: GreenCode) -> GreenCode {
    std::cmp::min(left, right)
}
