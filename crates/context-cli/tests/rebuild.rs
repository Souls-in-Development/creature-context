use std::{fs, path::PathBuf, process::Command};

fn temp_root() -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("creature-context-rebuild-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(
        path.join("PURPOSE.md"),
        "# CLI Fixture\n\n## Goals\n- Exercise the CLI\n",
    )
    .unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    path
}

#[test]
fn test_rebuild_command() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root();

    let scan = Command::new(binary)
        .args(["scan", root.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );

    let rebuild = Command::new(binary)
        .args(["rebuild", root.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        rebuild.status.success(),
        "{}",
        String::from_utf8_lossy(&rebuild.stderr)
    );

    fs::remove_dir_all(&root).unwrap();
}
