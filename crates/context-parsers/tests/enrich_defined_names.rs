//! `enrich_snapshot_parallel` must be able to hand back the `defined_names` set it
//! builds — symbol names AND macro-expanded names — so the layered scan's global
//! humility pass sees names that are defined but invisible to the provides index.

use creature_context_core::scan::scan_project_configured;
use creature_context_parsers::enrich::enrich_snapshot_parallel;
use std::collections::HashSet;

fn write(root: &std::path::Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[test]
fn parallel_enrich_reports_defined_names() {
    let stamp = std::process::id();
    let root = std::env::temp_dir().join(format!("cc-defnames-{stamp}"));
    let _ = std::fs::remove_dir_all(&root);
    write(&root, "PURPOSE.md", "# F\n\n## Goals\n- x\n");
    write(&root, "src/lib.rs", "pub fn alpha() {}\npub struct Beta;\n");

    let mut snapshot = scan_project_configured(&root).expect("scan");
    let mut names: HashSet<String> = HashSet::new();
    enrich_snapshot_parallel(&root, &mut snapshot, None, Some(&mut names));

    assert!(names.contains("alpha"), "symbol names must be reported: {names:?}");
    assert!(names.contains("Beta"), "type names must be reported: {names:?}");

    let _ = std::fs::remove_dir_all(&root);
}
