//! The vendor-neutral model partner interface (specification 7, 8).
//!
//! A model partner proposes enrichment for the Atlas; it never writes to it. The
//! only thing a partner produces is a `CandidateRecord`, and the only way that
//! record reaches the Atlas is through Milestone 3 admission — where inference is
//! contained: it can propose an inferred summary or an inferred edge, but it can
//! never set an observed fact, satisfy a deterministic Green axis, or overwrite a
//! protected human decision (spec §7.3). The speaking role and the acted-upon
//! payload must not diverge.
//!
//! This is the whole surface a model plugs into. A rules-only partner (the
//! mandatory zero-model fallback, `crate::rules`) implements the same trait and
//! simply proposes nothing, so the deterministic pipeline runs unchanged.

use creature_context_core::context::admission::{AdmissionOutcome, admit};
use creature_context_types::{
    AtlasEntity, EntityId, SnapshotId,
    model::{CandidateRecord, CapabilityProfile},
};

/// One unit of work for a partner: an entity to enrich, pinned to the snapshot
/// the model reasons about. Pinning is what makes a proposal auditable — a
/// candidate whose snapshot has moved on is stale and admission rejects it.
pub struct WorkItem<'a> {
    pub entity: &'a AtlasEntity,
    pub snapshot_id: SnapshotId,
}

/// A model partner. Its capability is *measured*, not declared (spec §8), and its
/// output is always candidates for admission, never admitted facts.
pub trait ModelPartner {
    /// The measured capability profile of this partner.
    fn capability(&self) -> &CapabilityProfile;

    /// Propose zero or more candidate records enriching `work`. Proposing
    /// nothing is a valid, first-class answer — it is exactly what a rules-only
    /// partner does, and what any partner does when it has nothing to add.
    fn propose(&self, work: &WorkItem) -> Vec<CandidateRecord>;
}

/// Run a partner's proposals for `work` through admission, returning one outcome
/// per proposal. This is the only path from a partner to the Atlas: every
/// proposal is validated, and reaches the Atlas only as the admission outcome
/// allows (admitted inferred, queued for review, or rejected). A partner that
/// proposes nothing yields no outcomes and changes nothing.
pub fn propose_and_admit(
    partner: &dyn ModelPartner,
    work: &WorkItem,
    active: &SnapshotId,
    protected: &[EntityId],
) -> Vec<AdmissionOutcome> {
    partner
        .propose(work)
        .into_iter()
        .map(|candidate| admit(candidate, active, protected))
        .collect()
}
