//! The coherence axis (H) — specification 11.
//!
//! H asks whether authoritative intent, structural facts, observed behaviour and
//! current tool state agree. Agreement is the default the other axes establish;
//! *disagreement* is a contradiction, recorded as a `ConflictRecord` and linked
//! to the entities it touches. This module turns an entity's open contradictions
//! into a contribution to its coherence axis, so a contradiction is visible in
//! Green rather than only in a side list.
//!
//! The severity carried on the conflict is the contribution, and that is where
//! the containment rule lives: "inference may identify a possible conflict but
//! cannot alone certify Green or a deterministic failure" (spec §11). A
//! model-suspected contradiction is created at Yellow severity and so can only
//! darken H to Yellow; a deterministically-verified or human-confirmed one
//! carries Red. The cap is enforced where the conflict is created (admission);
//! the evaluator reads the severity it was given.
//!
//! A resolved conflict contributes nothing — it has been reconciled.

use creature_context_types::{AtlasEntity, ConflictRecord, ConflictState, green::GreenCode};

/// What each of `entity`'s open contradictions contributes to its coherence
/// axis, paired with a reason naming the contradiction. Resolved contradictions,
/// and ids that name no conflict in the snapshot, contribute nothing.
pub fn coherence_contributions(
    entity: &AtlasEntity,
    conflicts: &[ConflictRecord],
) -> Vec<(GreenCode, String)> {
    entity
        .open_conflict_ids
        .iter()
        .filter_map(|id| conflicts.iter().find(|conflict| conflict.id == *id))
        .filter(|conflict| conflict.state == ConflictState::Open)
        .map(|conflict| {
            (
                conflict.severity,
                format!(
                    "open contradiction {} ({:?}): authoritative intent and observation disagree",
                    conflict.id, conflict.severity
                ),
            )
        })
        .collect()
}
