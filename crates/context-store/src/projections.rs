use creature_context_types::{AtlasEntity, AtlasSnapshot, ProjectId, ScopeScale};
use std::{collections::BTreeMap, fs, io, path::Path};

/// Write the distributed ATLAS.idx tree for a snapshot.
///
/// The root file is galaxy-scoped and contains the complete snapshot, because a
/// rebuild reads it. Each other meaningful folder receives a folder-scoped
/// `ATLAS.idx` containing its subtree, pointing at children with `@child`
/// records rather than duplicating them (specification §4.1).
pub fn write_projections(
    root: &Path,
    snapshot: &AtlasSnapshot,
    project_id: &ProjectId,
) -> io::Result<()> {
    // The root is galaxy-scoped and carries the complete snapshot. It is the
    // one file exempt from the non-duplication rule in specification 4.1,
    // because it is both the Galaxy entry point (5.4) and the portable source a
    // rebuild reads (4.2, section 20 item 5). Encoding it as just another
    // folder restored a partial snapshot — measured at 5 entities of 311 on
    // this repository — which is indistinguishable from data loss.
    //
    // It is encoded from the *original* snapshot, before parent inference
    // below. Inference is a navigation aid for the nested files' @child
    // records; applying it here would make the rebuild source differ from the
    // state it is supposed to restore, and would let the root diverge from
    // `atlas --format idx`.
    let root_idx = crate::idx::encode_atlas_idx(snapshot, crate::idx::IdxScope::Galaxy, project_id)
        .map_err(io::Error::other)?;
    atomic_write(&root.join("ATLAS.idx"), root_idx.as_bytes())?;

    let galaxy_id = snapshot
        .entities
        .iter()
        .find(|e| e.scale == ScopeScale::Galaxy)
        .map(|e| e.id);
    let original_folder_paths = folder_paths(snapshot, galaxy_id);
    let snapshot = snapshot_with_inferred_parents(snapshot, &original_folder_paths);
    let folder_entities: BTreeMap<String, creature_context_types::EntityId> = snapshot
        .entities
        .iter()
        .filter_map(|e| original_folder_paths.get(&e.id).map(|p| (p.clone(), e.id)))
        .collect();
    let meaningful = meaningful_folders(&folder_entities, &snapshot.entities);

    for folder_path in &meaningful {
        // Already written above, galaxy-scoped; must not be overwritten with a
        // folder-scoped encoding.
        if folder_path == "." {
            continue;
        }
        let Some(entity_id) = folder_entities.get(folder_path.as_str()) else {
            continue;
        };
        let idx_str = crate::idx::encode_atlas_idx(
            &snapshot,
            crate::idx::IdxScope::Folder(*entity_id),
            project_id,
        )
        .map_err(io::Error::other)?;
        atomic_write(
            &root.join(folder_path).join("ATLAS.idx"),
            idx_str.as_bytes(),
        )?;
    }

    Ok(())
}

/// Compute the folder path each non-Moon entity represents, using the original
/// parent_id from the snapshot. Folder entities whose parent is the Galaxy, or
/// whose path is the scanner's reserved "root" name, are treated as the project
/// root (`.`).
fn folder_paths(
    snapshot: &AtlasSnapshot,
    galaxy_id: Option<creature_context_types::EntityId>,
) -> BTreeMap<creature_context_types::EntityId, String> {
    snapshot
        .entities
        .iter()
        .filter_map(|e| {
            if e.scale == ScopeScale::Moon {
                return None;
            }
            let path = e.relative_path.as_ref()?;
            let folder = if e.parent_id == galaxy_id || path == "root" {
                ".".to_string()
            } else {
                normalise_folder(path)
            };
            Some((e.id, folder))
        })
        .collect()
}

/// Return a snapshot clone with parent_id set from folder hierarchy.
///
/// Coding agents rely on `@child` records to navigate the distributed Atlas.
/// Synthetic fixtures may not set parent_id, so the projection writer infers it
/// from relative_path prefixes before encoding.
fn snapshot_with_inferred_parents(
    snapshot: &AtlasSnapshot,
    original_folder_paths: &BTreeMap<creature_context_types::EntityId, String>,
) -> AtlasSnapshot {
    let folder_to_entity: BTreeMap<String, creature_context_types::EntityId> = snapshot
        .entities
        .iter()
        .filter_map(|e| original_folder_paths.get(&e.id).map(|p| (p.clone(), e.id)))
        .collect();

    let mut entities = snapshot.entities.clone();
    for entity in &mut entities {
        let Some(folder) = original_folder_paths.get(&entity.id) else {
            continue;
        };
        if folder == "." {
            continue;
        }
        let parent_folder = parent_folder(folder);
        if let Some(parent_id) = folder_to_entity.get(&parent_folder) {
            entity.parent_id = Some(*parent_id);
        }
    }

    AtlasSnapshot {
        id: snapshot.id.clone(),
        timestamp: snapshot.timestamp.clone(),
        entities,
        edges: snapshot.edges.clone(),
        records: snapshot.records.clone(),
        conflicts: snapshot.conflicts.clone(),
        sources: snapshot.sources.clone(),
    }
}

fn parent_folder(folder: &str) -> String {
    if folder == "." {
        return ".".to_string();
    }
    match folder.rfind('/') {
        Some(0) => ".".to_string(),
        Some(index) => folder[..index].to_string(),
        None => ".".to_string(),
    }
}

/// A folder is meaningful when it is represented by at least one entity in the
/// Atlas, or when it is the root. This ensures every folder that carries
/// project structure has a local entry point for coding agents.
fn meaningful_folders(
    folder_entities: &BTreeMap<String, creature_context_types::EntityId>,
    _entities: &[AtlasEntity],
) -> Vec<String> {
    let mut meaningful: Vec<String> = folder_entities.keys().cloned().collect();
    if !meaningful.contains(&".".to_string()) {
        meaningful.push(".".to_string());
    }
    meaningful.sort();
    meaningful
}

fn normalise_folder(path: &str) -> String {
    if path == "." || path.is_empty() {
        ".".to_string()
    } else {
        path.trim_end_matches('/').to_string()
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
