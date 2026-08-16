//! `map` writes a deterministic ATLAS.png from the current snapshot, plus an
//! append-only per-snapshot frame. Same snapshot → byte-identical bytes.

use std::path::Path;
use std::process::Command;

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[test]
fn map_is_deterministic_and_writes_a_frame() {
    let bin = env!("CARGO_BIN_EXE_creature-context");
    let stamp = std::process::id();
    let root = std::env::temp_dir().join(format!("cc-map-{stamp}"));
    let _ = std::fs::remove_dir_all(&root);

    write(&root, "PURPOSE.md", "# F\n\n## Goals\n- x\n");
    write(&root, "a/one.rs", "pub fn one() {}\npub struct A;\n");
    write(&root, "b/two.rs", "pub fn two() {}\n");

    for args in [
        vec!["init", root.to_str().unwrap(), "--format", "json"],
        vec!["scan", root.to_str().unwrap(), "--format", "json"],
    ] {
        assert!(
            Command::new(bin).args(&args).output().unwrap().status.success(),
            "{args:?} failed"
        );
    }

    let run_map = || {
        assert!(
            Command::new(bin)
                .args(["map", root.to_str().unwrap()])
                .output()
                .unwrap()
                .status
                .success(),
            "map failed"
        );
    };

    run_map();
    let png1 = std::fs::read(root.join("ATLAS.png")).unwrap();
    assert!(
        png1.len() > 8 && png1[0..8] == [137, 80, 78, 71, 13, 10, 26, 10],
        "valid PNG"
    );

    run_map();
    let png2 = std::fs::read(root.join("ATLAS.png")).unwrap();
    assert_eq!(png1, png2, "map must be deterministic: same snapshot -> same bytes");

    let frames_dir = root.join(".creature/atlas-frames");
    let frames: Vec<_> = std::fs::read_dir(&frames_dir)
        .expect("frames dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(frames.len(), 1, "exactly one frame after two runs (append-only)");
    let frame_bytes = std::fs::read(&frames[0]).unwrap();
    assert_eq!(frame_bytes, png1, "frame bytes equal ATLAS.png");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn map_galaxy_is_deterministic() {
    let bin = env!("CARGO_BIN_EXE_creature-context");
    let stamp = std::process::id();
    let root = std::env::temp_dir().join(format!("cc-mapgx-{stamp}"));
    let _ = std::fs::remove_dir_all(&root);

    write(&root, "PURPOSE.md", "# F\n\n## Goals\n- x\n");
    write(&root, "a/one.rs", "pub fn one() {}\npub struct A;\n");
    write(&root, "b/two.rs", "pub fn two() {}\n");
    write(&root, "b/c/three.rs", "pub fn three() {}\n");

    for args in [
        vec!["init", root.to_str().unwrap(), "--format", "json"],
        vec!["scan", root.to_str().unwrap(), "--format", "json"],
    ] {
        assert!(
            Command::new(bin).args(&args).output().unwrap().status.success(),
            "{args:?} failed"
        );
    }

    let run = || {
        assert!(
            Command::new(bin)
                .args(["map", root.to_str().unwrap(), "--galaxy"])
                .output()
                .unwrap()
                .status
                .success(),
            "map --galaxy failed"
        );
        std::fs::read(root.join("ATLAS.png")).unwrap()
    };

    let a = run();
    assert!(a.len() > 8 && a[0..8] == [137, 80, 78, 71, 13, 10, 26, 10], "valid PNG");
    let b = run();
    assert_eq!(a, b, "map --galaxy must be deterministic");

    let _ = std::fs::remove_dir_all(&root);
}
