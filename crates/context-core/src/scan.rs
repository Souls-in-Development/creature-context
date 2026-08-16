use crate::{
    green::evaluate_snapshot,
    project::{ProjectPaths, atomic_write, init_project},
    purpose::read_purpose,
};
use creature_context_types::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    ".creature",
    "target",
    ".build",
    ".release-build",
    // Xcode DerivedData: build output and vendored SwiftPM dependency checkouts,
    // never project source. The `.noindex` suffix Xcode stamps on build folders is
    // caught generically in the walk below, but the containing DerivedData folder
    // holds un-suffixed `SourcePackages/checkouts` (third-party source) too, so the
    // common folder names are skipped whole.
    "DerivedData",
    ".build-derived",
    "node_modules",
    ".swiftpm",
    "dist",
    "build",
    ".cache",
    "vendor",
    // Python dependency and tool-cache directories: third-party or generated, never
    // project source. Virtual environments themselves are caught by their
    // `pyvenv.cfg` marker below, whatever they are named.
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    "site-packages",
];

pub use crate::config::{ScanConfig, ScanLimits, ScanScope};

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("hierarchy error: {0}")]
    Hierarchy(String),
}

/// How a walk ended: what it collected, and whether a ceiling cut it short.
///
/// Truncation is data, not an error. A scan that stopped at a limit still
/// produced a real, usable Atlas of what it did see — failing the whole scan
/// instead would leave the project with nothing and, in the daemon, kill the
/// process. What must never happen is a truncated Atlas that *looks* complete,
/// so the reason is carried out of the walk and recorded on the root entity.
#[derive(Clone, Debug, Default)]
struct WalkOutcome {
    truncated: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IdentityRecord {
    id: EntityId,
    #[serde(default = "file_record_kind")]
    record_kind: String,
    relative_path: String,
    content_fingerprint: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct IdentityRegistry {
    records: Vec<IdentityRecord>,
    #[serde(default)]
    last_snapshot: Option<SnapshotId>,
    #[serde(default)]
    observed_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ScannedFile {
    relative_path: String,
    fingerprint: String,
    id: EntityId,
    /// The deterministic one-line summary, computed from the bytes at read time.
    summary: String,
    /// Lower-cased tokens found on this file's import lines, kept so cross-file
    /// import edges can be resolved without re-reading the file. Extracted at read
    /// time; the bytes are dropped immediately after.
    import_tokens: Vec<String>,
}

/// The scan reduced to identity: every kept file with its reconciled id, plus the
/// derived System/Planet id maps and the snapshot metadata. Heavy `AtlasEntity`
/// objects are NOT built here — that is `build_snapshot` / `build_directory_entities`,
/// so a caller that wants to stream can build entities a directory at a time from
/// this without ever holding the whole tree.
pub struct ScanIndex {
    /// The scanned root, so `write_registry` can resolve `.creature/…` and the
    /// orchestrator can resolve folder paths for `ATLAS.idx` writes.
    pub root: std::path::PathBuf,
    pub root_identity: crate::project::ProjectIdentity,
    /// The Galaxy's canonical name — the canonicalized root's basename, computed
    /// once here so `build_roots` need not re-canonicalize (and stay infallible).
    pub project_name: String,
    pub files: Vec<ScannedFile>,
    pub snapshot_id: SnapshotId,
    pub observed_at: String,
    pub truncated: Option<String>,
    pub purpose: crate::purpose::PurposeDocument,
    /// system_name -> System entity id
    pub system_ids: BTreeMap<String, EntityId>,
    /// planet_key   -> Planet entity id
    pub planet_ids: BTreeMap<String, EntityId>,
    /// stable ordering of the groups, for the registry and for building
    pub system_groups: BTreeMap<String, Vec<EntityId>>,
    pub planet_groups: BTreeMap<String, Vec<EntityId>>,
}

/// What `collect_files` keeps for one file: everything derived from its bytes at
/// read time, with the bytes already dropped. Reconciling the id (which needs the
/// previous registry) turns this into a `ScannedFile`. Keeping this rather than the
/// bytes is what lets a large tree be scanned without holding its contents at once.
#[derive(Clone, Debug)]
struct RawFile {
    relative_path: String,
    fingerprint: String,
    summary: String,
    import_tokens: Vec<String>,
}

pub struct ScanEvidenceContext<'a> {
    pub snapshot: &'a SnapshotId,
    pub observed_at: &'a str,
}

/// Scan `root` with its own `.creature/config.toml`. This is what the pipeline
/// calls, so editing that file changes what is indexed — the limits and scope are
/// configuration, not constants baked into the binary.
pub fn scan_project_configured(root: &Path) -> Result<AtlasSnapshot, ScanError> {
    scan_project_with(root, &ScanConfig::load(root))
}

/// Scan with an explicit configuration, which is what a caller supplying its own
/// scope or ceilings uses. Composed of the two halves the streaming orchestrator
/// also uses: `scan_index` (all the cheap id/grouping work, no heavy entities) then
/// `build_snapshot` (the heavy `AtlasEntity` construction). The output is
/// byte-identical to the pre-split monolith.
pub fn scan_project_with(root: &Path, config: &ScanConfig) -> Result<AtlasSnapshot, ScanError> {
    let index = scan_index(root, config)?;
    build_snapshot(root, &index)
}

/// The identity half of the scan: walk the tree, reconcile every file/system/planet
/// id, and derive the snapshot metadata — **without** building any heavy
/// `AtlasEntity`. All id, fingerprint and ordering logic is moved here verbatim from
/// the old monolith, so the ids it produces are identical; only the entity
/// construction is deferred to `build_snapshot` / `build_directory_entities`.
pub fn scan_index(root: &Path, config: &ScanConfig) -> Result<ScanIndex, ScanError> {
    let identity = init_project(root)?;
    let paths = ProjectPaths::new(root);
    let previous = load_registry(&paths.registry)?;
    let mut raw_files = Vec::new();
    let mut outcome = WalkOutcome::default();

    // With no `include`, the root itself is the subject. With one, only the named
    // subtrees are walked — which is what lets a home directory or a Library
    // folder be a root without indexing everything under it.
    if config.scope.include.is_empty() {
        collect_files(root, root, config, &mut raw_files, &mut 0u64, &mut outcome)?;
    } else {
        let mut total = 0u64;
        for included in &config.scope.include {
            let directory = root.join(included);
            if !directory.is_dir() {
                continue; // a named scope that is absent is not an error
            }
            collect_files(
                root,
                &directory,
                config,
                &mut raw_files,
                &mut total,
                &mut outcome,
            )?;
        }
    }
    let truncated = outcome.truncated.clone();
    raw_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let current_paths: BTreeSet<_> = raw_files
        .iter()
        .map(|raw| raw.relative_path.clone())
        .collect();
    let mut files = Vec::new();
    for raw in raw_files {
        let id = reconcile_id(
            &previous,
            "file",
            &raw.relative_path,
            &raw.fingerprint,
            &current_paths,
            identity.project_id,
        );
        files.push(ScannedFile {
            relative_path: raw.relative_path,
            fingerprint: raw.fingerprint,
            id,
            summary: raw.summary,
            import_tokens: raw.import_tokens,
        });
    }
    let snapshot_id = snapshot_id(&files);
    let observed_at = if previous.last_snapshot.as_ref() == Some(&snapshot_id) {
        previous.observed_at.clone().unwrap_or_else(current_rfc3339)
    } else {
        current_rfc3339()
    };
    let purpose = read_purpose(root)?.unwrap_or_default();
    let canonical_root = fs::canonicalize(root)?;
    let project_name = canonical_root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut system_groups: BTreeMap<String, Vec<EntityId>> = BTreeMap::new();
    let mut planet_groups: BTreeMap<String, Vec<EntityId>> = BTreeMap::new();
    for file in &files {
        let (system_name, _, planet_key) = group_keys(&file.relative_path);
        system_groups.entry(system_name).or_default().push(file.id);
        planet_groups.entry(planet_key).or_default().push(file.id);
    }
    let current_system_paths: BTreeSet<_> = system_groups.keys().cloned().collect();
    let current_planet_paths: BTreeSet<_> = planet_groups.keys().cloned().collect();
    let system_ids: BTreeMap<_, _> = system_groups
        .iter()
        .map(|(path, ids)| {
            let fingerprint = group_fingerprint(ids);
            (
                path.clone(),
                reconcile_id(
                    &previous,
                    "system",
                    path,
                    &fingerprint,
                    &current_system_paths,
                    identity.project_id,
                ),
            )
        })
        .collect();
    let planet_ids: BTreeMap<_, _> = planet_groups
        .iter()
        .map(|(path, ids)| {
            let fingerprint = group_fingerprint(ids);
            (
                path.clone(),
                reconcile_id(
                    &previous,
                    "planet",
                    path,
                    &fingerprint,
                    &current_planet_paths,
                    identity.project_id,
                ),
            )
        })
        .collect();

    Ok(ScanIndex {
        root: root.to_path_buf(),
        root_identity: identity,
        project_name,
        files,
        snapshot_id,
        observed_at,
        truncated,
        purpose,
        system_ids,
        planet_ids,
        system_groups,
        planet_groups,
    })
}

/// The Universe and Galaxy entities plus the Universe→Galaxy `contains` edge — the
/// shared roots every path emits once. Moved verbatim from the old monolith
/// (including `purpose_clauses`, `protected_decision_ids`, the empty-goals and
/// truncation `uncertainty` pushes), reading everything from `index`.
pub fn build_roots(
    index: &ScanIndex,
    ctx: &ScanEvidenceContext<'_>,
) -> (AtlasEntity, AtlasEntity, AtlasEdge) {
    let identity = &index.root_identity;
    let universe = atlas_entity(
        identity.universe_id,
        "Local Universe",
        ScopeScale::Universe,
        EntityKind::Registry,
        None,
        None,
        ctx,
    );
    let mut galaxy = atlas_entity(
        identity.galaxy_id,
        &index.project_name,
        ScopeScale::Galaxy,
        EntityKind::Product,
        Some(identity.universe_id),
        None,
        ctx,
    );
    galaxy.purpose_clauses = index.purpose.goals.clone();
    galaxy.protected_decision_ids = index
        .purpose
        .protected_decisions
        .iter()
        .filter_map(|id| uuid::Uuid::parse_str(id).ok().map(RecordId))
        .collect();
    if galaxy.purpose_clauses.is_empty() {
        galaxy
            .uncertainty
            .push("PURPOSE.md is missing or contains no project goals".into());
    }
    // A truncated scan produced a real Atlas of part of the root. It must never
    // pass for a complete one, so the project entity carries the reason — visible
    // in status, in the IDX projection, and in any Orbit built from it.
    if let Some(reason) = &index.truncated {
        galaxy.uncertainty.push(reason.clone());
    }
    let contains_edge = contains(
        identity.universe_id,
        identity.galaxy_id,
        &index.snapshot_id,
        &index.observed_at,
    );
    (universe, galaxy, contains_edge)
}

/// Write `.creature`'s identity registry from the index — the `IdentityRecord`s for
/// every file/system/planet, `last_snapshot` and `observed_at`. Moved verbatim from
/// the old monolith so both the monolith and the streaming orchestrator write the
/// registry the same way.
pub fn write_registry(index: &ScanIndex) -> Result<(), ScanError> {
    let paths = ProjectPaths::new(&index.root);
    let mut records: Vec<_> = index
        .files
        .iter()
        .map(|f| IdentityRecord {
            id: f.id,
            record_kind: "file".into(),
            relative_path: f.relative_path.clone(),
            content_fingerprint: f.fingerprint.clone(),
        })
        .collect();
    records.extend(index.system_groups.iter().map(|(path, ids)| IdentityRecord {
        id: index.system_ids[path],
        record_kind: "system".into(),
        relative_path: path.clone(),
        content_fingerprint: group_fingerprint(ids),
    }));
    records.extend(index.planet_groups.iter().map(|(path, ids)| IdentityRecord {
        id: index.planet_ids[path],
        record_kind: "planet".into(),
        relative_path: path.clone(),
        content_fingerprint: group_fingerprint(ids),
    }));
    records.sort_by_key(|record| (record.record_kind.clone(), record.relative_path.clone()));
    let registry = IdentityRegistry {
        records,
        last_snapshot: Some(index.snapshot_id.clone()),
        observed_at: Some(index.observed_at.clone()),
    };
    atomic_write(
        &paths.registry,
        &serde_json::to_vec_pretty(&registry).map_err(io::Error::other)?,
    )?;
    Ok(())
}

/// The entity half of the monolith scan: build every entity and edge from the index
/// and produce the evaluated snapshot. Byte-identical to the pre-split function
/// because it runs the same per-file loop over the same ids, in the same order.
pub(crate) fn build_snapshot(root: &Path, index: &ScanIndex) -> Result<AtlasSnapshot, ScanError> {
    let paths = ProjectPaths::new(root);
    let ctx = ScanEvidenceContext {
        snapshot: &index.snapshot_id,
        observed_at: &index.observed_at,
    };
    let identity = &index.root_identity;

    let mut entities = Vec::new();
    let mut edges = Vec::new();
    let (universe, galaxy, root_edge) = build_roots(index, &ctx);
    entities.extend([universe, galaxy]);
    edges.push(root_edge);

    // System and planet entities are created the first time a file names them.
    // The membership test used to be `entities.iter().any(...)`, a linear scan of
    // the growing entity vector for every file — O(files × entities). A set of
    // the ids already created answers the same question in O(1) and produces the
    // identical entity and edge set.
    let mut created_groups: BTreeSet<EntityId> = BTreeSet::new();
    for file in &index.files {
        let path = Path::new(&file.relative_path);
        let (system_name, planet_name, planet_key) = group_keys(&file.relative_path);
        let system_id = index.system_ids[&system_name];
        let planet_id = index.planet_ids[&planet_key];
        if created_groups.insert(system_id) {
            entities.push(atlas_entity(
                system_id,
                &system_name,
                ScopeScale::System,
                EntityKind::Subsystem,
                Some(identity.galaxy_id),
                Some(&system_name),
                &ctx,
            ));
            edges.push(contains(
                identity.galaxy_id,
                system_id,
                &index.snapshot_id,
                &index.observed_at,
            ));
        }
        if created_groups.insert(planet_id) {
            entities.push(atlas_entity(
                planet_id,
                &planet_name,
                ScopeScale::Planet,
                EntityKind::Module,
                Some(system_id),
                Some(&planet_name),
                &ctx,
            ));
            edges.push(contains(
                system_id,
                planet_id,
                &index.snapshot_id,
                &index.observed_at,
            ));
        }
        let mut moon = atlas_entity(
            file.id,
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
            ScopeScale::Moon,
            file_kind(path),
            Some(planet_id),
            Some(&file.relative_path),
            &ctx,
        );
        moon.structural_fingerprint = file.fingerprint.clone();
        moon.deterministic_summary = file.summary.clone();
        entities.push(moon);
        edges.push(contains(
            planet_id,
            file.id,
            &index.snapshot_id,
            &index.observed_at,
        ));
    }
    edges.extend(import_edges(
        &index.files,
        &index.snapshot_id,
        &index.observed_at,
    ));
    let mut snapshot = AtlasSnapshot {
        id: index.snapshot_id.clone(),
        timestamp: current_rfc3339(),
        entities,
        edges,
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    };
    merge_recorded_evidence(&paths.evidence, &mut snapshot)?;
    evaluate_snapshot(&mut snapshot, &GreenPolicy::default())
        .map_err(|e| ScanError::Hierarchy(e.to_string()))?;
    write_registry(index)?;
    Ok(snapshot)
}

/// Build the entities and edges for one directory (Planet), identical to the slice
/// `build_snapshot` produces for it: the System entity (emitted here, but the caller
/// dedups it across the directories under it), the Planet entity, and one Moon per
/// file plus their `contains` edges. Universe/Galaxy are built once by the caller
/// (`build_roots`), not here. Ids are taken from the index — never recomputed — so
/// the slice is byte-identical to the monolith's.
pub fn build_directory_entities(
    index: &ScanIndex,
    planet_key: &str,
    ctx: &ScanEvidenceContext<'_>,
) -> (Vec<AtlasEntity>, Vec<AtlasEdge>) {
    let identity = &index.root_identity;
    let mut entities = Vec::new();
    let mut edges = Vec::new();
    let mut roots_emitted = false;
    // The files sorted (as in the index) and gated to this planet — the same file
    // order build_snapshot walks, so the moon construction is identical.
    for file in index
        .files
        .iter()
        .filter(|f| group_keys(&f.relative_path).2 == planet_key)
    {
        let path = Path::new(&file.relative_path);
        let (system_name, planet_name, _planet_key) = group_keys(&file.relative_path);
        let system_id = index.system_ids[&system_name];
        let planet_id = index.planet_ids[planet_key];
        if !roots_emitted {
            roots_emitted = true;
            entities.push(atlas_entity(
                system_id,
                &system_name,
                ScopeScale::System,
                EntityKind::Subsystem,
                Some(identity.galaxy_id),
                Some(&system_name),
                ctx,
            ));
            edges.push(contains(
                identity.galaxy_id,
                system_id,
                &index.snapshot_id,
                &index.observed_at,
            ));
            entities.push(atlas_entity(
                planet_id,
                &planet_name,
                ScopeScale::Planet,
                EntityKind::Module,
                Some(system_id),
                Some(&planet_name),
                ctx,
            ));
            edges.push(contains(
                system_id,
                planet_id,
                &index.snapshot_id,
                &index.observed_at,
            ));
        }
        let mut moon = atlas_entity(
            file.id,
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
            ScopeScale::Moon,
            file_kind(path),
            Some(planet_id),
            Some(&file.relative_path),
            ctx,
        );
        moon.structural_fingerprint = file.fingerprint.clone();
        moon.deterministic_summary = file.summary.clone();
        entities.push(moon);
        edges.push(contains(
            planet_id,
            file.id,
            &index.snapshot_id,
            &index.observed_at,
        ));
    }
    (entities, edges)
}

/// The cross-file import edges for a scanned index — the stitch the layered scan
/// runs after all directories are persisted. Identical to what `build_snapshot`
/// produces internally, because it calls the same `import_edges` over the same files.
pub fn index_import_edges(index: &ScanIndex) -> Vec<AtlasEdge> {
    import_edges(&index.files, &index.snapshot_id, &index.observed_at)
}

/// Test seam: run `scan_index` then build every directory's entities from it and
/// concatenate (plus the universe/galaxy built once), so a test can prove the sum of
/// the per-directory slices equals the whole snapshot's entity set. `#[doc(hidden)]`
/// and reachable only by name — not part of the intended public surface.
#[doc(hidden)]
pub fn build_all_directories_for_test(root: &Path) -> Vec<AtlasEntity> {
    let index = scan_index(root, &ScanConfig::load(root)).expect("scan_index");
    let ctx = ScanEvidenceContext {
        snapshot: &index.snapshot_id,
        observed_at: &index.observed_at,
    };
    let (universe, galaxy, _edge) = build_roots(&index, &ctx);
    let mut out = vec![universe, galaxy];
    for planet_key in index.planet_groups.keys() {
        let (entities, _edges) = build_directory_entities(&index, planet_key, &ctx);
        out.extend(entities);
    }
    out
}

/// The small, intentional public surface the CLI orchestrator consumes to stream
/// the scan a directory at a time. Everything here reads from a `ScanIndex`, so the
/// orchestrator never holds the whole entity tree.
pub mod streaming {
    pub use super::{
        ScanEvidenceContext, ScanIndex, build_directory_entities, build_roots, index_import_edges,
        scan_index, write_registry,
    };
}

fn collect_files(
    root: &Path,
    directory: &Path,
    config: &ScanConfig,
    output: &mut Vec<RawFile>,
    total: &mut u64,
    outcome: &mut WalkOutcome,
) -> Result<(), ScanError> {
    let limits = config.limits();
    // A directory that cannot be read — a permission-denied Library subfolder, a
    // vanished temp dir — is skipped rather than fatal. Walking a general root
    // like a home directory means meeting these routinely, and one unreadable
    // folder must not cost the whole Atlas.
    let Ok(read) = fs::read_dir(directory) else {
        return Ok(());
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if outcome.truncated.is_some() {
            return Ok(()); // a ceiling was reached; stop walking, keep what we have
        }
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            // `*.noindex` is Xcode's own marker for build folders it wants Spotlight
            // (and us) to skip — the generic, name-independent build-artifact signal.
            if SKIPPED_DIRECTORIES.contains(&name.as_str())
                || name.ends_with(".noindex")
                || config.scope.excludes(&name)
            {
                continue;
            }
            // A directory holding a `pyvenv.cfg` is a Python virtual environment —
            // interpreter and installed dependency code, not project source. Skip
            // it whatever it is named (`.venv`, `.venv311`, `venv`, `env`, …), the
            // same way `node_modules` and `vendor` are skipped by name. On a large
            // repo this is the difference between indexing the project and indexing
            // every pip package inside it.
            if path.join("pyvenv.cfg").is_file() {
                continue;
            }
            collect_files(root, &path, config, output, total, outcome)?;
            continue;
        }
        if !metadata.is_file() || metadata.len() > limits.max_file_bytes {
            continue;
        }
        if matches!(
            entry.file_name().to_string_lossy().as_ref(),
            "ATLAS.idx" | ".atlas.yaml" | ".module-map.yaml" | ".DS_Store"
        ) {
            continue;
        }
        if limits.files_exhausted(output.len()) {
            outcome.truncated = Some(format!(
                "scan truncated at the configured max_files ({}); the Atlas covers \
                 only part of this root",
                limits.max_files
            ));
            return Ok(());
        }
        if limits.bytes_exhausted(*total + metadata.len()) {
            outcome.truncated = Some(format!(
                "scan truncated at the configured max_total_bytes ({}); the Atlas \
                 covers only part of this root",
                limits.max_total_bytes
            ));
            return Ok(());
        }
        *total += metadata.len();
        let relative = path
            .strip_prefix(root)
            .map_err(io::Error::other)?
            .to_string_lossy()
            .replace('\\', "/");
        // Read the bytes once, reduce them to what the Atlas needs — fingerprint,
        // summary, import tokens — and drop them before the next file. This is what
        // keeps a large tree's contents out of memory: only the small per-file
        // record survives, never the bytes.
        let bytes = fs::read(&path)?;
        // Computed before the struct literal moves `relative`.
        let import_tokens = extract_import_tokens(&relative, &bytes);
        output.push(RawFile {
            relative_path: relative,
            fingerprint: blake3::hash(&bytes).to_hex().to_string(),
            summary: summarize_file(&path, &bytes),
            import_tokens,
        });
    }
    Ok(())
}

fn load_registry(path: &Path) -> io::Result<IdentityRegistry> {
    if !path.exists() {
        return Ok(IdentityRegistry::default());
    }
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}

fn snapshot_id(files: &[ScannedFile]) -> SnapshotId {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update(file.fingerprint.as_bytes());
    }
    SnapshotId(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn stable_id(project: ProjectId, kind: &str, key: &str) -> EntityId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(project.0.as_bytes());
    hasher.update(kind.as_bytes());
    hasher.update(key.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    EntityId(uuid::Uuid::from_bytes(bytes))
}

fn reconcile_id(
    previous: &IdentityRegistry,
    kind: &str,
    path: &str,
    fingerprint: &str,
    current_paths: &BTreeSet<String>,
    project: ProjectId,
) -> EntityId {
    if let Some(record) = previous
        .records
        .iter()
        .find(|record| record.record_kind == kind && record.relative_path == path)
    {
        return record.id;
    }
    let candidates: Vec<_> = previous
        .records
        .iter()
        .filter(|record| {
            record.record_kind == kind
                && record.content_fingerprint == fingerprint
                && !current_paths.contains(&record.relative_path)
        })
        .collect();
    if candidates.len() == 1 {
        candidates[0].id
    } else {
        stable_id(project, kind, path)
    }
}

fn group_keys(relative_path: &str) -> (String, String, String) {
    let path = Path::new(relative_path);
    let components: Vec<_> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect();
    let system_name = if components.len() > 1 {
        components[0].clone()
    } else {
        "root".into()
    };
    let planet_name = if components.len() > 2 {
        components[..components.len() - 1].join("/")
    } else {
        system_name.clone()
    };
    let planet_key = format!("{system_name}/{planet_name}");
    (system_name, planet_name, planet_key)
}

fn group_fingerprint(ids: &[EntityId]) -> String {
    let mut ids = ids.to_vec();
    ids.sort();
    let mut hasher = blake3::Hasher::new();
    for id in ids {
        hasher.update(id.0.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn file_record_kind() -> String {
    "file".into()
}

fn atlas_entity(
    id: EntityId,
    name: &str,
    scale: ScopeScale,
    kind: EntityKind,
    parent_id: Option<EntityId>,
    path: Option<&str>,
    context: &ScanEvidenceContext<'_>,
) -> AtlasEntity {
    let evidence = [GreenAxis::Content, GreenAxis::Structure]
        .into_iter()
        .map(|axis| Evidence {
            axis,
            source: FactSource::Parsed,
            proof: ProofStrength::Metadata,
            outcome: EvidenceOutcome::Pass,
            confidence: 1.0,
            fingerprint: context.snapshot.0.clone(),
            observed_at: context.observed_at.into(),
            producer: "creature-context-scanner".into(),
            snapshot_id: context.snapshot.clone(),
            message: String::new(),
        })
        .collect();
    AtlasEntity {
        id,
        scale,
        kind,
        canonical_name: name.into(),
        aliases: vec![],
        parent_id,
        relative_path: path.map(str::to_owned),
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        deterministic_summary: String::new(),
        local_evidence: evidence,
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        inferred_summaries: vec![],
        uncertainty: vec![],
        observed_at: context.observed_at.into(),
        fresh_until: None,
        snapshot_id: context.snapshot.clone(),
        structural_fingerprint: String::new(),
    }
}

fn contains(
    source: EntityId,
    target: EntityId,
    snapshot: &SnapshotId,
    observed_at: &str,
) -> AtlasEdge {
    AtlasEdge {
        id: edge_id(source, target, "contains"),
        source_entity_id: source,
        target_entity_id: target,
        kind: RelationshipKind::Contains,
        plane: RelationshipPlane::Declared,
        proof_record_ids: vec![],
        required: true,
        evidence: vec![Evidence {
            axis: GreenAxis::Integration,
            source: FactSource::Parsed,
            proof: ProofStrength::Metadata,
            outcome: EvidenceOutcome::Pass,
            confidence: 1.0,
            fingerprint: snapshot.0.clone(),
            observed_at: observed_at.into(),
            producer: "creature-context-scanner".into(),
            snapshot_id: snapshot.clone(),
            message: String::new(),
        }],
        source_id: "scanner".into(),
        confidence: 1.0,
        observed_at: observed_at.into(),
        fresh_until: None,
        snapshot_id: snapshot.clone(),
    }
}

fn edge_id(source: EntityId, target: EntityId, kind: &str) -> EdgeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(source.0.as_bytes());
    hasher.update(target.0.as_bytes());
    hasher.update(kind.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    EdgeId(uuid::Uuid::from_bytes(bytes))
}

fn file_kind(path: &Path) -> EntityKind {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if name.contains("test") || name.contains("spec") {
        EntityKind::Test
    } else if matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("png" | "jpg" | "jpeg" | "gif" | "svg" | "json" | "yaml" | "yml" | "toml")
    ) {
        EntityKind::Resource
    } else {
        EntityKind::File
    }
}

fn summarize_file(path: &Path, bytes: &[u8]) -> String {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown");
    let lines =
        bytes.iter().filter(|byte| **byte == b'\n').count() + usize::from(!bytes.is_empty());
    format!("{extension} file, {lines} line(s), {} byte(s)", bytes.len())
}

/// The lower-cased tokens on a file's import lines — the candidates cross-file
/// import edges are matched on. Extracted once at read time so the bytes need not
/// be held; the stem matching happens later in `import_edges`. This reproduces the
/// exact token stream the byte-holding version fed the matcher.
/// The language family a path belongs to, or `None` for anything that is not
/// source code. Import syntax only means "import" inside a source file: in prose a
/// line beginning "From the outset..." or "Use the following..." is an ordinary
/// sentence, and treating it as an import statement is what filled the graph with
/// edges like `1421.txt -> not.onnx`. Matching is confined within one family
/// because a `.py` file cannot import a `.ts` file.
fn language_group(relative_path: &str) -> Option<&'static str> {
    let extension = Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())?
        .to_lowercase();
    Some(match extension.as_str() {
        "swift" => "swift",
        "py" | "pyi" => "python",
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => "js",
        "rs" => "rust",
        "go" => "go",
        "java" | "kt" | "kts" => "jvm",
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "m" | "mm" => "c",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        _ => return None,
    })
}

/// Keywords that open an import line, plus the connectives that ride along on it.
/// They are never the imported module's name, but they are real file stems
/// somewhere in a large tree, so leaving them in mints spurious edges.
const IMPORT_STOPWORDS: &[&str] = &[
    "import", "from", "use", "include", "as", "pub", "crate", "self", "super", "mod",
    "extern", "package", "require", "export", "default", "const", "let", "var", "type",
    "static", "public", "private", "internal", "the", "and", "not", "or", "for", "with",
];

fn extract_import_tokens(relative_path: &str, bytes: &[u8]) -> Vec<String> {
    // Only source files have import statements. Everything else — prose, data,
    // notebooks, manifests — is excluded outright.
    if language_group(relative_path).is_none() {
        return Vec::new();
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| {
            let value = line.trim_start();
            value.starts_with("import ")
                || value.starts_with("from ")
                || value.starts_with("use ")
                || value.starts_with("#include")
        })
        .flat_map(|line| line.split(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|token| token.len() > 1)
        .map(|token| token.to_lowercase())
        .filter(|token| !IMPORT_STOPWORDS.contains(&token.as_str()))
        .collect()
}

fn import_edges(files: &[ScannedFile], snapshot: &SnapshotId, observed_at: &str) -> Vec<AtlasEdge> {
    // Key on (language family, stem) so a Python import can never resolve to a
    // TypeScript file. A stem claimed by more than one file in the same family is
    // AMBIGUOUS: a lexical matcher cannot tell which was meant, and the previous
    // map-collect silently kept whichever came last. Drop those instead of inventing
    // a target — a missing edge is honest, a wrong one is not.
    let mut stem_owners: BTreeMap<(&'static str, String), Vec<EntityId>> = BTreeMap::new();
    for file in files {
        let Some(group) = language_group(&file.relative_path) else {
            continue;
        };
        if let Some(stem) = Path::new(&file.relative_path).file_stem() {
            stem_owners
                .entry((group, stem.to_string_lossy().to_lowercase()))
                .or_default()
                .push(file.id);
        }
    }
    let by_stem: BTreeMap<(&'static str, String), EntityId> = stem_owners
        .into_iter()
        .filter_map(|(key, owners)| match owners.as_slice() {
            [only] => Some((key, *only)),
            _ => None,
        })
        .collect();
    let mut edges = BTreeMap::new();
    for file in files {
        let Some(group) = language_group(&file.relative_path) else {
            continue;
        };
        for token in &file.import_tokens {
            if let Some(target) = by_stem
                .get(&(group, token.clone()))
                .copied()
                .filter(|id| *id != file.id)
            {
                let edge = AtlasEdge {
                    id: edge_id(file.id, target, "imports"),
                    source_entity_id: file.id,
                    target_entity_id: target,
                    kind: RelationshipKind::Imports,
                    plane: RelationshipPlane::Observed,
                    proof_record_ids: vec![],
                    required: false,
                    evidence: vec![Evidence {
                        axis: GreenAxis::Integration,
                        source: FactSource::Parsed,
                        proof: ProofStrength::Syntax,
                        outcome: EvidenceOutcome::Pass,
                        confidence: 0.7,
                        fingerprint: snapshot.0.clone(),
                        observed_at: observed_at.into(),
                        producer: "creature-context-import-scanner".into(),
                        snapshot_id: snapshot.clone(),
                        message: "lexical import match".into(),
                    }],
                    source_id: "scanner".into(),
                    confidence: 0.7,
                    observed_at: observed_at.into(),
                    fresh_until: None,
                    snapshot_id: snapshot.clone(),
                };
                edges.insert(edge.id, edge);
            }
        }
    }
    edges.into_values().collect()
}

pub fn current_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn merge_recorded_evidence(path: &Path, snapshot: &mut AtlasSnapshot) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let recorded: Vec<RecordedEvidence> =
        serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    let mut by_id: BTreeMap<_, _> = snapshot
        .entities
        .iter_mut()
        .map(|entity| (entity.id, entity))
        .collect();
    for record in recorded {
        if record.evidence.snapshot_id != snapshot.id {
            continue;
        }
        if let Some(entity) = by_id.get_mut(&record.entity_id) {
            entity.local_evidence.push(record.evidence);
        }
    }
    Ok(())
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u64, u64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u64, day as u64)
}

#[allow(dead_code)]
fn _normalise(path: PathBuf) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod import_edge_tests {
    use super::*;

    fn file(path: &str, n: u128, tokens: &[&str]) -> ScannedFile {
        ScannedFile {
            relative_path: path.into(),
            fingerprint: "f".into(),
            id: EntityId(uuid::Uuid::from_u128(n)),
            summary: "s".into(),
            import_tokens: tokens.iter().map(|t| (*t).to_string()).collect(),
        }
    }

    fn edges_between(files: &[ScannedFile]) -> Vec<(EntityId, EntityId)> {
        import_edges(files, &SnapshotId("blake3:test".into()), "now")
            .into_iter()
            .map(|e| (e.source_entity_id, e.target_entity_id))
            .collect()
    }

    /// Prose is not code. "From the outset..." / "Use the following..." are ordinary
    /// sentences; reading them as import statements is what produced edges like
    /// `1421.txt -> not.onnx` across the corpora.
    #[test]
    fn prose_files_yield_no_import_tokens() {
        let prose = b"From the outset the study used a broad sample.\nUse of the term is contested.\n";
        assert!(extract_import_tokens("corpora/books/1421.txt", prose).is_empty());
        assert!(extract_import_tokens("data/notes.md", prose).is_empty());
    }

    /// The keyword opening the line is never the imported module.
    #[test]
    fn import_keywords_are_not_module_names() {
        let source = b"import Foundation\nfrom collections import OrderedDict\n";
        let tokens = extract_import_tokens("App/Thing.swift", source);
        assert!(tokens.contains(&"foundation".to_string()));
        assert!(!tokens.contains(&"import".to_string()));
        assert!(!tokens.contains(&"from".to_string()));
    }

    /// A Python file cannot import a TypeScript file.
    #[test]
    fn imports_do_not_cross_language_families() {
        let files = vec![
            file("pkg/__init__.py", 1, &["utils"]),
            file("web/utils.ts", 2, &[]),
        ];
        assert!(edges_between(&files).is_empty());
    }

    /// Same family, unique stem: this is the edge that should exist.
    #[test]
    fn a_unique_same_language_stem_resolves() {
        let files = vec![
            file("tests/test_loaders.py", 1, &["loaders"]),
            file("pkg/loaders.py", 2, &[]),
        ];
        assert_eq!(
            edges_between(&files),
            vec![(EntityId(uuid::Uuid::from_u128(1)), EntityId(uuid::Uuid::from_u128(2)))]
        );
    }

    /// A stem claimed by several files in one family is unresolvable lexically.
    /// Dropping it is honest; picking whichever collected last invents a fact.
    #[test]
    fn ambiguous_stems_are_dropped_not_guessed() {
        let files = vec![
            file("app/main.py", 1, &["config"]),
            file("a/config.py", 2, &[]),
            file("b/config.py", 3, &[]),
        ];
        assert!(edges_between(&files).is_empty());
    }
}
