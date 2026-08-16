//! Layered scan: index and persist one directory at a time, holding only a
//! lightweight per-file list — never the whole entity tree.
//!
//! `scan_index` walks the tree and produces the identity of every file (path,
//! fingerprint, reconciled id) plus the System/Planet id maps and the global
//! `snapshot_id` — but no heavy `AtlasEntity` objects. The orchestrator then builds
//! the entities for one directory at a time with `build_directory_entities`,
//! enriches and evaluates that directory, persists it, writes its `ATLAS.idx`
//! layer, and drops it. The heavy entities for other directories never exist yet,
//! so peak memory is O(one directory + the lightweight list), not O(whole tree).
//!
//! Cross-directory work stays global but compact: import edges come from the
//! index's stored tokens (`index_import_edges`); socket resolution is stitched
//! globally after the directory loop (a global provider index + a `defined_names`
//! union resolve every cross-directory `requires`/`provides` pair, then affected
//! planets are re-evaluated with the canonical evaluator); identity is reconciled
//! per directory against the previous snapshot's subtree (loaded from the store one
//! directory at a time), so a moved or renamed declaration keeps its id across a
//! rescan exactly as the monolith does; and folder Green is rolled up from the
//! corrected per-directory results. Byte-identical to the monolith on entities,
//! edges, socket resolutions and Green, on both a first scan and a rescan — the
//! `layered_matches_monolith`, `layered_payload_parity` and `layered_rescan_identity`
//! tests guard it.
//!
//! Known gap (flagged): no root manifest is written, so `rebuild` is not wired for
//! this path.

use creature_context_core::{
    green::{evaluate_entity, evaluate_snapshot},
    identity::reconcile_identity,
    project::ProjectPaths,
    scan::{ScanConfig, ScanEvidenceContext, streaming},
    sockets::{match_name, resolve_against},
};
use creature_context_parsers::enrich::enrich_snapshot_parallel;
use creature_context_store::{
    AtlasRepository,
    idx::{IdxScope, encode_atlas_idx},
};
use creature_context_types::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::error::Error;
use std::path::Path;

/// What the layered scan produced, for the CLI status line.
pub struct LayeredStatus {
    pub directories: usize,
    pub entities: usize,
    pub relationships: usize,
    pub snapshot_id: SnapshotId,
}

/// One `requires` socket seen during the directory loop: the file that owns it, the
/// socket id, the name it wants (already reduced to the match name), and the
/// resolution its own directory assigned — the baseline the global stitch compares
/// against so only genuinely cross-directory links are touched.
struct GlobalRequire {
    file_id: EntityId,
    socket_id: SocketId,
    wanted: String,
    local: SocketResolution,
}

