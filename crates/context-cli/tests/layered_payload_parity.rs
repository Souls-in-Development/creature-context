//! Stronger determinism gate than `layered_matches_monolith`: the layered scan and
//! the monolith must agree not just on entity/edge IDs but on the socket
//! RESOLUTIONS and the Green OVERALL codes carried in each entity's payload. The
//! fixture has cross-directory imports (so a `requires` in one folder is satisfied
//! by a `provides` in another) and a name provided by two different folders (so the
//! Ambiguous candidate list's ORDER is exercised).

use std::collections::BTreeMap;
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

/// id -> (sorted requires-socket resolutions as JSON strings, green.overall string).
fn payload_facts(db: &Path) -> BTreeMap<String, (Vec<String>, String)> {
    let conn = rusqlite::Connection::open(db).expect("open store");
    let mut stmt = conn
        .prepare("SELECT id, payload_json FROM atlas_entities")
        .expect("prepare");
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("query");
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, payload) = row.expect("row");
        let value: serde_json::Value = serde_json::from_str(&payload).expect("payload json");
        // requires-socket resolutions, tagged by the wanted name and sorted, so the
        // comparison is order-independent across the socket vec while still comparing
        // each resolution's full JSON (including the Ambiguous candidate order).
        let mut resolutions: Vec<String> = value
            .get("sockets")
            .and_then(|s| s.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|sock| {
                        sock.get("direction").and_then(|d| d.as_str()) == Some("requires")
                    })
                    .map(|sock| {
                        let name = sock
                            .get("shape")
                            .and_then(|sh| sh.get("qualified_name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let res =
                            sock.get("resolution").cloned().unwrap_or(serde_json::Value::Null);
                        format!("{name}={}", serde_json::to_string(&res).unwrap())
                    })
                    .collect()
            })
            .unwrap_or_default();
        resolutions.sort();
        let overall = value
            .get("green")
            .and_then(|g| g.get("overall"))
            .and_then(|o| o.as_str())
            .unwrap_or("null")
            .to_string();
        out.insert(id, (resolutions, overall));
    }
    out
}

#[test]
fn layered_and_monolith_agree_on_socket_resolutions_and_green() {
    let bin = env!("CARGO_BIN_EXE_creature-context");
    let stamp = std::process::id();
    let base = std::env::temp_dir().join(format!("cc-payload-{stamp}"));
    let mono = std::env::temp_dir().join(format!("cc-payload-{stamp}-m"));
    let lay = std::env::temp_dir().join(format!("cc-payload-{stamp}-l"));
    for dir in [&base, &mono, &lay] {
        let _ = std::fs::remove_dir_all(dir);
    }

    write(&base, "PURPOSE.md", "# F\n\n## Goals\n- x\n");
    // Cross-directory Fits: b and b/c both `use one` (provided in a); a `use two`
    // (provided in b). Each is a NoMatch within its own folder, a Fit globally.
    write(&base, "a/one.rs", "use two;\npub fn one() {}\npub struct A;\n");
    write(&base, "b/two.rs", "use one;\npub fn two() {}\n");
    write(&base, "b/c/three.rs", "use one;\npub fn three() {}\n");
    // Ambiguous across folders: `dup` provided by both d and e, required by f.
    // The candidate order must be (relative_path, parse_index) = d before e.
    write(&base, "d/dupe1.rs", "pub fn dup() {}\n");
    write(&base, "e/dupe2.rs", "pub fn dup() {}\n");
    write(&base, "f/needsdup.rs", "use dup;\npub fn f() {}\n");

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

    let mono_facts = payload_facts(&mono.join(".creature/atlas.db"));
    let lay_facts = payload_facts(&lay.join(".creature/atlas.db"));

    assert_eq!(
        mono_facts, lay_facts,
        "socket resolutions and green.overall must be identical between monolith and layered scans"
    );

    for dir in [&base, &mono, &lay] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
