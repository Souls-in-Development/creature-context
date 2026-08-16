//! The Universe registry: multiple unrelated Galaxies on one installation, kept
//! isolated.
//!
//! Isolation is structural, not a rule to remember. Each project is keyed by its
//! own `ProjectId`; nothing is shared across galaxies unless a cross-galaxy
//! relationship is *explicitly declared*. Resemblance is never a relationship
//! (specification 2), so `resolve_dependency` returns `Isolated` by default and
//! `Resolved` only for a declared link.

use creature_context_types::{EntityId, ProjectId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One project registered in the Universe: its stable identity and where it
/// lives. Distinct `ProjectId`/`galaxy_id` per project is what keeps two
/// unrelated codebases from ever merging.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredGalaxy {
    pub project_id: ProjectId,
    pub universe_id: EntityId,
    pub galaxy_id: EntityId,
    pub root: PathBuf,
}

/// Whether two projects are related. `Resolved` requires an explicit link;
/// there is no inference from similarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Isolated,
    Resolved,
}

/// An unordered pair of projects, so a declared link is symmetric and stored
/// once regardless of argument order.
fn pair(a: ProjectId, b: ProjectId) -> (ProjectId, ProjectId) {
    if a.to_string() <= b.to_string() {
        (a, b)
    } else {
        (b, a)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UniverseRegistry {
    galaxies: BTreeMap<ProjectId, RegisteredGalaxy>,
    /// Explicitly declared cross-galaxy relationships, stored as ordered pairs.
    links: BTreeSet<(ProjectId, ProjectId)>,
}

impl UniverseRegistry {
    pub fn register(&mut self, galaxy: RegisteredGalaxy) {
        self.galaxies.insert(galaxy.project_id, galaxy);
    }

    pub fn find(&self, project_id: ProjectId) -> Option<&RegisteredGalaxy> {
        self.galaxies.get(&project_id)
    }

    /// Declare an explicit cross-galaxy relationship. Only a declared link makes
    /// two projects resolve as related.
    pub fn link(&mut self, a: ProjectId, b: ProjectId) {
        self.links.insert(pair(a, b));
    }

    pub fn resolve_dependency(&self, a: ProjectId, b: ProjectId) -> Resolution {
        if self.links.contains(&pair(a, b)) {
            Resolution::Resolved
        } else {
            Resolution::Isolated
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::project::atomic_write(path, &bytes)
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(std::io::Error::other)
    }
}
