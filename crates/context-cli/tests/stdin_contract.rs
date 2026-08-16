use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

fn temp_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!("creature-context-stdin-{}", std::process::id()));
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
fn test_stdin_context_request() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root();

    // First scan to generate an index
    let scan = Command::new(binary)
        .args(["scan", root.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );

    let request_json = serde_json::json!({
        "task": "understand CLI Fixture",
        "scale": "galaxy"
    });

    let mut cmd = Command::new(binary)
        .args([
            "context",
            root.to_str().unwrap(),
            "--request",
            "-",
            "--format",
            "idx",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = cmd.stdin.take().unwrap();
    stdin
        .write_all(serde_json::to_string(&request_json).unwrap().as_bytes())
        .unwrap();
    drop(stdin);

    let output = cmd.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "Command failed with: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "Stderr should be empty");
    assert!(
        stdout.starts_with("@creature-context"),
        "Should start with IDX header"
    );

    fs::remove_dir_all(&root).unwrap();
}
