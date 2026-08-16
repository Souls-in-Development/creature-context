//! Milestone 2 Task 3: typed events, durable journalling, and replay that
//! resumes rather than repeats.
//!
//! The plan's stated goal is that "replay must not duplicate applied events
//! after a restart". Reading the journal twice does not test that — it tests
//! that reading is non-destructive. The applied-marker path is tested here
//! directly, because it is the part that makes a restart safe.

use creature_context_core::project::ProjectPaths;
use creature_context_runtime::events::{RuntimeEvent, RuntimeEventKind};
use creature_context_store::{JournalFinding, JsonlJournal};
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "creature-context-journal-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join(".creature")).expect("create dirs");
    path
}

fn event(kind: RuntimeEventKind, path: &str) -> RuntimeEvent {
    RuntimeEvent::new(kind, "2026-08-05T00:00:00Z").with_path(path)
}

#[test]
fn appended_events_are_readable_and_reading_does_not_consume() {
    let root = temp_root("append");
    let paths = ProjectPaths::new(&root);
    let recorded = event(RuntimeEventKind::FileModified, "src/main.rs");

    {
        let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("open");
        journal.append(&recorded).expect("append");
    }

    let journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("reopen");
    assert_eq!(journal.read_all().expect("read").len(), 1);
    assert_eq!(journal.read_all().expect("read again").len(), 1);
    assert_eq!(journal.read_all().expect("read")[0].id, recorded.id);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_restart_does_not_reprocess_applied_events() {
    let root = temp_root("applied");
    let paths = ProjectPaths::new(&root);

    let first = event(RuntimeEventKind::FileModified, "src/a.rs");
    let second = event(RuntimeEventKind::FileAdded, "src/b.rs");

    {
        let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("open");
        journal.append(&first).expect("append");
        journal.append(&second).expect("append");
        // The runtime applied the first and died before the second.
        journal.mark_applied(first.id).expect("mark");
    }

    // Restart.
    let journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("reopen");
    let pending = journal.pending().expect("pending");

    assert_eq!(
        pending.len(),
        1,
        "an applied event must not be handed back after a restart"
    );
    assert_eq!(pending[0].id, second.id, "the unapplied event must survive");
    assert_eq!(
        journal.read_all().expect("read").len(),
        2,
        "the journal is append-only; applying does not erase history"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn applying_every_event_leaves_nothing_pending() {
    let root = temp_root("drained");
    let paths = ProjectPaths::new(&root);
    let only = event(RuntimeEventKind::FileModified, "src/a.rs");

    let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("open");
    journal.append(&only).expect("append");
    journal.mark_applied(only.id).expect("mark");

    assert!(journal.pending().expect("pending").is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_truncated_tail_is_reported_not_fabricated() {
    let root = temp_root("truncated");
    let paths = ProjectPaths::new(&root);

    {
        let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("open");
        journal
            .append(&event(RuntimeEventKind::FileModified, "src/main.rs"))
            .expect("append");
    }

    // Simulate a crash part-way through writing the line.
    let bytes = fs::read(&paths.journal).expect("read");
    fs::write(&paths.journal, &bytes[..bytes.len() - 10]).expect("truncate");

    let journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("reopen");
    let (events, findings) = journal.read_all_with_findings().expect("read");

    assert!(
        events.is_empty(),
        "a partially written line is the absence of a record, not a record"
    );
    assert!(
        matches!(findings.as_slice(), [JournalFinding::TruncatedTail { .. }]),
        "the truncation must be reported rather than silently dropped, got {findings:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn corruption_before_the_final_line_is_an_error_not_a_truncation() {
    let root = temp_root("corrupt");
    let paths = ProjectPaths::new(&root);

    {
        let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("open");
        journal
            .append(&event(RuntimeEventKind::FileModified, "src/a.rs"))
            .expect("append");
        journal
            .append(&event(RuntimeEventKind::FileAdded, "src/b.rs"))
            .expect("append");
    }

    let contents = fs::read_to_string(&paths.journal).expect("read");
    let mut lines: Vec<&str> = contents.lines().collect();
    lines[0] = "{ not valid json";
    fs::write(&paths.journal, format!("{}\n", lines.join("\n"))).expect("write");

    let journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("reopen");

    assert!(
        journal.read_all().is_err(),
        "a crash cannot damage a line that was already followed by another, so this is \
         corruption and must not be quietly skipped"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn ingested_payload_survives_the_round_trip() {
    let root = temp_root("payload");
    let paths = ProjectPaths::new(&root);

    let ingested = RuntimeEvent::new(RuntimeEventKind::ExternalActivity, "2026-08-05T00:00:00Z")
        .with_payload(serde_json::json!({
            "producer": "cargo test",
            "outcome": "pass",
            "tests": 79
        }));

    {
        let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("open");
        journal.append(&ingested).expect("append");
    }

    let journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal).expect("reopen");
    let replayed = &journal.read_all().expect("read")[0];
    let payload = replayed.payload.as_ref().expect(
        "payload must survive — an event recording only that something happened \
                 cannot be replayed into an Atlas update",
    );

    assert_eq!(payload["outcome"], "pass");
    assert_eq!(payload["tests"], 79);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn stream_failure_kinds_demand_full_reconciliation() {
    // Specification 7.1: watcher events are hints; overflow and root loss mean
    // the stream cannot be trusted to have been complete.
    for kind in [
        RuntimeEventKind::Overflow,
        RuntimeEventKind::RootUnavailable,
        RuntimeEventKind::PermissionChanged,
        RuntimeEventKind::RescanRequired,
    ] {
        assert!(
            RuntimeEvent::new(kind, "2026-08-05T00:00:00Z").requires_full_reconciliation(),
            "{kind:?} must force reconciliation rather than being handled as one path"
        );
    }
    for kind in [
        RuntimeEventKind::FileAdded,
        RuntimeEventKind::FileModified,
        RuntimeEventKind::FileRemoved,
    ] {
        assert!(
            !RuntimeEvent::new(kind, "2026-08-05T00:00:00Z").requires_full_reconciliation(),
            "{kind:?} describes one path and must not trigger a full rescan"
        );
    }
}
