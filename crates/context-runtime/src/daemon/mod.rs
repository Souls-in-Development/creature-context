//! Registering the resident service with the operating system (specification 7.1, 16).
//!
//! `creature-context run` is the constant background lane, but a process started
//! from a terminal lives and dies with that terminal — so the Atlas was only
//! current while somebody kept a shell open, and the lane belonged to whoever
//! started it. This module hands the process to the OS's own supervisor instead,
//! so the daemon is resident across logins and belongs to no particular agent,
//! editor or session.
//!
//! The portable half lives here: deriving a stable per-project label and the
//! supervisor definition. The per-OS submodules own the I/O skin that registers
//! it, and each reports its true capability state — an OS whose adapter has not
//! been implemented and run says so rather than fabricating success (spec §16).
//!
//! Definitions are generated as data (`ServiceDefinition`) *before* anything is
//! written, so the exact file contents can be asserted in a test and validated by
//! the platform's own tooling without installing anything.

use creature_context_types::model::CapabilityState;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("no supervisor adapter on this platform")]
    Unsupported,
    #[error("the project root could not be resolved: {0}")]
    Root(#[from] std::io::Error),
    #[error("the running binary could not be located: {0}")]
    Binary(String),
    #[error("{0}")]
    Supervisor(String),
}

/// A supervisor registration, as data. Produced without touching the filesystem
/// so it can be asserted in a test and linted by the platform's own tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDefinition {
    /// The supervisor's identifier for this project's daemon.
    pub label: String,
    /// Where the definition file belongs.
    pub unit_path: PathBuf,
    /// The definition file's full contents.
    pub contents: String,
}

/// Whether this daemon is registered with the OS supervisor, and running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaemonStatus {
    pub label: String,
    /// The definition file exists where the supervisor expects it.
    pub installed: bool,
    /// The supervisor reports the job as loaded.
    pub loaded: bool,
}

/// A stable, filesystem-safe label for `root`'s daemon.
///
/// Derived from the canonical path so the same project always yields the same
/// label — installing twice replaces one registration rather than accumulating
/// them — and two checkouts of the same repository get different ones. Hashed
/// rather than path-derived because a supervisor label may not contain a path
/// separator, and because a repository path can be long, non-ASCII, or private.
pub fn label_for(root: &Path) -> Result<String, DaemonError> {
    label_for_roots(std::slice::from_ref(&root.to_path_buf()))
}

/// The label for a *set* of roots supervised by one daemon.
///
/// Derived from the sorted canonical paths, so the same set always yields the
/// same label regardless of the order they were given in — installing the same
/// projects twice replaces one registration rather than accumulating them — and
/// any different set gets a different label.
pub fn label_for_roots(roots: &[PathBuf]) -> Result<String, DaemonError> {
    let mut canonical: Vec<String> = Vec::new();
    for root in roots {
        canonical.push(root.canonicalize()?.to_string_lossy().to_string());
    }
    canonical.sort();
    canonical.dedup();
    let digest = blake3::hash(canonical.join("\u{0}").as_bytes()).to_hex();
    Ok(format!("com.creature-context.{}", &digest[..16]))
}

/// The absolute path of the currently running binary, which is what the
/// supervisor must invoke. Resolved rather than assumed: the daemon is started
/// by the OS with no shell, no PATH lookup and no working directory of ours.
pub fn current_binary() -> Result<PathBuf, DaemonError> {
    std::env::current_exe()
        .map_err(|error| DaemonError::Binary(error.to_string()))?
        .canonicalize()
        .map_err(|error| DaemonError::Binary(error.to_string()))
}

/// Where the daemon's own output is written. Inside the project's `.creature`
/// directory, because a supervised process has no terminal to print to and its
/// output is the only way to see what it did.
pub fn log_path(root: &Path) -> PathBuf {
    root.join(".creature").join("daemon.log")
}

/// Whether this platform can register the resident service with its supervisor.
pub fn capability() -> CapabilityState {
    #[cfg(target_os = "macos")]
    {
        macos::capability()
    }
    #[cfg(target_os = "linux")]
    {
        linux::capability()
    }
    #[cfg(target_os = "windows")]
    {
        windows::capability()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        CapabilityState::Unavailable
    }
}

/// Build the supervisor definition for `root` without writing anything.
pub fn definition(roots: &[PathBuf]) -> Result<ServiceDefinition, DaemonError> {
    let binary = current_binary()?;
    definition_with_binary(roots, &binary)
}

/// As `definition`, with the binary supplied — the seam a test uses to assert
/// exact contents without depending on where the test harness happens to live.
pub fn definition_with_binary(
    roots: &[PathBuf],
    binary: &Path,
) -> Result<ServiceDefinition, DaemonError> {
    #[cfg(target_os = "macos")]
    {
        macos::definition(roots, binary)
    }
    #[cfg(target_os = "linux")]
    {
        linux::definition(roots, binary)
    }
    #[cfg(target_os = "windows")]
    {
        windows::definition(roots, binary)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (roots, binary);
        Err(DaemonError::Unsupported)
    }
}

/// Register `root`'s daemon with the OS supervisor and start it. Installing over
/// an existing registration replaces it, so this is safe to repeat.
pub fn install(roots: &[PathBuf]) -> Result<ServiceDefinition, DaemonError> {
    #[cfg(target_os = "macos")]
    {
        macos::install(roots)
    }
    #[cfg(target_os = "linux")]
    {
        linux::install(roots)
    }
    #[cfg(target_os = "windows")]
    {
        windows::install(roots)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = roots;
        Err(DaemonError::Unsupported)
    }
}

/// Stop `root`'s daemon and remove its registration. Uninstalling something that
/// was never installed succeeds — the desired end state is what matters.
pub fn uninstall(roots: &[PathBuf]) -> Result<(), DaemonError> {
    #[cfg(target_os = "macos")]
    {
        macos::uninstall(roots)
    }
    #[cfg(target_os = "linux")]
    {
        linux::uninstall(roots)
    }
    #[cfg(target_os = "windows")]
    {
        windows::uninstall(roots)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = roots;
        Err(DaemonError::Unsupported)
    }
}

/// Report whether `root`'s daemon is registered and loaded.
pub fn status(roots: &[PathBuf]) -> Result<DaemonStatus, DaemonError> {
    #[cfg(target_os = "macos")]
    {
        macos::status(roots)
    }
    #[cfg(target_os = "linux")]
    {
        linux::status(roots)
    }
    #[cfg(target_os = "windows")]
    {
        windows::status(roots)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = roots;
        Err(DaemonError::Unsupported)
    }
}
