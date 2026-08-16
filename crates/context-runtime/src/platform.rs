//! Platform capability matrix (specification 16, 18.4).
//!
//! Every native capability reports its true state, and this is where they are
//! gathered for the platform this build runs on. The rule the whole milestone
//! turns on: a capability that has not run on a platform reports `Unavailable`
//! or `ImplementedUnverified`, never a fabricated `Verified`. Native feature
//! status is reported only after actually running on the claimed platform.
//!
//! The matrix is per-current-platform by construction — it runs on one OS and
//! reports that OS's real state. The portable core beneath it (scan, Atlas,
//! Green, Orbit) is platform-neutral and produces equivalent records everywhere;
//! the adapters here are the I/O skin that differs.

use creature_context_types::model::CapabilityState;

/// The native capabilities of the current platform.
#[derive(Clone, Debug, PartialEq)]
pub struct PlatformCapabilities {
    /// The operating system this build runs on.
    pub os: &'static str,
    /// Projecting Green state onto native file metadata (spec §16).
    pub metadata: CapabilityState,
    /// Watching the filesystem for changes (spec §16.1).
    pub watcher: CapabilityState,
    /// Registering the resident daemon with the OS supervisor, so the background
    /// lane survives logout and belongs to no session (spec §7.1).
    pub supervisor: CapabilityState,
}

/// Report the current platform's capabilities, measured — not assumed.
pub fn capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        os: std::env::consts::OS,
        metadata: crate::metadata::capability(),
        watcher: watcher_capability(),
        supervisor: crate::daemon::capability(),
    }
}

/// The filesystem-watcher capability. The portable `notify`-based
/// `RuntimeWatcher` is the default adapter and satisfies the §16.1 contract —
/// canonical watch root, typed stream-failure events, self-exclusion. It is
/// verified on macOS by the watcher tests in this crate. On the other backends
/// `notify` supports it is implemented but this build has not run its tests
/// there, so it reports `ImplementedUnverified` rather than claiming a
/// verification it did not perform. Per-OS native watchers (FSEvents,
/// ReadDirectoryChangesW, inotify, FileObserver) beyond the portable adapter are
/// a recorded refinement.
fn watcher_capability() -> CapabilityState {
    #[cfg(target_os = "macos")]
    {
        CapabilityState::Verified
    }
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        CapabilityState::ImplementedUnverified
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        CapabilityState::Unavailable
    }
}
