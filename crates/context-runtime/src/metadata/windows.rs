//! Windows metadata adapter (specification 16): NTFS alternate data streams or
//! the property system would carry the Green projection here. The adapter is not
//! yet implemented or run on Windows, so it reports its true state —
//! `Unavailable` — rather than fabricating success. It will report `Verified`
//! only once it has actually run on the platform (spec §16, §18.4).

use creature_context_types::model::CapabilityState;

pub fn capability() -> CapabilityState {
    CapabilityState::Unavailable
}
