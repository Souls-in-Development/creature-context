use creature_context_types::{EntityId, ProjectId};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectIdentity {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub universe_id: EntityId,
    pub galaxy_id: EntityId,
}

#[derive(Clone, Debug)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub creature: PathBuf,
    pub identity: PathBuf,
    pub registry: PathBuf,
    pub database: PathBuf,
    pub journal: PathBuf,
    pub evidence: PathBuf,
    /// Prepare-authority output: proposals never touch project source.
    pub proposals: PathBuf,
    /// Queryable permission ledger. A local index (gitignored); specification
    /// 4.2 names `permissions.jsonl` as the portable form, a later projection.
    pub permissions: PathBuf,
}

impl ProjectPaths {
    pub fn new(root: &Path) -> Self {
        let root = root.to_path_buf();
        let creature = root.join(".creature");
        Self {
            identity: creature.join("project.yaml"),
            registry: creature.join("identities.json"),
            database: creature.join("atlas.db"),
            journal: creature.join("journal.jsonl"),
            proposals: creature.join("proposals"),
            evidence: creature.join("evidence.json"),
            permissions: creature.join("permissions.db"),
            root,
            creature,
        }
    }
}

pub fn init_project(root: &Path) -> io::Result<ProjectIdentity> {
    fs::create_dir_all(root)?;
    let paths = ProjectPaths::new(root);
    fs::create_dir_all(paths.creature.join("cache"))?;
    if paths.identity.exists() {
        return load_identity(root);
    }
    let identity = ProjectIdentity {
        schema_version: 1,
        project_id: ProjectId::new(),
        universe_id: EntityId::new(),
        galaxy_id: EntityId::new(),
    };
    atomic_write(
        &paths.identity,
        serde_yaml::to_string(&identity)
            .map_err(io::Error::other)?
            .as_bytes(),
    )?;
    // Written once, and actually read on every scan (crate::config).
    atomic_write(
        &paths.creature.join("config.toml"),
        crate::config::DEFAULT_CONFIG_TOML.as_bytes(),
    )?;
    if !paths.journal.exists() {
        atomic_write(&paths.journal, b"")?;
    }
    Ok(identity)
}

pub fn load_identity(root: &Path) -> io::Result<ProjectIdentity> {
    let bytes = fs::read(ProjectPaths::new(root).identity)?;
    serde_yaml::from_slice(&bytes).map_err(io::Error::other)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    {
        use std::io::Write;
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(temp, path)
}
