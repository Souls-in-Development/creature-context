//! Milestone 2 Task 2: the disposable database must be reconstructible from
//! the portable root ATLAS.idx alone.
//!
//! Specification 20 item 5: "SQLite can be deleted and reconstructed from
//! portable project state." Until Task 1 the root held 5 entities of 311, so
//! this was not achievable; it is now galaxy-scoped and complete.
//!
//! Rebuild is fail-closed: a malformed or absent root must error rather than
//! produce a partial database, because a silently partial rebuild is
//! indistinguishable from data loss.

use creature_context_store::{
    AtlasRepository, rebuild_repository_from_portable, write_projections,
};
use creature_context_types::ProjectId;
use std::fs;
use std::path::PathBuf;

mod support;
use creature_context_types::ScopeScale;
use support::{entity, snapshot_with};

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "creature-context-rebuild-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join(".creature")).expect("create dirs");
    fs::create_dir_all(path.join("src/feature")).expect("create dirs");
    path
}

fn sample() -> creature_context_types::AtlasSnapshot {
    snapshot_with(vec![
        entity("universe", ".", ScopeScale::Universe),
        entity("root", ".", ScopeScale::Galaxy),
        entity("src", "src", ScopeScale::System),
        entity("feature", "src/feature", ScopeScale::Planet),
        entity("deep.rs", "src/feature/deep.rs", ScopeScale::Moon),
    ])
}

#[test]
fn rebuild_restores_the_complete_snapshot_from_the_root_file() {
    let root = temp_root("complete");
    let snapshot = sample();
    write_projections(&root, &snapshot, &ProjectId::new()).expect("write projections");

    let db = root.join(".creature/atlas.db");
    rebuild_repository_from_portable(&root, &db).expect("rebuild");

    let restored = AtlasRepository::open(&db)
        .expect("open rebuilt db")
        .load_snapshot()
        .expect("load");

    assert_eq!(
        restored.entities.len(),
        snapshot.entities.len(),
        "rebuild must restore every entity — a partial rebuild is indistinguishable from data loss"
    );
    assert_eq!(restored.id, snapshot.id, "snapshot identity must survive");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn deleting_the_database_and_rebuilding_is_lossless() {
    let root = temp_root("lossless");
    let snapshot = sample();
    write_projections(&root, &snapshot, &ProjectId::new()).expect("write projections");

    let db = root.join(".creature/atlas.db");
    let mut repository = AtlasRepository::open(&db).expect("open");
    repository.replace_snapshot(&snapshot).expect("store");
    let before = repository.load_snapshot().expect("load before");
    drop(repository);

    fs::remove_file(&db).expect("delete the disposable database");
    rebuild_repository_from_portable(&root, &db).expect("rebuild");

    let after = AtlasRepository::open(&db)
        .expect("open")
        .load_snapshot()
        .expect("load after");

    assert_eq!(
        before.entities.len(),
        after.entities.len(),
        "the database is disposable; deleting it must lose nothing"
    );
    assert_eq!(before.id, after.id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn rebuild_fails_closed_when_the_root_is_missing() {
    let root = temp_root("missing");
    let db = root.join(".creature/atlas.db");

    let result = rebuild_repository_from_portable(&root, &db);

    assert!(
        result.is_err(),
        "with no portable root there is nothing to rebuild from; producing an empty \
         database would silently discard the project"
    );
    assert!(
        !db.exists(),
        "a failed rebuild must not leave a partial database behind"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn rebuild_fails_closed_on_a_malformed_root() {
    let root = temp_root("malformed");
    fs::write(
        root.join("ATLAS.idx"),
        "@entity id:not-a-uuid scale:nonsense\n",
    )
    .expect("write");
    let db = root.join(".creature/atlas.db");

    let result = rebuild_repository_from_portable(&root, &db);

    assert!(
        result.is_err(),
        "a malformed root must error rather than yield a fabricated snapshot"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn rebuild_refuses_a_folder_scoped_root() {
    // Guards the Task 1 regression directly: a folder-scoped root carries only
    // its own subtree, so rebuilding from one restores a partial snapshot.
    let root = temp_root("folder-scoped");
    let snapshot = sample();
    let folder_scoped = creature_context_store::idx::encode_atlas_idx(
        &snapshot,
        creature_context_store::idx::IdxScope::Folder(snapshot.entities[2].id),
        &ProjectId::new(),
    )
    .expect("encode");
    fs::write(root.join("ATLAS.idx"), folder_scoped).expect("write");

    let result = rebuild_repository_from_portable(&root, &root.join(".creature/atlas.db"));

    assert!(
        result.is_err(),
        "the rebuild source must be galaxy-scoped; accepting a folder-scoped root is how \
         a 5-of-311 snapshot would be restored without anyone noticing"
    );
    let _ = fs::remove_dir_all(&root);
}
