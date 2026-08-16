use creature_context_types::{OrbitPacket, SelectedEdge, SelectedEntity};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum OrbitBudgetError {
    #[error("mandatory Orbit content requires at least {minimum_tokens} tokens")]
    MandatoryContentExceedsBudget { minimum_tokens: usize },
    #[error("Orbit serialization failed: {0}")]
    Serialization(String),
}

impl OrbitBudgetError {
    /// The budget this request would actually need, when that is known.
    pub fn minimum_required_tokens(&self) -> Option<usize> {
        match self {
            Self::MandatoryContentExceedsBudget { minimum_tokens } => Some(*minimum_tokens),
            _ => None,
        }
    }
}

pub fn estimate_tokens(packet: &OrbitPacket) -> Result<usize, OrbitBudgetError> {
    let bytes = serde_json::to_vec(packet)
        .map_err(|e| OrbitBudgetError::Serialization(e.to_string()))?
        .len();
    Ok(bytes.div_ceil(4))
}

fn estimate_entity(item: &SelectedEntity) -> Result<usize, OrbitBudgetError> {
    let bytes = serde_json::to_vec(item)
        .map_err(|e| OrbitBudgetError::Serialization(e.to_string()))?
        .len();
    Ok(bytes.div_ceil(4).max(1))
}

fn estimate_edge(item: &SelectedEdge) -> Result<usize, OrbitBudgetError> {
    let bytes = serde_json::to_vec(item)
        .map_err(|e| OrbitBudgetError::Serialization(e.to_string()))?
        .len();
    Ok(bytes.div_ceil(4).max(1))
}

fn estimate_spine(entity: &creature_context_types::AtlasEntity) -> Result<usize, OrbitBudgetError> {
    let bytes = serde_json::to_vec(entity)
        .map_err(|e| OrbitBudgetError::Serialization(e.to_string()))?
        .len();
    Ok(bytes.div_ceil(4).max(1))
}

/// Assign each selected item its own token cost and compute the packet total.
fn settle_estimates(packet: &mut OrbitPacket) -> Result<(), OrbitBudgetError> {
    packet.estimated_total_tokens = 0;
    for item in &mut packet.selected_entities {
        item.estimated_tokens = estimate_entity(item)?;
        packet.estimated_total_tokens += item.estimated_tokens;
    }
    for item in &mut packet.relationships {
        item.estimated_tokens = estimate_edge(item)?;
        packet.estimated_total_tokens += item.estimated_tokens;
    }
    for entity in &mut packet.architectural_spine {
        // Spine entities are not SelectedEntity wrappers; estimate directly.
        packet.estimated_total_tokens += estimate_spine(entity)?;
    }
    // Fixed overhead for the packet envelope and scalar fields.
    packet.estimated_total_tokens += 120;
    Ok(())
}

/// Fit a packet to its budget. The budget is a **maximum, never a minimum**: the
/// core (mandatory, high-score) entities are kept first, and the supporting
/// context — extra selected entities, then the architectural spine — is
/// dynamically quantified to fill whatever budget remains, trimmed
/// lowest-relevance-first. Nothing is an untrimmable floor, so a focused request
/// never fails demanding a huge "requires at least N tokens"; it simply returns
/// the most relevant detail that fits. The only irreducible case is a single
/// entity larger than the whole budget, which is returned as-is (one entity of
/// detail is the honest floor, not an error).
pub fn enforce_budget(packet: &mut OrbitPacket) -> Result<(), OrbitBudgetError> {
    packet.selected_entities.sort_by_key(|selected| {
        (
            std::cmp::Reverse(selected.mandatory),
            std::cmp::Reverse(selected.score),
            selected.entity.id,
        )
    });
    settle_estimates(packet)?;

    while packet.estimated_total_tokens > packet.budget {
        // 1) Trim the least-relevant *supporting* selected entity first.
        if let Some(index) = packet
            .selected_entities
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.mandatory)
            .min_by_key(|(_, item)| (item.score, item.entity.id))
            .map(|(index, _)| index)
        {
            let removed = packet.selected_entities.remove(index);
            *packet
                .omission_counts
                .entry(category_for(&removed))
                .or_default() += 1;
        }
        // 2) Then trim the architectural spine, coarsest scale first — a focused
        //    task cares least about the Universe/Galaxy framing. The spine used to
        //    be untrimmable, which is what let it balloon past any budget.
        else if !packet.architectural_spine.is_empty() {
            let index = packet
                .architectural_spine
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| (e.scale.rank(), e.id))
                .map(|(index, _)| index)
                .unwrap();
            packet.architectural_spine.remove(index);
            *packet.omission_counts.entry("spine".to_string()).or_default() += 1;
        }
        // 3) Finally trim even core entities, lowest-score first, but always keep
        //    at least one so the packet is never empty.
        else if packet.selected_entities.len() > 1 {
            let index = packet
                .selected_entities
                .iter()
                .enumerate()
                .min_by_key(|(_, item)| (item.score, item.entity.id))
                .map(|(index, _)| index)
                .unwrap();
            let removed = packet.selected_entities.remove(index);
            *packet
                .omission_counts
                .entry(category_for(&removed))
                .or_default() += 1;
        } else {
            // One irreducible entity bigger than the whole budget: return it.
            break;
        }

        let retained: std::collections::BTreeSet<_> = packet
            .selected_entities
            .iter()
            .map(|item| item.entity.id)
            .chain(packet.architectural_spine.iter().map(|e| e.id))
            .collect();
        packet.relationships.retain(|edge| {
            retained.contains(&edge.edge.source_entity_id)
                && retained.contains(&edge.edge.target_entity_id)
        });
        settle_estimates(packet)?;
    }

    packet.minimum_required_tokens = Some(packet.estimated_total_tokens);
    Ok(())
}

fn category_for(item: &SelectedEntity) -> String {
    if item.entity.scale == creature_context_types::ScopeScale::Moon {
        "moon".to_string()
    } else if item.entity.scale == creature_context_types::ScopeScale::Planet {
        "planet".to_string()
    } else if item.entity.scale == creature_context_types::ScopeScale::System {
        "system".to_string()
    } else {
        "entity".to_string()
    }
}

pub fn selected(
    entity: creature_context_types::AtlasEntity,
    mandatory: bool,
    score: i64,
    reason: impl Into<String>,
    ring: u8,
) -> SelectedEntity {
    SelectedEntity {
        entity,
        mandatory,
        score,
        reasons: vec![reason.into()],
        ring,
        estimated_tokens: 0,
    }
}
