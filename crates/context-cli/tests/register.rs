use std::{fs, path::PathBuf, process::Command};

fn temp_root() -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("creature-context-register-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn test_register_command() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root();

    let register = Command::new(binary)
        .args([
            "register",
            root.to_str().unwrap(),
            "project-c",
            "/some/path",
        ])
        .output()
        .unwrap();
    assert!(
        register.status.success(),
        "{}",
        String::from_utf8_lossy(&register.stderr)
    );

    fs::remove_dir_all(&root).unwrap();
}
