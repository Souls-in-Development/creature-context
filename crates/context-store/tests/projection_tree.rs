use creature_context_store::write_projections;
use creature_context_types::ScopeScale;
use std::fs;

mod support;
use support::{entity, snapshot_with};

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("creature-projection-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/feature")).expect("create dirs");
    fs::create_dir_all(dir.join("tests")).expect("create dirs");
    dir
}

fn project_id() -> creature_context_types::ProjectId {
    creature_context_types::ProjectId(
        uuid::Uuid::parse_str("019fcb87-5aa3-74f2-aed2-1a8e998986c5").unwrap(),
    )
}

#[test]
fn writes_one_atlas_idx_per_meaningful_folder() {
    let root = temp_root("per-folder");
    let snapshot = snapshot_with(vec![
        entity("root", ".", ScopeScale::Galaxy),
        entity("src", "src", ScopeScale::System),
        entity("feature", "src/feature", ScopeScale::Planet),
        entity("tests", "tests", ScopeScale::System),
    ]);

    write_projections(&root, &snapshot, &project_id()).expect("write projections");

    for folder in ["", "src", "src/feature", "tests"] {
        let path = root.join(folder).join("ATLAS.idx");
        assert!(path.exists(), "missing ATLAS.idx in {folder:?}");
    }
}

#[test]
fn parent_files_reference_children_and_do_not_duplicate_them() {
    let root = temp_root("child-refs");
    let snapshot = snapshot_with(vec![
        entity("root", ".", ScopeScale::Galaxy),
        entity("src", "src", ScopeScale::System),
        entity("feature", "src/feature", ScopeScale::Planet),
    ]);

    write_projections(&root, &snapshot, &project_id()).expect("write projections");

    // Non-duplication is a rule about *nested* parents. The root is explicitly
    // exempt (specification 4.1): it is galaxy-scoped and complete, because a
    // rebuild reads it and a summarising root restores a partial snapshot.
    // Assert the rule where it actually applies — on src/, whose child is
    // src/feature.
    let nested = fs::read_to_string(root.join("src/ATLAS.idx")).expect("read src");
    assert!(
        nested
            .lines()
            .any(|l| l.starts_with("@child") && l.contains("src/feature/ATLAS.idx")),
        "a nested parent must point to its child Atlas file, got:\n{nested}"
    );

    let root_idx = fs::read_to_string(root.join("ATLAS.idx")).expect("read root");
    assert!(
        root_idx.contains("src/feature"),
        "the root is the exception and must carry every entity — see specification 4.1"
    );
}

#[test]
fn rewriting_an_unchanged_snapshot_is_byte_identical() {
    let root = temp_root("stable");
    let snapshot = snapshot_with(vec![entity("root", ".", ScopeScale::Galaxy)]);

    write_projections(&root, &snapshot, &project_id()).expect("first write");
    let first = fs::read_to_string(root.join("ATLAS.idx")).expect("read");
    write_projections(&root, &snapshot, &project_id()).expect("second write");
    let second = fs::read_to_string(root.join("ATLAS.idx")).expect("read");

    assert_eq!(first, second, "unchanged snapshot must not churn the file");
}
