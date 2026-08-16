use creature_context_types::{AtlasEntity, EntityId, ScopeScale};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum HierarchyError {
    #[error("duplicate entity id {0}")]
    Duplicate(EntityId),
    #[error("entity {0} has no parent")]
    MissingParent(EntityId),
    #[error("entity {child} has invalid parent scale {parent_scale:?}")]
    InvalidScale {
        child: EntityId,
        parent_scale: ScopeScale,
    },
    #[error("containment cycle includes {0}")]
    Cycle(EntityId),
    #[error("expected exactly one universe root, found {0}")]
    RootCount(usize),
}

#[derive(Clone, Debug)]
pub struct AtlasHierarchy {
    entities: BTreeMap<EntityId, AtlasEntity>,
    children: BTreeMap<EntityId, Vec<EntityId>>,
    root_id: EntityId,
}

impl AtlasHierarchy {
    pub fn from_entities(entities: &[AtlasEntity]) -> Result<Self, HierarchyError> {
        let mut by_id = BTreeMap::new();
        for entity in entities {
            if by_id.insert(entity.id, entity.clone()).is_some() {
                return Err(HierarchyError::Duplicate(entity.id));
            }
        }
        let roots: Vec<_> = entities
            .iter()
            .filter(|e| e.scale == ScopeScale::Universe && e.parent_id.is_none())
            .collect();
        if roots.len() != 1 {
            return Err(HierarchyError::RootCount(roots.len()));
        }
        let root_id = roots[0].id;
        let mut children: BTreeMap<EntityId, Vec<EntityId>> = BTreeMap::new();
        for entity in entities {
            if entity.id == root_id {
                continue;
            }
            let parent_id = entity
                .parent_id
                .ok_or(HierarchyError::MissingParent(entity.id))?;
            let parent = by_id
                .get(&parent_id)
                .ok_or(HierarchyError::MissingParent(entity.id))?;
            if !allowed(parent.scale, entity.scale) {
                return Err(HierarchyError::InvalidScale {
                    child: entity.id,
                    parent_scale: parent.scale,
                });
            }
            children.entry(parent_id).or_default().push(entity.id);
        }
        for ids in children.values_mut() {
            ids.sort_by_key(|id| {
                let e = &by_id[id];
                (e.scale.rank(), e.canonical_name.to_lowercase(), e.id)
            });
        }
        let hierarchy = Self {
            entities: by_id,
            children,
            root_id,
        };
        hierarchy.ensure_acyclic()?;
        Ok(hierarchy)
    }

    pub fn root_id(&self) -> EntityId {
        self.root_id
    }
    pub fn entity(&self, id: EntityId) -> Option<&AtlasEntity> {
        self.entities.get(&id)
    }
    pub fn children_of(&self, id: EntityId) -> Vec<&AtlasEntity> {
        self.children
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|child| self.entities.get(child))
            .collect()
    }
    pub fn ancestors_of(&self, id: EntityId) -> Vec<&AtlasEntity> {
        let mut result = Vec::new();
        let mut current = self.entities.get(&id).and_then(|e| e.parent_id);
        while let Some(parent_id) = current {
            let Some(parent) = self.entities.get(&parent_id) else {
                break;
            };
            result.push(parent);
            current = parent.parent_id;
        }
        result.reverse();
        result
    }
    pub fn descendants_of(&self, id: EntityId) -> Vec<&AtlasEntity> {
        let mut result = Vec::new();
        let mut pending = self.children.get(&id).cloned().unwrap_or_default();
        while let Some(next) = pending.pop() {
            if let Some(entity) = self.entities.get(&next) {
                result.push(entity);
                if let Some(child_ids) = self.children.get(&next) {
                    pending.extend(child_ids.iter().copied());
                }
            }
        }
        result.sort_by_key(|e| (e.scale.rank(), e.canonical_name.to_lowercase(), e.id));
        result
    }
    pub fn cross_scale_spine(&self, id: EntityId) -> Vec<&AtlasEntity> {
        let mut spine = self.ancestors_of(id);
        if let Some(entity) = self.entities.get(&id) {
            spine.push(entity);
        }
        spine
    }
    pub fn at_scale(&self, scale: ScopeScale) -> Vec<&AtlasEntity> {
        self.entities
            .values()
            .filter(|e| e.scale == scale)
            .collect()
    }

    fn ensure_acyclic(&self) -> Result<(), HierarchyError> {
        for id in self.entities.keys().copied() {
            let mut seen = BTreeSet::new();
            let mut current = Some(id);
            while let Some(value) = current {
                if !seen.insert(value) {
                    return Err(HierarchyError::Cycle(value));
                }
                current = self.entities.get(&value).and_then(|e| e.parent_id);
            }
        }
        Ok(())
    }
}

fn allowed(parent: ScopeScale, child: ScopeScale) -> bool {
    matches!(
        (parent, child),
        (ScopeScale::Universe, ScopeScale::Galaxy)
            | (ScopeScale::Galaxy, ScopeScale::System)
            | (ScopeScale::Galaxy, ScopeScale::Planet)
            | (ScopeScale::System, ScopeScale::System)
            | (ScopeScale::System, ScopeScale::Planet)
            | (ScopeScale::Planet, ScopeScale::Moon)
            // A file and the symbols it contains are both Moon scale
            // (specification 3.4: a Moon is a "file, type, function, test or
            // exact resource"), so a file Moon may contain symbol Moons — the
            // same same-scale nesting already allowed for System.
            | (ScopeScale::Moon, ScopeScale::Moon)
    )
}
