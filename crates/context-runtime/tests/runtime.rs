//! Milestone 2 Task 4: the reconciliation coordinator.
//!
//! The coordinator is the testable core of the resident service — pure, no I/O.
//! The watcher and service are exercised end to end by `run` in Task 6; here we
//! prove the two decisions that matter: a burst collapses to one reconciliation,
//! and a stream-failure event forces a full scan.

use creature_context_runtime::coordinator::{Coordinator, CoordinatorConfig};
use creature_context_runtime::events::{RuntimeEvent, RuntimeEventKind};

fn file_event(path: &str) -> RuntimeEvent {
    RuntimeEvent::new(RuntimeEventKind::FileModified, "2026-08-05T00:00:00Z").with_path(path)
}

#[test]
fn one_atomic_save_produces_one_reconciliation() {
    let mut coordinator = Coordinator::new(CoordinatorConfig { debounce_ms: 250 });

    // An editor save emits many events; they must collapse into one unit.
    for _ in 0..100 {
        coordinator.enqueue(file_event("src/main.rs"));
    }

    assert!(coordinator.settle(), "a non-empty batch must reconcile");
    assert_eq!(
        coordinator.reconciliations(),
        1,
        "one hundred events for one save must produce one reconciliation, not one hundred"
    );
}

#[test]
fn an_empty_batch_does_not_reconcile() {
    let mut coordinator = Coordinator::default();
    assert!(!coordinator.settle(), "nothing pending means nothing to do");
    assert_eq!(coordinator.reconciliations(), 0);
}

#[test]
fn an_overflow_event_forces_a_full_scan() {
    let mut coordinator = Coordinator::default();
    coordinator.enqueue(RuntimeEvent::new(
        RuntimeEventKind::Overflow,
        "2026-08-05T00:00:00Z",
    ));

    assert!(coordinator.settle());
    assert_eq!(coordinator.reconciliations(), 1);
    assert!(
        coordinator.last_scan_was_bounded(),
        "a dropped-event overflow means the stream is untrustworthy and a full rescan is required"
    );
}

#[test]
fn a_plain_file_change_does_not_force_a_full_scan() {
    let mut coordinator = Coordinator::default();
    coordinator.enqueue(file_event("src/main.rs"));

    assert!(coordinator.settle());
    assert!(
        !coordinator.last_scan_was_bounded(),
        "a single file change is a targeted update, not a full rescan"
    );
}

#[test]
fn an_applied_event_is_not_reprocessed_after_restart() {
    // The service seeds the coordinator from the journal's applied set on
    // startup. An event already completed before a crash must not run again.
    let mut coordinator = Coordinator::default();
    let event = file_event("src/main.rs");
    let id = event.id;

    coordinator.mark_applied(&id);
    coordinator.enqueue(event);

    assert!(
        !coordinator.settle(),
        "the only pending event was already applied, so there is nothing to reconcile"
    );
    assert_eq!(coordinator.reconciliations(), 0);
    assert!(!coordinator.should_apply(&id));
}

#[test]
fn a_mixed_batch_reconciles_only_unapplied_events() {
    let mut coordinator = Coordinator::default();
    let done = file_event("src/done.rs");
    let fresh = file_event("src/fresh.rs");

    coordinator.mark_applied(&done.id);
    coordinator.enqueue(done);
    coordinator.enqueue(fresh);

    assert!(
        coordinator.settle(),
        "one unapplied event remains, so reconciliation must run"
    );
    assert_eq!(coordinator.reconciliations(), 1);
}