/// Index `root` directory by directory, persisting each layer as it completes.
pub fn scan_layered(
    root: &Path,
    progress: Option<&dyn ScanProgress>,
) -> Result<LayeredStatus, Box<dyn Error>> {
    // Phase A — identity only. No heavy entity tree is built or held; this returns
    // the lightweight per-file list plus the id maps and the snapshot metadata.
    if let Some(progress) = progress {
        progress.stage(ScanStage::Tree, "");
    }
    let index = streaming::scan_index(root, &ScanConfig::load(root))?;
    let snapshot_id = index.snapshot_id.clone();
    let project_id = index.root_identity.project_id;

    let paths = ProjectPaths::new(root);
    if let Some(dir) = paths.database.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut repository = AtlasRepository::open(&paths.database)?;
    // The previous snapshot's id, read once before anything is written (only
    // `finalize_snapshot` moves the pointer). Its rows stay in the store through the
    // whole scan, so each directory can reconcile identity against its prior version
    // and be pruned at the end. `None` on a first scan — nothing to carry forward.
    let prev_snapshot_id = repository.current_snapshot_id()?;

    let ctx = ScanEvidenceContext {
        snapshot: &index.snapshot_id,
        observed_at: &index.observed_at,
    };

    // The shared roots, built once. Persisted now with a placeholder Green; the
    // roll-up rewrites their Green at the end.
    let (universe, galaxy, root_edge) = streaming::build_roots(&index, &ctx);
    let universe_id = universe.id;
    let galaxy_id = galaxy.id;
    repository.upsert_subtree(
        &[universe.clone(), galaxy.clone()],
        std::slice::from_ref(&root_edge),
        &snapshot_id,
    )?;

    let total_dirs = index.planet_groups.len();
    // Green collected per entity so folders can be rolled up once every directory
    // is in: Moons (files + symbols) and Planets carry their evaluated Green;
    // Systems are rolled up from their Planets, not trusted per directory.
    let mut green_of: BTreeMap<EntityId, GreenCode> = BTreeMap::new();
    // The System and Planet entities as built, kept (deduped by id) so the roll-up
    // can re-emit them with corrected Green. Small: one per folder, not per file.
    let mut folder_entities: BTreeMap<EntityId, AtlasEntity> = BTreeMap::new();

    // Cross-folder socket stitch accumulators. Providers are tagged with their
    // global sort key (relative_path, parse_index) so the candidate order matches
    // the monolith's global entity-iteration order. `global_requires` records every
    // requires socket with the resolution its own directory gave it, so the stitch
    // only touches files whose GLOBAL resolution differs. `global_defined_names`
    // unions every directory's symbol + macro names for the humility downgrade.
    let mut global_provides: BTreeMap<String, Vec<(String, u32, SocketId)>> = BTreeMap::new();
    let mut global_requires: Vec<GlobalRequire> = Vec::new();
    let mut global_defined_names: HashSet<String> = HashSet::new();
    // Which planet each file with a requires socket belongs to, so an affected file
    // maps to the planet subtree that must be re-evaluated.
    let mut planet_of_file: BTreeMap<EntityId, EntityId> = BTreeMap::new();

    for (index_in_scan, planet_key) in index.planet_groups.keys().enumerate() {
        if let Some(progress) = progress {
            progress.unit(planet_key, index_in_scan + 1, total_dirs);
        }
        let planet_id = index.planet_ids[planet_key];

        // Build just this directory's entities from the index, then assemble a
        // valid hierarchy (Universe → Galaxy → System → Planet → files) to enrich
        // and evaluate in isolation.
        let (dir_entities, dir_edges) = streaming::build_directory_entities(&index, planet_key, &ctx);
        let mut sub_entities = vec![universe.clone(), galaxy.clone()];
        sub_entities.extend(dir_entities);
        let mut sub_edges = vec![root_edge.clone()];
        sub_edges.extend(dir_edges);
        let mut sub = AtlasSnapshot {
            id: snapshot_id.clone(),
            timestamp: index.observed_at.clone(),
            entities: sub_entities,
            edges: sub_edges,
            records: vec![],
            conflicts: vec![],
            sources: vec![],
        };

        let mut dir_defined: HashSet<String> = HashSet::new();
        enrich_snapshot_parallel(root, &mut sub, None, Some(&mut dir_defined));

        // Reconcile identity against the previous snapshot's version of this
        // directory, before evaluating — a declaration whose line moved (or that was
        // unambiguously renamed) keeps its id, exactly as the monolith's
        // whole-snapshot `reconcile_identity` does. Matching is file-scoped and a
        // file belongs to one directory, so reconciling per directory against that
        // directory's prior subtree is equivalent to the global pass, while holding
        // only one prior subtree at a time. (Cross-directory moves get fresh ids —
        // the same "later refinement" the monolith reconciler documents.)
        if let Some(prev_id) = &prev_snapshot_id {
            let (prev_entities, _) = repository.load_planet_subtree(planet_id, prev_id)?;
            if !prev_entities.is_empty() {
                let prev_sub = AtlasSnapshot {
                    id: prev_id.clone(),
                    timestamp: index.observed_at.clone(),
                    entities: prev_entities,
                    edges: vec![],
                    records: vec![],
                    conflicts: vec![],
                    sources: vec![],
                };
                reconcile_identity(&prev_sub, &mut sub);
            }
        }

        evaluate_snapshot(&mut sub, &GreenPolicy::default())?;
        global_defined_names.extend(dir_defined);

        // Harvest this directory's providers and requires for the global stitch.
        // Providers: every `Provides` socket, tagged with (relative_path,
        // parse_index). Within this sub a file's symbols are contiguous and in parse
        // order, so parse_index is the running count per relative_path — which
        // reproduces the position the monolith's global iteration gives within the
        // file. Sorting globally by (relative_path, parse_index) later reproduces the
        // whole-snapshot order, since files are disjoint across directories and
        // relative_path sorts identically to the scan's file order.
        let mut parse_index_of: BTreeMap<String, u32> = BTreeMap::new();
        for entity in &sub.entities {
            let rel = match entity.relative_path.as_deref() {
                Some(rel) => rel.to_string(),
                None => continue,
            };
            for socket in &entity.sockets {
                match socket.direction {
                    SocketDirection::Provides => {
                        let slot = parse_index_of.entry(rel.clone()).or_insert(0);
                        let parse_index = *slot;
                        *slot += 1;
                        global_provides
                            .entry(match_name(&socket.shape.qualified_name).to_string())
                            .or_default()
                            .push((rel.clone(), parse_index, socket.id));
                    }
                    SocketDirection::Requires => {
                        global_requires.push(GlobalRequire {
                            file_id: entity.id,
                            socket_id: socket.id,
                            wanted: match_name(&socket.shape.qualified_name).to_string(),
                            local: socket.resolution.clone(),
                        });
                        planet_of_file.insert(entity.id, planet_id);
                    }
                }
            }
        }

        // Record Green and keep the folder entities. A Planet's Green is final here
        // (its only children are its files). A System's is partial (this is one of
        // its directories), so its Green is left to the roll-up.
        for entity in &sub.entities {
            match entity.scale {
                ScopeScale::Moon => {
                    green_of.insert(entity.id, overall(entity));
                }
                ScopeScale::Planet => {
                    green_of.insert(entity.id, overall(entity));
                    folder_entities.insert(entity.id, entity.clone());
                }
                ScopeScale::System => {
                    folder_entities.entry(entity.id).or_insert_with(|| entity.clone());
                }
                _ => {}
            }
        }

        // Persist this directory: everything but the shared roots (written once).
        let persist_entities: Vec<AtlasEntity> = sub
            .entities
            .iter()
            .filter(|e| e.id != universe_id && e.id != galaxy_id)
            .cloned()
            .collect();
        let persist_edges: Vec<AtlasEdge> = sub
            .edges
            .iter()
            .filter(|e| e.source_entity_id != universe_id)
            .cloned()
            .collect();
        repository.upsert_subtree(&persist_entities, &persist_edges, &snapshot_id)?;

        // Write the directory's ATLAS.idx layer from its own subtree.
        if let Some(planet) = sub.entities.iter().find(|e| e.id == planet_id) {
            if let Some(folder_path) = planet.relative_path.as_deref() {
                let dir = root.join(folder_path);
                if dir.is_dir() {
                    let idx = encode_atlas_idx(&sub, IdxScope::Folder(planet_id), &project_id)?;
                    atomic_write(&dir.join("ATLAS.idx"), idx.as_bytes())?;
                }
            }
        }

        creature_context_runtime::metadata::apply(root, &sub);
    }

    // ---- Cross-folder socket stitch ----
    // Freeze the global provider index: sort each name's candidates by
    // (relative_path, parse_index) so the Ambiguous candidate order matches the
    // monolith's global entity-iteration order, then drop the keys to SocketIds.
    let global_candidates: BTreeMap<String, Vec<SocketId>> = global_provides
        .into_iter()
        .map(|(name, mut tagged)| {
            tagged.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            (name, tagged.into_iter().map(|(_, _, id)| id).collect())
        })
        .collect();

    // Resolve every requires socket globally; humility downgrades a global NoMatch
    // whose wanted name is defined-but-invisible anywhere. Collect, per affected
    // file, the socket ids whose global resolution differs from the local one, and
    // the planets those files belong to.
    let empty: Vec<SocketId> = Vec::new();
    let mut new_resolution: BTreeMap<SocketId, SocketResolution> = BTreeMap::new();
    let mut affected_planets: BTreeSet<EntityId> = BTreeSet::new();
    for req in &global_requires {
        let candidates = global_candidates.get(&req.wanted).unwrap_or(&empty);
        let mut global = resolve_against(candidates);
        if matches!(
            &global,
            SocketResolution::Hole(hole) if hole.reason == HoleReason::NoMatch
        ) && global_defined_names.contains(&req.wanted)
        {
            global = SocketResolution::Unresolved;
        }
        if global != req.local {
            new_resolution.insert(req.socket_id, global);
            if let Some(planet) = planet_of_file.get(&req.file_id) {
                affected_planets.insert(*planet);
            }
        }
    }

    // Re-evaluate each affected planet's subtree in isolation with the corrected
    // socket resolutions, using the canonical evaluator so the recomputed Green is
    // identical to the monolith's. Bounded: one planet subtree at a time.
    for planet_id in &affected_planets {
        let planet_id = *planet_id;
        let planet_key = index
            .planet_ids
            .iter()
            .find(|(_, id)| **id == planet_id)
            .map(|(key, _)| key.clone());
        let system_id = planet_key
            .as_deref()
            .and_then(|pk| index.system_ids.get(planet_key_system(pk)))
            .copied();
        let (subtree_entities, subtree_edges) =
            repository.load_planet_subtree(planet_id, &snapshot_id)?;

        // Rebuild a valid rooted subtree: shared roots + this planet's System +
        // Planet + files + symbols. The Planet and System come from folder_entities
        // (kept during the loop); the System is needed so the Planet's parent is
        // present for hierarchy validation.
        let mut sub_entities = vec![universe.clone(), galaxy.clone()];
        if let Some(system_id) = system_id {
            if let Some(system) = folder_entities.get(&system_id) {
                sub_entities.push(system.clone());
            }
        }
        if let Some(planet) = folder_entities.get(&planet_id) {
            sub_entities.push(planet.clone());
        }
        sub_entities.extend(subtree_entities);

        // Patch the corrected socket resolutions onto the file entities.
        for entity in &mut sub_entities {
            for socket in &mut entity.sockets {
                if let Some(res) = new_resolution.get(&socket.id) {
                    socket.resolution = res.clone();
                }
            }
        }

        let mut sub = AtlasSnapshot {
            id: snapshot_id.clone(),
            timestamp: index.observed_at.clone(),
            entities: sub_entities,
            edges: subtree_edges,
            records: vec![],
            conflicts: vec![],
            sources: vec![],
        };
        evaluate_snapshot(&mut sub, &GreenPolicy::default())?;

        // Harvest corrected Green and re-persist the planet + its files. Symbols own
        // no requires sockets, so their Green is unchanged and they need not be
        // rewritten. Update green_of so the folder roll-up sees the new Green.
        let mut to_persist: Vec<AtlasEntity> = Vec::new();
        for entity in &sub.entities {
            match entity.scale {
                ScopeScale::Planet if entity.id == planet_id => {
                    green_of.insert(entity.id, overall(entity));
                    folder_entities.insert(entity.id, entity.clone());
                    to_persist.push(entity.clone());
                }
                ScopeScale::Moon if entity.parent_id == Some(planet_id) => {
                    green_of.insert(entity.id, overall(entity));
                    to_persist.push(entity.clone());
                }
                _ => {}
            }
        }
        repository.upsert_subtree(&to_persist, &[], &snapshot_id)?;
    }
    // ---- end stitch ----

    // Cross-directory import edges — the global stitch, from the index's stored
    // tokens, added after the per-directory contains edges are all in.
    let import_edges = streaming::index_import_edges(&index);
    repository.upsert_subtree(&[], &import_edges, &snapshot_id)?;

    // Roll folder Green up from the per-directory results: a System is the weakest
    // of its Planets, the Galaxy the weakest of its Systems, the Universe the
    // Galaxy's. Planets already carry their final Green.
    let mut rolled: Vec<AtlasEntity> = Vec::new();
    for (system_name, system_id) in &index.system_ids {
        let system_green = index
            .planet_groups
            .keys()
            .filter(|pk| planet_key_system(pk) == system_name.as_str())
            .filter_map(|pk| green_of.get(&index.planet_ids[pk]).copied())
            .fold(GreenCode::Green, weakest);
        green_of.insert(*system_id, system_green);
        if let Some(entity) = folder_entities.get(system_id) {
            let mut entity = entity.clone();
            set_overall(&mut entity, system_green);
            rolled.push(entity);
        }
    }
    // Re-emit Planets with their (final) Green too, so a Planet whose only write so
    // far carried the pre-roll-up value is corrected uniformly.
    for (planet_key, planet_id) in &index.planet_ids {
        if let (Some(entity), Some(code)) =
            (folder_entities.get(planet_id), green_of.get(planet_id))
        {
            let mut entity = entity.clone();
            set_overall(&mut entity, *code);
            rolled.push(entity);
            let _ = planet_key;
        }
    }
    // The Galaxy and Universe carry a placeholder Green (None) from build_roots, so
    // they must be *evaluated*, not patched — `set_overall` no-ops on a None. The
    // isolation-safe evaluator reproduces the monolith's assessment exactly: the
    // Galaxy from its Systems' codes, the Universe from the Galaxy's. Required edges
    // are left empty — the hierarchy `contains` edges carry Green evidence, which
    // folds to no change and no reason, so the assessment is identical either way.
    let policy = GreenPolicy::default();
    let system_greens: Vec<GreenCode> = index
        .system_ids
        .values()
        .filter_map(|id| green_of.get(id).copied())
        .collect();
    let galaxy_assessment =
        evaluate_entity(&galaxy, &snapshot_id, &policy, &system_greens, &[], &[]);
    let galaxy_overall = galaxy_assessment.overall;
    let mut galaxy_rolled = galaxy.clone();
    galaxy_rolled.green = Some(galaxy_assessment);
    let universe_assessment =
        evaluate_entity(&universe, &snapshot_id, &policy, &[galaxy_overall], &[], &[]);
    let mut universe_rolled = universe.clone();
    universe_rolled.green = Some(universe_assessment);
    rolled.push(galaxy_rolled);
    rolled.push(universe_rolled);
    repository.upsert_subtree(&rolled, &[], &snapshot_id)?;

    // Write the identity registry exactly as the monolith does, then finalize.
    streaming::write_registry(&index)?;
    repository.finalize_snapshot(&snapshot_id, universe_id)?;

    let (entities, relationships) = repository.counts()?;
    Ok(LayeredStatus {
        directories: total_dirs,
        entities,
        relationships,
        snapshot_id,
    })
}

