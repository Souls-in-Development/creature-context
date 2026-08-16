//! The shared deterministic index pipeline.
//!
//! `index_project` is the single sequence both the one-shot CLI `scan` and the
//! resident daemon run: scan, enrich with parsed structure, reconcile identity
//! against the previous snapshot, and evaluate Green. Having one pipeline is the
//! point — a scanned Atlas and a watched Atlas must be identical, and before
//! this the daemon scanned file-level only (no symbols, no sockets) while the
//! CLI enriched.

use creature_context_parsers::index::index_project;
use creature_context_types::green::{GreenAxis, GreenCode};
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str, body: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cc-index-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("PURPOSE.md"), "# Fixture\n\n## Goals\n- index\n").unwrap();
    fs::write(root.join("src/lib.rs"), body).unwrap();
    root
}

#[test]
fn the_pipeline_enriches_and_evaluates_in_one_pass() {
    // A symbol plus an unmet intra-repo import (`nowhere`/`Absent` appear nowhere
    // else, so the socket genuinely holes).
    let root = fixture(
        "enrich",
        "use crate::nowhere::Absent;\npub fn build() -> Absent { todo!() }\n",
    );
    let snapshot = index_project(&root, None).expect("index");

    // Enrichment ran: the parsed symbol is an entity.
    let build = snapshot
        .entities
        .iter()
        .find(|e| e.canonical_name == "build");
    assert!(build.is_some(), "the parsed symbol must be present");

    // Evaluation ran over the enriched structure: the unmet import's socket hole
    // reddened its file's integration axis. This is only possible if enrich then
    // evaluate both ran in the pipeline.
    let file = snapshot
        .entities
        .iter()
        .find(|e| e.relative_path.as_deref() == Some("src/lib.rs"))
        .expect("the file entity");
    let integration = &file.green.as_ref().expect("evaluated").axes[&GreenAxis::Integration];
    assert_eq!(
        integration.code,
        GreenCode::Red,
        "the socket hole must darken integration, got {:?} ({:?})",
        integration.code,
        integration.reasons
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_pipeline_reconciles_identity_against_the_previous_snapshot() {
    let root = fixture("reconcile", "pub fn build() {}\n");
    let first = index_project(&root, None).expect("first index");
    let first_build = first
        .entities
        .iter()
        .find(|e| e.canonical_name == "build")
        .expect("build")
        .id;

    // Re-index with an edit above the symbol (it moves), passing the first as the
    // predecessor — its stable id must carry over.
    fs::write(
        root.join("src/lib.rs"),
        "// a new line above\n\npub fn build() {}\n",
    )
    .unwrap();
    let second = index_project(&root, Some(&first)).expect("second index");
    let second_build = second
        .entities
        .iter()
        .find(|e| e.canonical_name == "build")
        .expect("build")
        .id;

    assert_eq!(
        first_build, second_build,
        "identity reconciliation must carry the id across the move"
    );

    let _ = fs::remove_dir_all(&root);
}

/// The cache must not change the answer, only the work.
///
/// A cached index is what the resident daemon runs; a full index is what the
/// one-shot CLI runs. If they can diverge, the daemon's Atlas is a different
/// artefact from the scanned one — exactly the split `index_project` exists to
/// close. So this compares the two snapshots field by field over the parsed
/// structure: entities, their sockets and every socket's resolution, and edges.
///
/// Snapshot ids are excluded because they are content-addressed over the scan
/// and identical by construction; what is compared is everything enrichment and
/// evaluation produce.
#[test]
fn an_incrementally_enriched_snapshot_matches_a_full_one() {
    use creature_context_parsers::incremental::ParseCache;
    use creature_context_parsers::index::index_project_cached;

    let root = fixture(
        "equivalence",
        "use crate::nowhere::Absent;\npub fn build() -> Absent { todo!() }\npub struct Widget;\n",
    );

    // Full: no cache, every file parsed.
    let full = index_project(&root, None).expect("full index");

    // Warm a cache, then index again from it — the second pass parses nothing.
    let mut cache = ParseCache::new();
    index_project_cached(&root, None, &mut cache).expect("warming index");
    let (_, misses_after_warm) = cache.stats();
    let cached = index_project_cached(&root, None, &mut cache).expect("cached index");

    // The cache actually served this pass: no new misses since warming.
    let (hits, misses) = cache.stats();
    assert!(hits > 0, "the cache must have served at least one file");
    assert_eq!(
        misses, misses_after_warm,
        "an unchanged tree must not be parsed again"
    );

    // Same entities, same order, same names.
    let names = |s: &creature_context_types::AtlasSnapshot| {
        s.entities
            .iter()
            .map(|e| (e.canonical_name.clone(), e.scale, e.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&full), names(&cached), "entities must match");

    // Same sockets and — the load-bearing part — the same resolutions.
    let sockets = |s: &creature_context_types::AtlasSnapshot| {
        let mut out = s
            .entities
            .iter()
            .flat_map(|e| {
                e.sockets.iter().map(move |sock| {
                    (
                        e.canonical_name.clone(),
                        sock.direction,
                        sock.shape.qualified_name.clone(),
                        format!("{:?}", sock.resolution),
                    )
                })
            })
            .collect::<Vec<_>>();
        out.sort();
        out
    };
    assert_eq!(
        sockets(&full),
        sockets(&cached),
        "socket resolutions must match — the humility guard depends on macro \
         names surviving the cache"
    );

    // Same Green, evaluated over the same structure.
    let green = |s: &creature_context_types::AtlasSnapshot| {
        let mut out = s
            .entities
            .iter()
            .map(|e| {
                (
                    e.canonical_name.clone(),
                    e.green.as_ref().map(|g| format!("{:?}", g.overall)),
                )
            })
            .collect::<Vec<_>>();
        out.sort();
        out
    };
    assert_eq!(green(&full), green(&cached), "Green must match");

    assert_eq!(
        full.edges.len(),
        cached.edges.len(),
        "edge count must match"
    );

    let _ = fs::remove_dir_all(&root);
}

/// Changing a file must invalidate its parse. The fingerprint is the key, so a
/// content change is a different key — this proves the daemon cannot serve a
/// stale parse for an edited file.
#[test]
fn a_changed_file_is_reparsed_and_its_symbols_update() {
    use creature_context_parsers::incremental::ParseCache;
    use creature_context_parsers::index::index_project_cached;

    let root = fixture("invalidate", "pub fn before() {}\n");
    let mut cache = ParseCache::new();

    let first = index_project_cached(&root, None, &mut cache).expect("first index");
    assert!(
        first.entities.iter().any(|e| e.canonical_name == "before"),
        "the original symbol must be present"
    );

    fs::write(root.join("src/lib.rs"), "pub fn after() {}\n").unwrap();
    let second = index_project_cached(&root, Some(&first), &mut cache).expect("second index");

    assert!(
        second.entities.iter().any(|e| e.canonical_name == "after"),
        "the edited file's new symbol must appear"
    );
    assert!(
        !second.entities.iter().any(|e| e.canonical_name == "before"),
        "the stale symbol must be gone — a cached parse must not survive an edit"
    );

    let _ = fs::remove_dir_all(&root);
}
