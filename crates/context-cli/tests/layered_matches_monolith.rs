//! The layered scan and the monolith scan must produce the same Atlas — the same
//! entity ids and the same edge ids — given the same project identity. Only the
//! memory used to reach it differs. This is the determinism gate for the streaming
//! (`scan-layered`) path.
//!
//! Both runs must share one project identity: every id derives from the project's
//! `project_id`, so scanning two independently-initialised copies would produce two
//! disjoint id sets that are not comparable. So the fixture is `init`-ed once and
//! then copied, and both copies scan the *same* identity.

use std::path::Path;
use std::process::Command;

fn sorted_ids(db: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(db).expect("open store");
    let mut stmt = conn
        .prepare(&format!("SELECT id FROM {table} ORDER BY id"))
        .expect("prepare");
    let ids = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("row"))
        .collect();
    ids
}

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

#[test]
fn layered_and_monolith_agree_on_entity_and_edge_ids() {
    let bin = env!("CARGO_BIN_EXE_creature-context");
    let stamp = std::process::id();
    let base = std::env::temp_dir().join(format!("cc-parity-{stamp}"));
    let mono = std::env::temp_dir().join(format!("cc-parity-{stamp}-m"));
    let lay = std::env::temp_dir().join(format!("cc-parity-{stamp}-l"));
    for dir in [&base, &mono, &lay] {
        let _ = std::fs::remove_dir_all(dir);
    }

    // Cross-directory imports so the import-edge stitch is exercised, and a nested
    // directory so more than one Planet level appears.
    write(&base, "PURPOSE.md", "# F\n\n## Goals\n- x\n");
    write(&base, "a/one.rs", "use two;\npub fn one() {}\npub struct A;\n");
    write(&base, "b/two.rs", "use one;\npub fn two() {}\n");
    write(&base, "b/c/three.rs", "use one;\npub fn three() {}\n");

    // Fix the identity once, then copy — both scans share it.
    assert!(
        Command::new(bin)
            .args(["init", base.to_str().unwrap(), "--format", "json"])
            .output()
            .unwrap()
            .status
            .success(),
        "init failed"
    );
    copy_dir(&base, &mono);
    copy_dir(&base, &lay);

    assert!(
        Command::new(bin)
            .args(["scan", mono.to_str().unwrap(), "--format", "json"])
            .output()
            .unwrap()
            .status
            .success(),
        "monolith scan failed"
    );
    assert!(
        Command::new(bin)
            .args(["scan-layered", lay.to_str().unwrap()])
            .output()
            .unwrap()
            .status
            .success(),
        "layered scan failed"
    );

    let mono_db = mono.join(".creature/atlas.db");
    let lay_db = lay.join(".creature/atlas.db");
    assert_eq!(
        sorted_ids(&mono_db, "atlas_entities"),
        sorted_ids(&lay_db, "atlas_entities"),
        "entity ids must be identical between the monolith and layered scans"
    );
    assert_eq!(
        sorted_ids(&mono_db, "atlas_edges"),
        sorted_ids(&lay_db, "atlas_edges"),
        "edge ids must be identical between the monolith and layered scans"
    );

    for dir in [&base, &mono, &lay] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