/// The System name a planet key belongs to — the segment before the first `/`.
/// Planet keys are `"{system}/{planet_name}"`, so the System name is that prefix.
fn planet_key_system(planet_key: &str) -> &str {
    planet_key.split('/').next().unwrap_or(planet_key)
}

/// An entity's overall Green code, or Unknown when it has not been evaluated.
fn overall(entity: &AtlasEntity) -> GreenCode {
    entity
        .green
        .as_ref()
        .map(|g| g.overall)
        .unwrap_or(GreenCode::Unknown)
}

/// Set an entity's overall Green code, preserving any existing assessment detail.
fn set_overall(entity: &mut AtlasEntity, code: GreenCode) {
    if let Some(green) = entity.green.as_mut() {
        green.overall = code;
    }
}

fn weakest(left: GreenCode, right: GreenCode) -> GreenCode {
    fn rank(code: GreenCode) -> u8 {
        match code {
            GreenCode::Red => 0,
            GreenCode::Unknown => 1,
            GreenCode::Yellow => 2,
            GreenCode::Green => 3,
        }
    }
    if rank(left) <= rank(right) {
        left
    } else {
        right
    }
}

/// Write bytes to `path` via a temporary file and rename, so a reader never sees a
/// half-written `ATLAS.idx`.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = parent.join(".atlas-idx.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}
