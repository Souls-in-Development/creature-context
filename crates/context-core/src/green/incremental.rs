//! Incremental reconvergence — ported from creature-clean's `IncrementalUpdater`.
//!
//! A full `evaluate_snapshot` recomputes every entity. When one entity's local
//! evidence changes, most of that work is wasted: only that entity and the
//! ancestors whose rolled-up assessment folds it in can change. `reconverge`
//! recomputes exactly those, using a work-list that re-evaluates a node and, if
//! its overall assessment moved, enqueues its parent — until the finite Green
//! lattice settles. The result is identical to a full re-evaluation.
//!
//! The Swift original also propagated a change along reverse edges (a target's
//! status to its sources). Creature's rollup has no such dependency: a required
//! edge contributes its own recorded evidence, not the target entity's status,
//! and socket fits are settled ahead of time by the reconciler. So the only
//! cross-entity dependency is parent←child, and reconvergence walks ancestors.

use crate::atlas::AtlasHierarchy;
use crate::green::evaluator::evaluate_entity;
use creature_context_types::{
    AtlasEdge, AtlasSnapshot, EntityId, GreenAssessment, GreenPolicy, green::GreenCode,
};
use std::collections::{BTreeSet, HashMap, VecDeque};

/// Recompute the assessments affected by a change to `changed` entities' local
/// evidence, in place. Returns the set of entities whose *overall* assessment
/// moved (a superset of nothing and a subset of the changed entities plus their
/// ancestors). If the hierarchy does not validate, returns an empty set and
/// leaves the snapshot untouched — the caller should fall back to a full
/// `evaluate_snapshot`.
pub fn reconverge(
    snapshot: &mut AtlasSnapshot,
    changed: &[EntityId],
    policy: &GreenPolicy,
) -> BTreeSet<EntityId> {
    let active = snapshot.id.clone();
    let Ok(hierarchy) = AtlasHierarchy::from_entities(&snapshot.entities) else {
        return BTreeSet::new();
    };

    // Owned cache of each entity's current rolled-up code, updated as we go so a
    // parent re-evaluation sees its children's fresh values.
    let mut overall: HashMap<EntityId, GreenCode> = snapshot
        .entities
        .iter()
        .filter_map(|e| e.green.as_ref().map(|g| (e.id, g.overall)))
        .collect();

    let mut updates: HashMap<EntityId, GreenAssessment> = HashMap::new();
    let mut moved: BTreeSet<EntityId> = BTreeSet::new();
    let mut queue: VecDeque<EntityId> = changed.iter().copied().collect();

    while let Some(id) = queue.pop_front() {
        let Some(entity) = hierarchy.entity(id) else {
            continue;
        };
        let child_states: Vec<GreenCode> = hierarchy
            .children_of(id)
            .into_iter()
            .filter_map(|child| overall.get(&child.id).copied())
            .collect();
        let required_edges: Vec<&AtlasEdge> = snapshot
            .edges
            .iter()
            .filter(|edge| edge.source_entity_id == id && edge.required)
            .collect();

        let assessment = evaluate_entity(
            entity,
            &active,
            policy,
            &child_states,
            &required_edges,
            &snapshot.conflicts,
        );
        let new_overall = assessment.overall;
        let changed_here = overall.get(&id) != Some(&new_overall);
        overall.insert(id, new_overall);
        updates.insert(id, assessment);

        // Only a moved rolled-up value can affect the parent; a recompute that
        // lands on the same code stops here, exactly as the Swift work-list did.
        if changed_here {
            moved.insert(id);
            if let Some(parent) = entity.parent_id {
                queue.push_back(parent);
            }
        }
    }

    drop(hierarchy);
    for entity in &mut snapshot.entities {
        if let Some(assessment) = updates.remove(&entity.id) {
            entity.green = Some(assessment);
        }
    }
    moved
}
