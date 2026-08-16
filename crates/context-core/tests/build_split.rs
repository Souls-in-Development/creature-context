//! Guards for the scan_index / build_snapshot split. The layered scan must reach
//! byte-identical output through the same id/grouping work the monolith does, so
//! these tests pin the monolith's structural output and prove per-directory
//! construction reconstructs the same entity id set.

fn fixture(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("cc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::create_dir_all(root.join("b")).unwrap();
    std::fs::write(root.join("PURPOSE.md"), "# F\n\n## Goals\n- x\n").unwrap();
    std::fs::write(root.join("a/one.rs"), "use two;\npub fn one() {}\n").unwrap();
    std::fs::write(root.join("b/two.rs"), "pub fn two() {}\n").unwrap();
    root
}

#[test]
fn scan_project_with_is_unchanged_by_the_index_split() {
    let root = fixture("split");

    let snap = creature_context_core::scan::scan_project_configured(&root).unwrap();
    let entities = snap.entities.len();
    let edges = snap.edges.len();
    // Structural fingerprint: sorted (scale, name). The Galaxy's canonical name is
    // the (pid-suffixed) temp-dir basename, so normalize just that one entry to a
    // placeholder — everything else is a fixed, deterministic structural claim.
    let mut names: Vec<String> = snap
        .entities
        .iter()
        .map(|e| {
            if e.scale == creature_context_types::ScopeScale::Galaxy {
                "Galaxy:<root>".to_string()
            } else {
                format!("{:?}:{}", e.scale, e.canonical_name)
            }
        })
        .collect();
    names.sort();

    assert_eq!(entities, 11, "entity count must not change");
    assert_eq!(edges, 11, "edge count must not change");
    assert_eq!(
        names,
        vec![
            "Galaxy:<root>",
            "Moon:PURPOSE.md",
            "Moon:one.rs",
            "Moon:two.rs",
            "Planet:a",
            "Planet:b",
            "Planet:root",
            "System:a",
            "System:b",
            "System:root",
            "Universe:Local Universe",
        ],
        "entity set must not change"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn per_directory_build_matches_the_whole_snapshot() {
    let root = fixture("perdir");

    let whole = creature_context_core::scan::scan_project_configured(&root).unwrap();
    // Sum of per-directory builds (via the test seam) must equal the whole
    // snapshot's entity id set. The System entity is emitted by every directory
    // under it, so dedup the per-directory ids before comparing.
    let per_dir = creature_context_core::scan::build_all_directories_for_test(&root);
    let mut whole_ids: Vec<String> = whole.entities.iter().map(|e| e.id.to_string()).collect();
    whole_ids.sort();
    let mut dir_ids: Vec<String> = per_dir.iter().map(|e| e.id.to_string()).collect();
    dir_ids.sort();
    dir_ids.dedup();
    assert_eq!(
        whole_ids, dir_ids,
        "per-directory entities must equal the whole set"
    );
    let _ = std::fs::remove_dir_all(&root);
}
