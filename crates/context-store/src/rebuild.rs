//! Reconstruct the disposable database from portable project state.
//!
//! Specification 20 item 5 requires that SQLite can be deleted and rebuilt from
//! the files that travel with the project. The root `ATLAS.idx` is that source:
//! galaxy-scoped and complete (4.1).
//!
//! Every failure path here is closed. A rebuild that silently produces a
//! partial database is worse than one that refuses, because the result looks
//! like a working project with most of it missing — which is exactly what a
//! folder-scoped root produced before Task 1 (5 entities of 311).

use crate::{AtlasRepository, StoreError, idx::decode_atlas_idx};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RebuildError {
    #[error("no portable root at {0}: there is nothing to rebuild from")]
    MissingRoot(String),
    #[error("cannot read portable root {path}: {source}")]
    Unreadable {
        path: String,
        source: std::io::Error,
    },
    #[error(
        "portable root is not galaxy-scoped; rebuilding from it would restore a partial snapshot"
    )]
    NotGalaxyScoped,
    #[error("portable root is malformed: {0}")]
    Malformed(String),
    #[error("rebuilt snapshot is empty; refusing to replace the database with nothing")]
    EmptySnapshot,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Findings reported after a successful rebuild, so a caller can state what was
/// restored rather than only that the command exited zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildReport {
    pub snapshot_id: String,
    pub entities: usize,
    pub edges: usize,
    pub records: usize,
    /// Records whose type this build does not recognise, preserved verbatim.
    pub opaque_records: usize,
}

/// Rebuild `database` from the root `ATLAS.idx` under `project_root`.
///
/// The database is replaced only after the portable state has been read and
/// validated, so a failed rebuild leaves any existing database untouched and
/// creates no new one.
pub fn rebuild_repository_from_portable(
    project_root: &Path,
    database: &Path,
) -> Result<RebuildReport, RebuildError> {
    let root_idx = project_root.join("ATLAS.idx");
    if !root_idx.exists() {
        return Err(RebuildError::MissingRoot(root_idx.display().to_string()));
    }

    let contents =
        std::fs::read_to_string(&root_idx).map_err(|source| RebuildError::Unreadable {
            path: root_idx.display().to_string(),
            source,
        })?;

    // Check the scope before decoding. A folder-scoped file parses perfectly
    // well and yields a valid partial snapshot, which is the dangerous case.
    let header = contents.lines().next().unwrap_or_default();
    if !header.contains("kind:atlas") || !header.contains("scale:galaxy") {
        return Err(RebuildError::NotGalaxyScoped);
    }

    let decoded =
        decode_atlas_idx(&contents).map_err(|e| RebuildError::Malformed(e.to_string()))?;

    if decoded.snapshot.entities.is_empty() {
        return Err(RebuildError::EmptySnapshot);
    }

    let report = RebuildReport {
        snapshot_id: decoded.snapshot.id.0.clone(),
        entities: decoded.snapshot.entities.len(),
        edges: decoded.snapshot.edges.len(),
        records: decoded.snapshot.records.len(),
        opaque_records: decoded.opaque_records.len(),
    };

    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RebuildError::Unreadable {
            path: parent.display().to_string(),
            source,
        })?;
    }

    let mut repository = AtlasRepository::open(database)?;
    repository.replace_snapshot(&decoded.snapshot)?;

    Ok(report)
}
