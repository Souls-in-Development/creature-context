//! Milestone 2 Task 1: the root ATLAS.idx must be galaxy-scoped and complete.
//!
//! Measured before this change on the Creature Context repository itself: the
//! full snapshot held 311 entities and the root ATLAS.idx held 5, because the
//! root path "." was treated as just another meaningful folder and encoded with
//! `IdxScope::Folder`. A rebuild from portable state would have restored 1.6%
//! of the project.
//!
//! Specification 4.1 exempts the root from the non-duplication rule: it is both
//! the Galaxy entry point (5.4) and the rebuild source (4.2, section 20 item 5),
//! and a summarising root restores a partial snapshot.

use creature_context_store::{
    idx::{IdxScope, encode_atlas_idx},
    write_projections,
};
use creature_context_types::ProjectId;
use std::fs;
use std::path::PathBuf;

mod support;
use creature_context_types::ScopeScale;
use support::{entity, snapshot_with};

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "creature-context-projection-root-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src/feature")).expect("create dirs");
    path
}

/// Universe → Galaxy → System → Planet → Moon, so "complete" is measurable
/// rather than trivially true.
fn deep_snapshot() -> creature_context_types::AtlasSnapshot {
    snapshot_with(vec![
        entity("universe", ".", ScopeScale::Universe),
        entity("root", ".", ScopeScale::Galaxy),
        entity("src", "src", ScopeScale::System),
        entity("feature", "src/feature", ScopeScale::Planet),
        entity("deep.rs", "src/feature/deep.rs", ScopeScale::Moon),
    ])
}

#[test]
fn root_projection_is_galaxy_scoped() {
    let root = temp_root("scoped");
    write_projections(&root, &deep_snapshot(), &ProjectId::new()).expect("write");

    let root_idx = fs::read_to_string(root.join("ATLAS.idx")).expect("read root");
    let header = root_idx.lines().next().expect("header");

    assert!(
        header.starts_with("@creature-context v:1 kind:atlas scale:galaxy"),
        "root ATLAS.idx must be galaxy-scoped, got: {header}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_projection_contains_the_complete_snapshot() {
    let root = temp_root("complete");
    let snapshot = deep_snapshot();
    write_projections(&root, &snapshot, &ProjectId::new()).expect("write");

    let root_idx = fs::read_to_string(root.join("ATLAS.idx")).expect("read root");
    let emitted = root_idx
        .lines()
        .filter(|l| l.starts_with("@entity"))
        .count();

    assert_eq!(
        emitted,
        snapshot.entities.len(),
        "root must carry every entity — a rebuild reads this file and nothing else, \
         so a partial root is indistinguishable from data loss"
    );
    assert!(
        root_idx.contains("src/feature/deep.rs"),
        "the deepest entity must be present in the root"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn root_projection_matches_direct_galaxy_encoding() {
    let root = temp_root("identical");
    let snapshot = deep_snapshot();
    let project_id = ProjectId::new();
    write_projections(&root, &snapshot, &project_id).expect("write");

    let from_file = fs::read_to_string(root.join("ATLAS.idx")).expect("read root");
    let direct = encode_atlas_idx(&snapshot, IdxScope::Galaxy, &project_id).expect("encode");

    assert_eq!(
        from_file, direct,
        "the root file must be byte-identical to a direct galaxy encoding, so rebuild and \
         `atlas --format idx` cannot diverge"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_folders_remain_folder_scoped() {
    let root = temp_root("nested");
    write_projections(&root, &deep_snapshot(), &ProjectId::new()).expect("write");

    let nested = fs::read_to_string(root.join("src/ATLAS.idx")).expect("read src");
    let header = nested.lines().next().expect("header");

    assert!(
        header.contains("scale:folder"),
        "only the root is galaxy-scoped; nested files stay folder-scoped, got: {header}"
    );
    let _ = fs::remove_dir_all(&root);
}
