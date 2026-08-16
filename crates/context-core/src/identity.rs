//! Stable identity across rescans.
//!
//! A rescan parses the current text and mints fresh ids from it, but a moved or
//! renamed declaration is still the same entity. Paths and line numbers are
//! mutable attributes; the stable id is canonical (spec §3, §6). This reconciler
//! carries the previous snapshot's ids onto a freshly enriched one where the
//! entity is recognisably the same:
//!
//! - same name and kind (a move — only the line changed) → id preserved;
//! - a single structural match of the same kind under a new name (an
//!   unambiguous rename) → id preserved;
//! - several equally-good structural matches → left with fresh ids, never
//!   merged. "Rename ambiguity creates candidates; it never silently merges
//!   stable identities" (spec §17).
//!
//! Matching is within a file (grouped by relative path). A declaration that
//! moves to a different file is treated as new here — cross-file move and split
//! reconciliation is a later refinement, recorded rather than faked.

use creature_context_types::{AtlasSnapshot, EntityId, EntityKind, model::InferredSummary};
use std::collections::HashMap;

/// What a reconciliation did, for evidence and journalling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Ids carried over because the name and kind matched (a move).
    pub preserved: usize,
    /// Ids carried over across an unambiguous structural rename.
    pub renamed: usize,
    /// Renames with several candidates, left unmerged.
    pub ambiguous: usize,
}

/// A symbol's identity-relevant fields, lifted out so the reconciler can read
/// both snapshots and then mutate `next` without borrow conflicts.
struct Symbol {
    id: EntityId,
    name: String,
    kind: EntityKind,
    fingerprint: String,
    file: String,
}

/// The declarations of a snapshot: entities whose parent is a file entity.
fn symbols(snapshot: &AtlasSnapshot) -> Vec<Symbol> {
    let files: std::collections::HashSet<EntityId> = snapshot
        .entities
        .iter()
        .filter(|e| e.kind == EntityKind::File)
        .map(|e| e.id)
        .collect();
    snapshot
        .entities
        .iter()
        .filter(|e| e.parent_id.is_some_and(|p| files.contains(&p)))
        .map(|e| Symbol {
            id: e.id,
            name: e.canonical_name.clone(),
            kind: e.kind,
            fingerprint: e.structural_fingerprint.clone(),
            file: e.relative_path.clone().unwrap_or_default(),
        })
        .collect()
}

/// Carry `prev`'s stable ids onto `next` in place, returning what was matched.
pub fn reconcile_identity(prev: &AtlasSnapshot, next: &mut AtlasSnapshot) -> Reconciliation {
    let prev_syms = symbols(prev);
    let next_syms = symbols(next);

    let mut by_file: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in &prev_syms {
        by_file
            .entry(symbol.file.as_str())
            .or_default()
            .push(symbol);
    }
    let mut next_by_file: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in &next_syms {
        next_by_file
            .entry(symbol.file.as_str())
            .or_default()
            .push(symbol);
    }

    let mut recon = Reconciliation::default();
    // next id → prev id, applied to the whole snapshot once matching is done.
    let mut remap: HashMap<EntityId, EntityId> = HashMap::new();

    for (file, nexts) in &next_by_file {
        let Some(prevs) = by_file.get(file) else {
            continue; // no prior version of this file — every symbol is new
        };
        let mut consumed = vec![false; prevs.len()];

        // Phase 1 — same name and kind. Only the line moved.
        let mut unmatched = Vec::new();
        for n in nexts {
            let hit = prevs
                .iter()
                .enumerate()
                .find(|(i, p)| !consumed[*i] && p.name == n.name && p.kind == n.kind);
            match hit {
                Some((i, p)) => {
                    consumed[i] = true;
                    if p.id != n.id {
                        remap.insert(n.id, p.id);
                        recon.preserved += 1;
                    }
                }
                None => unmatched.push(*n),
            }
        }

        // Phase 2 — a structural match of the same kind under a new name.
        for n in unmatched {
            let candidates: Vec<usize> = prevs
                .iter()
                .enumerate()
                .filter(|(i, p)| {
                    !consumed[*i] && p.kind == n.kind && p.fingerprint == n.fingerprint
                })
                .map(|(i, _)| i)
                .collect();
            match candidates.as_slice() {
                [i] => {
                    consumed[*i] = true;
                    if prevs[*i].id != n.id {
                        remap.insert(n.id, prevs[*i].id);
                        recon.renamed += 1;
                    }
                }
                [] => {}                   // genuinely new — keep the fresh id
                _ => recon.ambiguous += 1, // several candidates — never merge
            }
        }
    }

    apply_remap(next, &remap);

    // Carry the semantic lane's enrichment forward with the entity's identity. A
    // fresh index produces empty inferred summaries, so without this every
    // deterministic reconcile would wipe the model's work. An entity that kept
    // its id is the same entity, so its inferred summaries travel with it —
    // unless this index already re-enriched it (non-empty), which wins.
    let carried: HashMap<EntityId, Vec<InferredSummary>> = prev
        .entities
        .iter()
        .filter(|entity| !entity.inferred_summaries.is_empty())
        .map(|entity| (entity.id, entity.inferred_summaries.clone()))
        .collect();
    if !carried.is_empty() {
        for entity in &mut next.entities {
            if entity.inferred_summaries.is_empty()
                && let Some(summaries) = carried.get(&entity.id)
            {
                entity.inferred_summaries = summaries.clone();
            }
        }
    }
    recon
}

/// Rewrite every reference to a remapped id: the entity's own id, parent links,
/// socket owners and edge endpoints — so the graph does not dangle.
fn apply_remap(next: &mut AtlasSnapshot, remap: &HashMap<EntityId, EntityId>) {
    if remap.is_empty() {
        return;
    }
    for entity in &mut next.entities {
        if let Some(&stable) = remap.get(&entity.id) {
            entity.id = stable;
        }
        if let Some(parent) = entity.parent_id
            && let Some(&stable) = remap.get(&parent)
        {
            entity.parent_id = Some(stable);
        }
        for socket in &mut entity.sockets {
            if let Some(&stable) = remap.get(&socket.entity_id) {
                socket.entity_id = stable;
            }
        }
    }
    for edge in &mut next.edges {
        if let Some(&stable) = remap.get(&edge.source_entity_id) {
            edge.source_entity_id = stable;
        }
        if let Some(&stable) = remap.get(&edge.target_entity_id) {
            edge.target_entity_id = stable;
        }
    }
}
