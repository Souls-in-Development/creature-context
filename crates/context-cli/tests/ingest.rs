//! Milestone 2 Task 5: `creature-context ingest` appends a typed activity
//! event, with its payload, to the journal.

use std::path::PathBuf;
use std::process::Command;
use std::{fs, thread};

/// Per-test temp dir. Keyed on process id *and* test name so parallel test
/// binaries and parallel tests within a binary do not collide on the
/// filesystem — the flake class the earlier CLI tests suffer from.
fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "creature-context-ingest-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src")).unwrap();
    fs::write(path.join("PURPOSE.md"), "# CLI Fixture\n").unwrap();
    fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    path
}

fn scan(binary: &str, root: &std::path::Path) {
    let output = Command::new(binary)
        .args(["scan", root.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ingest_appends_a_typed_event_carrying_its_payload() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("payload");
    scan(binary, &root);

    let ingest = Command::new(binary)
        .args([
            "ingest",
            root.to_str().unwrap(),
            "--kind",
            "git",
            "--message",
            "abc123",
        ])
        .output()
        .unwrap();
    assert!(
        ingest.status.success(),
        "ingest failed: {}",
        String::from_utf8_lossy(&ingest.stderr)
    );

    let journal = fs::read_to_string(root.join(".creature/journal.jsonl")).unwrap();
    // The event is external activity — a client handing in evidence — and the
    // payload must survive verbatim so it can later feed an Atlas update.
    assert!(
        journal.contains("external_activity"),
        "journal must record the typed event kind:\n{journal}"
    );
    assert!(journal.contains("git"), "payload source must be recorded");
    assert!(
        journal.contains("abc123"),
        "payload message must be recorded verbatim"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ingest_requires_kind_and_message() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("required-args");
    scan(binary, &root);

    // Missing --message: clap must reject rather than ingest a half-specified
    // event.
    let output = Command::new(binary)
        .args(["ingest", root.to_str().unwrap(), "--kind", "git"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "ingest without --message must fail rather than record an incomplete event"
    );

    // Nothing should have been appended for the rejected invocation.
    let journal = fs::read_to_string(root.join(".creature/journal.jsonl")).unwrap_or_default();
    assert!(
        !journal.contains("external_activity"),
        "a rejected ingest must not write an event"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ingested_events_accumulate_and_replay() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let root = temp_root("accumulate");
    scan(binary, &root);

    for message in ["first", "second", "third"] {
        let out = Command::new(binary)
            .args([
                "ingest",
                root.to_str().unwrap(),
                "--kind",
                "test",
                "--message",
                message,
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        // Distinct v7 ids require time to advance between calls.
        thread::sleep(std::time::Duration::from_millis(2));
    }

    let journal = fs::read_to_string(root.join(".creature/journal.jsonl")).unwrap();
    let count = journal
        .lines()
        .filter(|l| l.contains("external_activity"))
        .count();
    assert_eq!(count, 3, "each ingest appends one event; none overwrite");

    let _ = fs::remove_dir_all(&root);
}
