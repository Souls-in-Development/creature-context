//! Reconciliation coordinator.
//!
//! Pure by design: it collects normalised events, collapses a burst into one
//! unit of work, and decides whether that work is a bounded targeted update or
//! a full rescan. It performs no I/O — the service owns the watcher, the
//! journal and the repository. That separation is what makes the debounce and
//! the scan decision testable without a filesystem.

use crate::events::RuntimeEvent;
use std::collections::BTreeSet;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct CoordinatorConfig {
    /// How long to let a burst of events settle before reconciling. An editor
    /// saving a file emits several events in quick succession; they should
    /// produce one reconciliation, not several.
    pub debounce_ms: u64,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self { debounce_ms: 250 }
    }
}

pub struct Coordinator {
    config: CoordinatorConfig,
    pending: Vec<RuntimeEvent>,
    applied: BTreeSet<Uuid>,
    reconciliations: usize,
    last_scan_bounded: bool,
}

impl Coordinator {
    pub fn new(config: CoordinatorConfig) -> Self {
        Self {
            config,
            pending: Vec::new(),
            applied: BTreeSet::new(),
            reconciliations: 0,
            last_scan_bounded: false,
        }
    }

    pub fn enqueue(&mut self, event: RuntimeEvent) {
        self.pending.push(event);
    }

    /// Seed the applied set from the journal at startup, so a restart does not
    /// reprocess events completed before the crash.
    pub fn mark_applied(&mut self, id: &Uuid) {
        self.applied.insert(*id);
    }

    pub fn should_apply(&self, id: &Uuid) -> bool {
        !self.applied.contains(id)
    }

    pub fn pending_events(&self) -> &[RuntimeEvent] {
        &self.pending
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    pub fn reconciliations(&self) -> usize {
        self.reconciliations
    }

    /// Whether the last settled batch demanded a full rescan rather than a
    /// targeted update. A bounded full scan is what a stream failure requires,
    /// since the event stream can no longer be trusted to be complete.
    pub fn last_scan_was_bounded(&self) -> bool {
        self.last_scan_bounded
    }

    /// Called after the debounce window closes. Returns whether reconciliation
    /// should run, having dropped any already-applied events first.
    ///
    /// A burst of a hundred saves of one file collapses to a single
    /// reconciliation; that is the debounce doing its job.
    pub fn settle(&mut self) -> bool {
        // A stream-failure event (overflow, root loss, permission change,
        // explicit rescan) forces a full scan. See RuntimeEvent 7.1.
        let requires_full = self
            .pending
            .iter()
            .any(RuntimeEvent::requires_full_reconciliation);

        let applied = &self.applied;
        self.pending.retain(|event| !applied.contains(&event.id));
        if self.pending.is_empty() {
            return false;
        }

        self.last_scan_bounded = requires_full;
        self.reconciliations += 1;
        true
    }

    pub fn debounce_duration(&self) -> Duration {
        Duration::from_millis(self.config.debounce_ms)
    }
}

impl Default for Coordinator {
    fn default() -> Self {
        Self::new(CoordinatorConfig::default())
    }
}
