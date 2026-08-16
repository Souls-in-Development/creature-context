//! Deterministic socket unification — the reconciler of specification 6.4.
//!
//! A `provides` socket is a shape an entity exposes; a `requires` socket is a
//! shape an entity needs. Extraction (the parser) proposes these shapes but
//! never decides whether two fit — "that division is what makes the mechanism
//! both cheap and trustworthy" (spec 6.4). This function is the deciding half:
//! it is the operation a linker performs, set intersection over shapes decided
//! arithmetically, and it writes a resolution onto every `requires` socket:
//!
//! - exactly one provider of the required name → a `fit` on the `inferred`
//!   plane with `status:unconfirmed`. Alignment is automatic only when it is
//!   unambiguous, and even then it is a claim about a connection, not a proven
//!   one — a compile or a test settles it later (spec 6.4).
//! - several providers → a `hole` with reason `ambiguous`, listing the
//!   candidates. Choosing among them is forbidden: a wrong connection is more
//!   expensive than an absent one, because a hole is visible and a miswiring is
//!   not.
//! - no provider → a `hole` with reason `no_match`, naming the absent shape.
//!
//! Matching is by the shape's name, never its signature alone: "signature alone
//! collides, and two functions with identical signatures are routinely not
//! interchangeable" (spec 6.4). The qualified name is part of the shape, and it
//! is the cheapest defence against wiring an internal identifier where an
//! external one belongs.

use creature_context_types::{
    AtlasSnapshot, FitBasis, FitPlane, FitStatus, HoleReason, ProofPathState, SocketDirection,
    SocketFit, SocketHole, SocketId, SocketResolution,
};
use std::collections::BTreeMap;

/// The name a shape is matched by: the final segment of its qualified name,
/// after `::`, `.` or `/`. Two shapes unify only if these agree — a re-export
/// changes the path a caller writes but not the item's own name, so matching on
/// the final segment is tolerant of re-exports while still refusing to fit
/// across different names.
pub fn match_name(qualified_name: &str) -> &str {
    qualified_name
        .rsplit([':', '.', '/'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(qualified_name)
}

/// Resolve one `requires` socket against an already-built provider index —
/// `match_name` → the provider socket ids for that name, insertion-ordered. This
/// is the per-socket decision `resolve_sockets` makes, factored out so a caller
/// that builds the index itself (the layered scan's cross-directory stitch, which
/// unions provider names from every directory) resolves identically to the
/// whole-snapshot pass. `candidates` must be the exact `Vec<SocketId>` the index
/// holds for the wanted name (empty when absent).
pub fn resolve_against(candidates: &[SocketId]) -> SocketResolution {
    match candidates {
        [] => SocketResolution::Hole(SocketHole {
            reason: HoleReason::NoMatch,
            candidates: vec![],
            adapter_target: false,
        }),
        [only] => SocketResolution::Fit(SocketFit {
            provided_socket_id: *only,
            basis: FitBasis::Unique,
            status: FitStatus::Unconfirmed,
            checked_by: None,
            proof_path: ProofPathState::Unchecked,
            plane: FitPlane::Inferred,
            confidence: 0.5,
        }),
        many => SocketResolution::Hole(SocketHole {
            reason: HoleReason::Ambiguous,
            candidates: many.to_vec(),
            adapter_target: false,
        }),
    }
}

/// Resolve every `requires` socket in `snapshot` against the `provides` sockets
/// available anywhere in it, writing each one's `resolution` in place. Providers
/// already resolved are left untouched; a `requires` socket is always given a
/// definite `fit` or `hole`, never left `unresolved` — matching has run.
pub fn resolve_sockets(snapshot: &mut AtlasSnapshot) {
    // Index every provider by the name it is matched on. BTreeMap and the
    // insertion-ordered socket vectors keep candidate lists deterministic.
    let mut providers: BTreeMap<String, Vec<SocketId>> = BTreeMap::new();
    for entity in &snapshot.entities {
        for socket in &entity.sockets {
            if socket.direction == SocketDirection::Provides {
                providers
                    .entry(match_name(&socket.shape.qualified_name).to_string())
                    .or_default()
                    .push(socket.id);
            }
        }
    }

    for entity in &mut snapshot.entities {
        for socket in &mut entity.sockets {
            if socket.direction != SocketDirection::Requires {
                continue;
            }
            let wanted = match_name(&socket.shape.qualified_name);
            let candidates = providers.get(wanted).cloned().unwrap_or_default();
            socket.resolution = resolve_against(&candidates);
        }
    }
}
