use creature_context_types::{AtlasEdge, AtlasSnapshot, EntityId, RelationshipPlane};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug)]
pub struct ModuleMap {
    outgoing: BTreeMap<EntityId, Vec<AtlasEdge>>,
    incoming: BTreeMap<EntityId, Vec<AtlasEdge>>,
}

impl ModuleMap {
    pub fn from_snapshot(snapshot: &AtlasSnapshot) -> Self {
        let mut outgoing: BTreeMap<EntityId, Vec<AtlasEdge>> = BTreeMap::new();
        let mut incoming: BTreeMap<EntityId, Vec<AtlasEdge>> = BTreeMap::new();
        for edge in &snapshot.edges {
            outgoing
                .entry(edge.source_entity_id)
                .or_default()
                .push(edge.clone());
            incoming
                .entry(edge.target_entity_id)
                .or_default()
                .push(edge.clone());
        }
        for edges in outgoing.values_mut().chain(incoming.values_mut()) {
            edges.sort_by_key(|e| e.id);
        }
        Self { outgoing, incoming }
    }

    pub fn outgoing(&self, id: EntityId, include_inferred: bool) -> Vec<&AtlasEdge> {
        self.outgoing
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|e| include_inferred || e.plane != RelationshipPlane::Inferred)
            .collect()
    }

    pub fn incoming(&self, id: EntityId, include_inferred: bool) -> Vec<&AtlasEdge> {
        self.incoming
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|e| include_inferred || e.plane != RelationshipPlane::Inferred)
            .collect()
    }

    pub fn neighbourhood(
        &self,
        starts: &[EntityId],
        depth: usize,
        include_inferred: bool,
    ) -> (BTreeSet<EntityId>, Vec<AtlasEdge>) {
        let mut entities: BTreeSet<EntityId> = starts.iter().copied().collect();
        let mut edge_ids = BTreeSet::new();
        let mut edges = Vec::new();
        let mut queue: VecDeque<_> = starts.iter().copied().map(|id| (id, 0usize)).collect();
        while let Some((id, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }
            for edge in self
                .outgoing(id, include_inferred)
                .into_iter()
                .chain(self.incoming(id, include_inferred))
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
                    queue.push_back((neighbour, current_depth + 1));
                }
            }
        }
        edges.sort_by_key(|e| e.id);
        (entities, edges)
    }
}
