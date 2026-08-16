//! Milestone 2 Task 5: `creature-context run` stays alive as a resident
//! service and exits cleanly when signalled.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{fs, thread};

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "creature-context-run-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(path.join("PURPOSE.md"), "# CLI Fixture\n").unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    path
}

#[test]
fn run_stays_alive_until_stopped() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("alive");

    let scan = Command::new(binary)
        .args(["scan", root.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        scan.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&scan.stderr)
    );

    let mut child = Command::new(binary)
        .args(["run", root.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // A resident service must not exit on its own; the stub returned immediately.
    thread::sleep(Duration::from_millis(600));
    let still_running = child.try_wait().unwrap().is_none();

    // Stop it before asserting, so a failure never leaves an orphan watcher.
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        still_running,
        "run must stay alive as a resident service, not return immediately"
    );

    let _ = fs::remove_dir_all(&root);
}
