//! Native metadata projection (specification 16).
//!
//! Green state is projected onto the filesystem as native metadata — Finder tags
//! on macOS, and the equivalent per-OS surface elsewhere — so a person browsing
//! files sees the same colours the Atlas holds. The projection is *derived*: it
//! is a view of the Atlas, never a source of truth. Deleting it loses nothing,
//! because it is rebuilt from the Atlas on demand (spec §11, §16).
//!
//! This module owns the portable half — deciding which tag each entity gets from
//! its Green code. The per-OS submodules own the I/O skin that writes those tags,
//! and each reports its true capability state: an OS whose adapter has not been
//! implemented and run says `Unavailable`, never fabricates success (spec §16).

use creature_context_types::{AtlasSnapshot, green::GreenCode, model::CapabilityState};
use std::path::Path;

/// Apply the Green projection to `root`'s files as native metadata, returning how
/// many files were tagged. This is the write half — it derives the projection
/// with `project` and hands each assignment to the current platform's adapter.
/// On a platform whose adapter is unavailable it is a no-op returning zero, which
/// is honest: nothing was projected because nothing could be. Deleting the tags
/// later loses nothing — they rebuild from the Atlas on the next call.
pub fn apply(root: &Path, snapshot: &AtlasSnapshot) -> usize {
    let assignments = project(snapshot);
    #[cfg(target_os = "macos")]
    {
        assignments
            .iter()
            .filter(|assignment| {
                macos::write_tag(&root.join(&assignment.relative_path), &assignment.tag).is_ok()
            })
            .count()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (root, &assignments);
        0
    }
}

/// One tag to project onto one file: a repository-relative path and the label
/// derived from its Green code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagAssignment {
    pub relative_path: String,
    pub tag: String,
}

/// The tag label for a Green code. Stable, human-readable, and the same on every
/// platform — the OS adapters differ only in how they attach it.
pub fn tag_label(code: GreenCode) -> &'static str {
    match code {
        GreenCode::Green => "Green",
        GreenCode::Yellow => "Yellow",
        GreenCode::Red => "Red",
        GreenCode::Unknown => "Unknown",
    }
}

/// Derive the metadata projection from a snapshot: one assignment per entity that
/// has both a file path and an evaluated Green code. This is the whole source of
/// truth for the projection — the OS layer only writes what this returns, so the
/// projection can always be rebuilt from the Atlas.
pub fn project(snapshot: &AtlasSnapshot) -> Vec<TagAssignment> {
    snapshot
        .entities
        .iter()
        .filter_map(|entity| {
            let path = entity.relative_path.as_ref()?;
            let code = entity.green.as_ref()?.overall;
            Some(TagAssignment {
                relative_path: path.clone(),
                tag: tag_label(code).to_string(),
            })
        })
        .collect()
}

/// The metadata capability of the platform this build runs on. macOS has a real,
/// verified adapter; the others report their true state until their adapter is
/// implemented and run on that platform.
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
    #[cfg(target_os = "android")]
    {
        android::capability()
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows",
        target_os = "android"
    )))]
    {
        CapabilityState::Unavailable
    }
}

pub mod android;
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod windows;
