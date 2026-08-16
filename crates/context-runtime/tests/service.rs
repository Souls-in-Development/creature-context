//! The resident daemon runs the same enriched pipeline as the one-shot CLI.
//!
//! Before this the daemon scanned file-level only — no symbols, no sockets — so
//! watch-mode produced a weaker Atlas than a manual `scan`. `reconcile_once` now
//! runs the shared `index_project`, so the daemon's Atlas carries the parsed
//! structure and an evaluated Green baseline just like the CLI's.

use creature_context_core::project::init_project;
use creature_context_runtime::service::reconcile_once;
use creature_context_types::EntityKind;
use std::fs;
use std::path::PathBuf;

fn project(name: &str, body: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cc-service-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("PURPOSE.md"), "# Fixture\n\n## Goals\n- serve\n").unwrap();
    fs::write(root.join("src/lib.rs"), body).unwrap();
    init_project(&root).unwrap();
    root
}

#[test]
fn the_daemon_reconcile_enriches_and_evaluates() {
    let root = project("enrich", "pub fn build() {}\npub struct Widget {}\n");
    let snapshot = reconcile_once(&root).expect("reconcile");

    // Enriched: the parsed symbols are entities, not just files.
    let build = snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == "build" && e.kind != EntityKind::File)
        .expect("the daemon must enrich with parsed symbols, like the CLI");

    // Evaluated: the symbol carries a Green assessment — the daemon runs the same
    // evaluation the one-shot scan does.
    assert!(
        build.green.is_some(),
        "the daemon evaluates Green over the enriched structure"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_daemon_preserves_identity_across_reconciles() {
    let root = project("identity", "pub fn build() {}\n");
    let first = reconcile_once(&root).expect("first");
    let first_id = first
        .entities
        .iter()
        .find(|e| e.canonical_name == "build" && e.kind != EntityKind::File)
        .expect("build")
        .id;

    // A change above the symbol moves it; the daemon reconciles identity against
    // the stored snapshot, so the id survives.
    fs::write(
        root.join("src/lib.rs"),
        "// moved down\n\npub fn build() {}\n",
    )
    .unwrap();
    let second = reconcile_once(&root).expect("second");
    let second_id = second
        .entities
        .iter()
        .find(|e| e.canonical_name == "build" && e.kind != EntityKind::File)
        .expect("build")
        .id;

    assert_eq!(first_id, second_id, "the daemon carries stable ids forward");

    let _ = fs::remove_dir_all(&root);
}
