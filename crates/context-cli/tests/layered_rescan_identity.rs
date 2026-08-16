//! A rescan must keep stable ids: a declaration whose line moved is the same
//! entity and keeps its id (spec §3, §6). The monolith does this via
//! `reconcile_identity`; the layered scan must reconcile per directory against the
//! previous snapshot and reach the same ids. This guards both properties: a moved
//! symbol's id is stable across two layered scans, and the layered rescan's id set
//! matches the monolith's rescan given the same shared project identity.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

fn sorted_ids(db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).expect("open store");
    let mut stmt = conn
        .prepare("SELECT id FROM atlas_entities ORDER BY id")
        .expect("prepare");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect()
}

fn id_of_symbol(db: &Path, name: &str) -> String {
    let conn = rusqlite::Connection::open(db).expect("open store");
    conn.query_row(
        "SELECT id FROM atlas_entities WHERE canonical_name = ?1 AND scale = 'moon'",
        [name],
        |row| row.get::<_, String>(0),
    )
    .unwrap_or_else(|e| panic!("symbol {name} not found: {e}"))
}

fn scan(bin: &str, subcommand: &str, root: &Path) {
    let mut args = vec![subcommand, root.to_str().unwrap()];
    if subcommand == "scan" {
        args.push("--format");
        args.push("json");
    }
    assert!(
        Command::new(bin).args(&args).output().unwrap().status.success(),
        "{subcommand} failed"
    );
}

#[test]
fn layered_rescan_preserves_ids_and_matches_the_monolith() {
    let bin = env!("CARGO_BIN_EXE_creature-context");
    let stamp = std::process::id();
    let base = std::env::temp_dir().join(format!("cc-rescan-{stamp}"));
    let mono = std::env::temp_dir().join(format!("cc-rescan-{stamp}-m"));
    let lay = std::env::temp_dir().join(format!("cc-rescan-{stamp}-l"));
    for dir in [&base, &mono, &lay] {
        let _ = std::fs::remove_dir_all(dir);
    }

    write(&base, "PURPOSE.md", "# F\n\n## Goals\n- x\n");
    write(&base, "a/one.rs", "pub fn stableone() {}\n");
    write(&base, "b/two.rs", "pub fn stabletwo() {}\n");

    assert!(
        Command::new(bin)
            .args(["init", base.to_str().unwrap(), "--format", "json"])
            .output().unwrap().status.success(),
        "init failed"
    );
    copy_dir(&base, &mono);
    copy_dir(&base, &lay);

    // First pass.
    scan(bin, "scan", &mono);
    scan(bin, "scan-layered", &lay);
    let mono_db = mono.join(".creature/atlas.db");
    let lay_db = lay.join(".creature/atlas.db");
    let mono_id_first = id_of_symbol(&mono_db, "stableone");
    let lay_id_first = id_of_symbol(&lay_db, "stableone");

    // Move the symbol down a line in both — same declaration, new line number.
    write(&mono, "a/one.rs", "// shifted down\npub fn stableone() {}\n");
    write(&lay, "a/one.rs", "// shifted down\npub fn stableone() {}\n");

    // Second pass — reconciliation must carry the id forward.
    scan(bin, "scan", &mono);
    scan(bin, "scan-layered", &lay);
    let mono_id_second = id_of_symbol(&mono_db, "stableone");
    let lay_id_second = id_of_symbol(&lay_db, "stableone");

    assert_eq!(
        mono_id_first, mono_id_second,
        "monolith must keep the moved symbol's id stable across a rescan"
    );
    assert_eq!(
        lay_id_first, lay_id_second,
        "layered scan must keep the moved symbol's id stable across a rescan (reconciliation)"
    );

    let mono_ids: BTreeSet<String> = sorted_ids(&mono_db).into_iter().collect();
    let lay_ids: BTreeSet<String> = sorted_ids(&lay_db).into_iter().collect();
    assert_eq!(
        mono_ids, lay_ids,
        "after a rescan, layered and monolith id sets must match"
    );

    for dir in [&base, &mono, &lay] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
