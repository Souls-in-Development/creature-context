use crate::atlas::AtlasHierarchy;
use creature_context_types::{AtlasEntity, EntityId, OrbitScale, ScopeScale};
use std::collections::BTreeSet;

/// A coarse match zoomed down to a fine scale must not flood the core with the
/// whole subtree. When a start would expand to more than this many descendants at
/// the target scale, it is too broad to be "the answer" — keep the start itself
/// as the root, and let the dependency neighbourhood (relevance, not containment)
/// surface the specific fine-scale entities that actually matter. This is what
/// keeps a vague question's answer small regardless of the budget.
const MAX_ZOOM_EXPANSION: usize = 24;

pub fn scope_for_orbit(scale: OrbitScale) -> Option<ScopeScale> {
    match scale {
        OrbitScale::Universe => Some(ScopeScale::Universe),
        OrbitScale::Galaxy => Some(ScopeScale::Galaxy),
        OrbitScale::System => Some(ScopeScale::System),
        OrbitScale::Planet => Some(ScopeScale::Planet),
        OrbitScale::Moon => Some(ScopeScale::Moon),
        OrbitScale::Adaptive => None,
    }
}

pub fn zoom_roots(
    hierarchy: &AtlasHierarchy,
    starts: &[EntityId],
    scale: OrbitScale,
) -> Vec<EntityId> {
    let Some(target) = scope_for_orbit(scale) else {
        return starts.to_vec();
    };
    let mut result = BTreeSet::new();
    for start in starts {
        let Some(entity) = hierarchy.entity(*start) else {
            continue;
        };
        if entity.scale == target {
            result.insert(entity.id);
            continue;
        }
        if entity.scale.rank() > target.rank() {
            if let Some(ancestor) = hierarchy
                .ancestors_of(entity.id)
                .into_iter()
                .find(|e| e.scale == target)
            {
                result.insert(ancestor.id);
            }
        } else {
            let descendants: Vec<_> = hierarchy
                .descendants_of(entity.id)
                .into_iter()
                .filter(|e| e.scale == target)
                .collect();
            if descendants.len() <= MAX_ZOOM_EXPANSION {
                for descendant in descendants {
                    result.insert(descendant.id);
                }
            } else {
                // Too broad to expand — keep the coarse start as the root so the
                // core stays small; relevance (the dependency graph) fills in the
                // fine detail that matters.
                result.insert(entity.id);
            }
        }
    }
    result.into_iter().collect()
}

pub fn immediate_scale_contents(hierarchy: &AtlasHierarchy, root: EntityId) -> Vec<AtlasEntity> {
    let Some(entity) = hierarchy.entity(root) else {
        return Vec::new();
    };
    match entity.scale {
        ScopeScale::Universe | ScopeScale::Galaxy | ScopeScale::System => {
            hierarchy.children_of(root).into_iter().cloned().collect()
        }
        ScopeScale::Planet => hierarchy
            .descendants_of(root)
            .into_iter()
            .filter(|e| e.scale == ScopeScale::Moon)
            .cloned()
            .collect(),
        ScopeScale::Moon => Vec::new(),
    }
}
