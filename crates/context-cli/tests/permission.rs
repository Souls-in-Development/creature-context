//! Milestone 3 Task 3: the permission CLI reads and writes the real ledger.
//!
//! Replaces the exit-code-only test that asserted success on no-op handlers.
//! These assert behaviour: a recorded rule appears in `list`, and the
//! fabricated `["permission1","permission2"]` is gone.

use std::path::PathBuf;
use std::process::Command;
use std::{fs, thread};

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "creature-context-permission-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(path.join("PURPOSE.md"), "# CLI Fixture\n").unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    path
}

fn scan(binary: &str, root: &std::path::Path) {
    let out = Command::new(binary)
        .args(["scan", root.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run(binary: &str, args: &[&str]) -> std::process::Output {
    Command::new(binary).args(args).output().unwrap()
}

#[test]
fn a_recorded_rule_appears_in_the_list() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("recorded");
    let p = root.to_str().unwrap();
    scan(binary, &root);

    let allow = run(
        binary,
        &[
            "permission",
            "allow",
            p,
            "--subject",
            "agent:x",
            "--action",
            "read",
            "--resource",
            "src/**",
            "--scope",
            "ongoing",
        ],
    );
    assert!(
        allow.status.success(),
        "{}",
        String::from_utf8_lossy(&allow.stderr)
    );

    let list = run(binary, &["permission", "list", p, "--format", "json"]);
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    let out = String::from_utf8_lossy(&list.stdout);
    let lower = out.to_lowercase();

    assert!(
        out.contains("src/**"),
        "list must show the recorded rule: {out}"
    );
    assert!(lower.contains("read"), "list must show the action: {out}");
    assert!(
        !out.contains("permission1"),
        "the fabricated placeholder list must be gone: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn allow_and_deny_both_persist() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("both");
    let p = root.to_str().unwrap();
    scan(binary, &root);

    assert!(
        run(
            binary,
            &[
                "permission",
                "allow",
                p,
                "--subject",
                "agent:x",
                "--action",
                "read",
                "--resource",
                "**",
                "--scope",
                "ongoing",
            ],
        )
        .status
        .success()
    );
    thread::sleep(std::time::Duration::from_millis(2));
    assert!(
        run(
            binary,
            &[
                "permission",
                "deny",
                p,
                "--subject",
                "agent:x",
                "--action",
                "read",
                "--resource",
                "secrets/**",
                "--scope",
                "ongoing",
            ],
        )
        .status
        .success()
    );

    let list = run(binary, &["permission", "list", p, "--format", "json"]);
    let out = String::from_utf8_lossy(&list.stdout);
    let lower = out.to_lowercase();
    assert!(
        out.contains("**") && out.contains("secrets/**"),
        "both rules persist: {out}"
    );
    assert!(
        lower.contains("allow") && lower.contains("deny"),
        "both decisions recorded: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_unknown_action_is_rejected_not_recorded() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("unknown-action");
    let p = root.to_str().unwrap();
    scan(binary, &root);

    let bad = run(
        binary,
        &[
            "permission",
            "allow",
            p,
            "--subject",
            "agent:x",
            "--action",
            "teleport",
            "--resource",
            "src/**",
            "--scope",
            "ongoing",
        ],
    );
    assert!(
        !bad.status.success(),
        "an unrecognised action must be rejected, not recorded as some default"
    );

    let list = run(binary, &["permission", "list", p, "--format", "json"]);
    let out = String::from_utf8_lossy(&list.stdout);
    assert!(
        !out.contains("teleport"),
        "the rejected rule must not be in the ledger: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}
