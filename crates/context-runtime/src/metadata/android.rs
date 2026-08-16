//! Android metadata adapter (specification 16): a MediaStore / URI-based
//! projection would carry the Green state here, since app-scoped storage has no
//! general extended-attribute surface. The adapter is not yet implemented or run
//! on Android, so it reports its true state — `Unavailable` — rather than
//! fabricating success. It will report `Verified` only once it has actually run
//! on the platform (spec §16, §18.4).

use creature_context_types::model::CapabilityState;

pub fn capability() -> CapabilityState {
    CapabilityState::Unavailable
}
