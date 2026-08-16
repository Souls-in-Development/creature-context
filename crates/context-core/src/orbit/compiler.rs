use crate::atlas::{AtlasHierarchy, HierarchyError, ModuleMap};
use crate::orbit::{
    OrbitBudgetError, compare_entities, enforce_budget, immediate_scale_contents, selected,
    zoom_roots,
};
use creature_context_types::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrbitCompileError {
    #[error(transparent)]
    Hierarchy(#[from] HierarchyError),
    #[error(transparent)]
    Budget(#[from] OrbitBudgetError),
    #[error("comparison requires valid left and right entities")]
    MissingComparisonEntities,
}

impl OrbitCompileError {
    /// The budget this request would actually need, when that is known.
    pub fn minimum_required_tokens(&self) -> Option<usize> {
        match self {
            Self::Budget(e) => e.minimum_required_tokens(),
            _ => None,
        }
    }
}

pub fn compile_orbit(
    snapshot: &AtlasSnapshot,
    request: &OrbitRequest,
) -> Result<OrbitPacket, OrbitCompileError> {
    let hierarchy = AtlasHierarchy::from_entities(&snapshot.entities)?;
    let graph = ModuleMap::from_snapshot(snapshot);
    let by_id: BTreeMap<_, _> = snapshot
        .entities
        .iter()
        .map(|entity| (entity.id, entity))
        .collect();
    let exclusions: BTreeSet<_> = request.exclusions.iter().copied().collect();
    let comparison = if request.mode == OrbitMode::Compare {
        // Find left and right entities from target_references
        let mut targets = request.target_references.iter().filter_map(|r| r.stable_id);
        let left_id = targets
            .next()
            .ok_or(OrbitCompileError::MissingComparisonEntities)?;
        let right_id = targets
            .next()
            .ok_or(OrbitCompileError::MissingComparisonEntities)?;
        let left = by_id
            .get(&left_id)
            .copied()
            .ok_or(OrbitCompileError::MissingComparisonEntities)?;
        let right = by_id
            .get(&right_id)
            .copied()
            .ok_or(OrbitCompileError::MissingComparisonEntities)?;
        Some(compare_entities(
            left,
            right,
            &request.comparison_dimensions,
        ))
    } else {
        None
    };

    let starts = choose_starts(snapshot, request);
    let roots = zoom_roots(&hierarchy, &starts, request.scale);
    let roots = if roots.is_empty() {
        // Just take the first entity if there's no root
        snapshot
            .entities
            .first()
            .map(|e| vec![e.id])
            .unwrap_or_default()
    } else {
        roots
    };

    let root_set: BTreeSet<_> = roots.iter().copied().collect();
    let (neighbour_entities, relationships, depth_map) =
        neighbourhood_with_depth(&graph, &roots, request.maximum_graph_depth, request);

    let mut selected_by_id: BTreeMap<EntityId, SelectedEntity> = BTreeMap::new();
    let mut spine_by_id: BTreeMap<EntityId, AtlasEntity> = BTreeMap::new();

    for root in &roots {
        if let Some(entity) = by_id.get(root) {
            let reason = format!(
                "ring 0: requested Orbit root at {:?} scale: {}",
                request.scale, entity.canonical_name
            );
            selected_by_id.insert(*root, selected((*entity).clone(), true, 1_000, reason, 0));
            for ancestor in hierarchy.ancestors_of(*root) {
                spine_by_id
                    .entry(ancestor.id)
                    .or_insert_with(|| ancestor.clone());
            }
            for child in immediate_scale_contents(&hierarchy, *root) {
                selected_by_id.entry(child.id).or_insert_with(|| {
                    let reason = format!(
                        "ring 2: contained within root {} at {:?} scale",
                        entity.canonical_name, request.scale
                    );
                    selected(child, false, 400, reason, 2)
                });
            }
        }
    }

    for id in neighbour_entities {
        if root_set.contains(&id) {
            continue;
        }
        if let Some(entity) = by_id.get(&id) {
            let depth = depth_map.get(&id).copied().unwrap_or(1);
            let ring = if depth == 1 { 1 } else { 2 };
            let reason = concrete_graph_reason(&graph, id, &by_id, depth, request);
            selected_by_id.entry(id).or_insert_with(|| {
                selected(
                    (*entity).clone(),
                    false,
                    250 - (depth as i64 * 20),
                    reason,
                    ring,
                )
            });
        }
    }

    let mut targets = request.target_references.iter().filter_map(|r| r.stable_id);
    if let Some(left) = targets.next().and_then(|id| by_id.get(&id).copied()) {
        let ring = if request.mode == OrbitMode::Compare {
            4
        } else {
            0
        };
        selected_by_id
            .entry(left.id)
            .or_insert_with(|| selected(left.clone(), true, 1_000, "comparison left", ring));
    }
    if let Some(right) = targets.next().and_then(|id| by_id.get(&id).copied()) {
        let ring = if request.mode == OrbitMode::Compare {
            4
        } else {
            0
        };
        selected_by_id
            .entry(right.id)
            .or_insert_with(|| selected(right.clone(), true, 1_000, "comparison right", ring));
    }
    selected_by_id.retain(|id, _| !exclusions.contains(id));
    spine_by_id.retain(|id, _| !exclusions.contains(id));

    // Promote spine entities to ring 3: Galaxy/System purpose and architecture.
    for entity in spine_by_id.values() {
        selected_by_id.entry(entity.id).or_insert_with(|| {
            let reason = format!(
                "ring 3: architectural spine containing the target ({:?})",
                entity.scale
            );
            selected(entity.clone(), false, 300, reason, 3)
        });
    }

    let mut packet = OrbitPacket {
        id: deterministic_orbit_id(snapshot, request).0.to_string(),
        scale: request.scale,
        mode: request.mode,
        task: request.task.clone(),
        architectural_spine: spine_by_id.into_values().collect(),
        selected_entities: selected_by_id.into_values().collect(),
        comparison,
        uncertainty: snapshot
            .entities
            .iter()
            .flat_map(|e| e.uncertainty.clone())
            .collect(),
        selection_reasons: vec![format!(
            "compiled {:?} {:?} Orbit from snapshot {}",
            request.scale, request.mode, snapshot.id
        )],
        estimated_total_tokens: 0,
        budget: request.token_budget,
        request: request.clone(),
        resolved_references: vec![],
        context_records: vec![],
        conflicts: vec![],
        relationships: relationships
            .into_iter()
            .map(|e| SelectedEdge {
                edge: e,
                mandatory: false,
                ring: 1,
                reasons: vec!["module-map relationship".into()],
                estimated_tokens: 0,
            })
            .collect(),
        omission_counts: BTreeMap::new(),
        minimum_required_tokens: None,
    };
    packet
        .architectural_spine
        .sort_by_key(|e| (e.scale.rank(), e.canonical_name.to_lowercase(), e.id));
    packet.selected_entities.sort_by_key(|s| {
        (
            s.ring,
            std::cmp::Reverse(s.mandatory),
            std::cmp::Reverse(s.score),
            s.entity.id,
        )
    });
    enforce_budget(&mut packet)?;
    Ok(packet)
}

fn neighbourhood_with_depth(
    graph: &ModuleMap,
    starts: &[EntityId],
    depth: usize,
    request: &OrbitRequest,
) -> (
    BTreeSet<EntityId>,
    Vec<AtlasEdge>,
    BTreeMap<EntityId, usize>,
) {
    let mut entities: BTreeSet<EntityId> = starts.iter().copied().collect();
    let mut depth_map: BTreeMap<EntityId, usize> = starts.iter().map(|&id| (id, 0)).collect();
    let mut edge_ids = BTreeSet::new();
    let mut edges = Vec::new();
    let mut queue: VecDeque<_> = starts.iter().copied().map(|id| (id, 0usize)).collect();
    let include_inferred = request.inferred_policy != InferredPolicy::Exclude;

    while let Some((id, current_depth)) = queue.pop_front() {
        if current_depth >= depth {
            continue;
        }
        for edge in graph
            .outgoing(id, include_inferred)
            .into_iter()
            .chain(graph.incoming(id, include_inferred))
        {
            if edge_ids.insert(edge.id) {
                edges.push(edge.clone());
            }
            let neighbour = if edge.source_entity_id == id {
                edge.target_entity_id
            } else {
                edge.source_entity_id
            };
            if entities.insert(neighbour) {
                depth_map.insert(neighbour, current_depth + 1);
                queue.push_back((neighbour, current_depth + 1));
            }
        }
    }
    edges.sort_by_key(|e| e.id);
    (entities, edges, depth_map)
}

fn concrete_graph_reason(
    graph: &ModuleMap,
    id: EntityId,
    by_id: &BTreeMap<EntityId, &AtlasEntity>,
    depth: usize,
    request: &OrbitRequest,
) -> String {
    let include_inferred = request.inferred_policy != InferredPolicy::Exclude;
    let edges: Vec<_> = graph
        .outgoing(id, include_inferred)
        .into_iter()
        .chain(graph.incoming(id, include_inferred))
        .take(3)
        .collect();
    if edges.is_empty() {
        return format!(
            "ring {}: connected through module map (depth {})",
            depth + 1,
            depth
        );
    }
    let descriptions: Vec<String> = edges
        .iter()
        .map(|edge| {
            let other_id = if edge.source_entity_id == id {
                edge.target_entity_id
            } else {
                edge.source_entity_id
            };
            let other_name = by_id
                .get(&other_id)
                .map(|e| e.canonical_name.as_str())
                .unwrap_or("<unknown>");
            format!("{:?} -> {}", edge.kind, other_name)
        })
        .collect();
    format!(
        "ring {}: connected through module map: {} (depth {})",
        depth + 1,
        descriptions.join(", "),
        depth
    )
}

fn deterministic_orbit_id(snapshot: &AtlasSnapshot, request: &OrbitRequest) -> OrbitId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(snapshot.id.0.as_bytes());
    if let Ok(bytes) = serde_json::to_vec(request) {
        hasher.update(&bytes);
    }
    let mut value = [0u8; 16];
    value.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    OrbitId(uuid::Uuid::from_bytes(value))
}

fn choose_starts(snapshot: &AtlasSnapshot, request: &OrbitRequest) -> Vec<EntityId> {
    if request.mode == OrbitMode::Compare {
        return request
            .target_references
            .iter()
            .filter_map(|r| r.stable_id)
            .collect();
    }
    if !request.target_references.is_empty() {
        return request
            .target_references
            .iter()
            .filter_map(|r| r.stable_id)
            .collect();
    }
    let terms: Vec<_> = request
        .task
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.len() > 1)
        .map(str::to_lowercase)
        .collect();
    let mut scored: Vec<_> = snapshot
        .entities
        .iter()
        .filter_map(|entity| {
            let haystack = format!(
                "{} {} {}",
                entity.canonical_name,
                entity.deterministic_summary,
                entity.purpose_clauses.join(" ")
            )
            .to_lowercase();
            let score = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count();
            (score > 0).then_some((std::cmp::Reverse(score), entity.id))
        })
        .collect();
    scored.sort();
    scored.into_iter().take(8).map(|(_, id)| id).collect()
}
