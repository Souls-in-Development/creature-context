//! The resident service behind `creature-context run`.
//!
//! One continuous loop: drain the watcher, journal each event *before* it is
//! processed, debounce, reconcile, then mark events applied *after* the
//! snapshot commits. The ordering is deliberate — an event journalled before
//! processing survives a crash mid-reconciliation, and a marker written only
//! after the commit means a restart never treats unfinished work as done.
//!
//! Applied markers use the journal's own append-only `.applied` sidecar
//! (`mark_applied`/`applied_ids`), not marker events mixed into the event
//! stream. The stream stays a pure record of what was observed.

use crate::coordinator::Coordinator;
use crate::events::RuntimeEvent;
use crate::watcher::RuntimeWatcher;
use creature_context_core::project::{ProjectPaths, load_identity};
use creature_context_parsers::incremental::ParseCache;
use creature_context_store::{AtlasRepository, JsonlJournal, write_projections};
use creature_context_types::AtlasSnapshot;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;

/// How often the daemon re-checks whether an on-device model has appeared.
///
/// Availability is not fixed for the life of the process: the daemon may start
/// before the model is ready, the user may enable it later, or a platform may
/// finish downloading it (Android AICore and the Windows GPU path both fetch on
/// demand). Detecting once at startup meant the active lane stayed dark for the
/// whole process lifetime in every one of those cases. The probe is a cheap
/// availability query, so a minute is frequent enough to notice and rare enough
/// to cost nothing.
const MODEL_RECHECK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Notify(#[from] notify::Error),
    #[error(transparent)]
    Scan(#[from] creature_context_core::scan::ScanError),
    #[error(transparent)]
    Index(#[from] creature_context_parsers::index::IndexError),
    #[error(transparent)]
    Store(#[from] creature_context_store::StoreError),
    #[error(transparent)]
    Journal(#[from] creature_context_store::JsonlError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Reconcile once and return the snapshot it produced: run the shared index
/// pipeline (scan, enrich, reconcile identity against the stored snapshot,
/// evaluate Green), replace the snapshot, rewrite the portable projections, and
/// project Green onto native file metadata. Shared by the initial pass and every
/// debounced update so the daemon and the one-shot CLI produce an identical Atlas
/// — the daemon used to scan file-level only, losing the symbols and sockets
/// enrichment adds.
pub fn reconcile_once(root: &Path) -> Result<AtlasSnapshot, ServiceError> {
    let mut cache = ParseCache::new();
    reconcile_once_cached(root, &mut cache)
}

/// As `reconcile_once`, but reusing `cache` so files whose content has not
/// changed are not read or parsed again. The snapshot produced is identical —
/// proved by `an_incrementally_enriched_snapshot_matches_a_full_one` — so the
/// daemon's Atlas stays the same artefact the one-shot `scan` produces. This is
/// what makes the constant background lane affordable: one keystroke used to pay
/// for a full re-parse of every source file in the project.
pub fn reconcile_once_cached(
    root: &Path,
    cache: &mut ParseCache,
) -> Result<AtlasSnapshot, ServiceError> {
    let paths = ProjectPaths::new(root);
    // Opening the repository before the scan means its directory must exist; the
    // old scan-first ordering created it as a side effect.
    if let Some(dir) = paths.database.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut repository = AtlasRepository::open(&paths.database)?;
    let previous = repository.load_snapshot().ok();
    let snapshot =
        creature_context_parsers::index::index_project_cached(root, previous.as_ref(), cache)?;
    repository.replace_snapshot(&snapshot)?;
    write_projections(root, &snapshot, &load_identity(root)?.project_id)?;
    // Keep the live map fresh: re-render ATLAS.png each cycle so a viewer (the
    // menu-bar app watching the file) always shows the current atlas. Best-effort
    // — a render hiccup must never fail the re-index.
    let png = creature_context_store::texture::render_snapshot_png(&snapshot, true);
    let _ = creature_context_core::project::atomic_write(&root.join("ATLAS.png"), &png);
    // Project Green onto native metadata (Finder tags on macOS); a no-op where
    // no adapter exists.
    crate::metadata::apply(root, &snapshot);
    Ok(snapshot)
}

/// One watched project: everything the loop needs to keep a single root current.
///
/// Each root owns its watcher, journal, coordinator and parse cache, because each
/// is per-project state — one project's debounce must not collapse another's
/// events, and one project's parse cache must not be evicted by another's files.
struct WatchedRoot {
    root: PathBuf,
    coordinator: Coordinator,
    journal: JsonlJournal<RuntimeEvent>,
    watcher: RuntimeWatcher,
    cache: ParseCache,
}

impl WatchedRoot {
    fn open(root: &Path) -> Result<Self, ServiceError> {
        let paths = ProjectPaths::new(root);
        let mut coordinator = Coordinator::default();
        let journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal)?;
        // Seed the applied set from the journal so a restart resumes rather than
        // reprocesses. pending() already excludes applied events; marking them
        // keeps the coordinator's in-memory view consistent for this run.
        for id in journal.applied_ids()? {
            coordinator.mark_applied(&id);
        }
        Ok(Self {
            watcher: RuntimeWatcher::new(root)?,
            root: root.to_path_buf(),
            coordinator,
            journal,
            cache: ParseCache::new(),
        })
    }
}

/// Run the resident service for one project until `shutdown` is set.
pub async fn run_service(root: &Path, shutdown: Arc<AtomicBool>) -> Result<(), ServiceError> {
    run_service_multi(std::slice::from_ref(&root.to_path_buf()), shutdown).await
}

/// Run the resident service over several projects in one process.
///
/// One daemon, many roots. Installing a separate supervised daemon per project
/// works, but costs a process, a watcher and a model probe each; a person with
/// several projects wants one background service, not one per directory.
///
/// `shutdown` is an `AtomicBool` the caller flips from a signal handler, so the
/// loop exits cleanly on SIGTERM/SIGINT without dropping in-flight work.
///
/// A failure reconciling one root is logged and skipped rather than propagated:
/// with several projects supervised together, one unreadable or half-deleted
/// project must not take the others down with it.
pub async fn run_service_multi(
    roots: &[PathBuf],
    shutdown: Arc<AtomicBool>,
) -> Result<(), ServiceError> {
    let mut watched: Vec<WatchedRoot> = Vec::new();
    for root in roots {
        watched.push(WatchedRoot::open(root)?);
    }

    // Establish current truth before serving.
    for entry in &mut watched {
        if let Err(error) = reconcile_once_cached(&entry.root, &mut entry.cache) {
            eprintln!(
                "{}: initial reconcile failed: {error}",
                entry.root.display()
            );
        }
    }

    // Whether an on-device model exists. Re-probed periodically rather than
    // fixed at startup — see MODEL_RECHECK_INTERVAL.
    let mut model_available = crate::semantic::model_available();
    let mut model_checked_at = Instant::now();

    while !shutdown.load(Ordering::Relaxed) {
        if model_checked_at.elapsed() >= MODEL_RECHECK_INTERVAL {
            model_available = crate::semantic::model_available();
            model_checked_at = Instant::now();
        }

        let mut any_reconciled = false;
        for entry in &mut watched {
            while let Some(event) = entry.watcher.try_recv() {
                // Journal before processing: a crash now still leaves the event on
                // disk to be replayed.
                entry.journal.append(&event)?;
                entry.coordinator.enqueue(event);
            }

            if !entry.coordinator.settle() {
                continue;
            }
            any_reconciled = true;
            if let Err(error) = reconcile_once_cached(&entry.root, &mut entry.cache) {
                // Leave the events pending so the next settle retries them; a
                // transient failure must not silently drop observed changes.
                eprintln!("{}: reconcile failed: {error}", entry.root.display());
                continue;
            }
            // Mark applied only after the snapshot has committed.
            let applied: Vec<_> = entry
                .coordinator
                .pending_events()
                .iter()
                .map(|e| e.id)
                .collect();
            for id in applied {
                entry.journal.mark_applied(id)?;
                entry.coordinator.mark_applied(&id);
            }
            entry.coordinator.clear_pending();
        }

        // Background semantic lane (spec §7.2): only on an idle tick, so it yields
        // priority to the deterministic lane during active editing. Run off the
        // async thread — the model call is blocking — so watching never stalls;
        // a model or store hiccup is logged, never fatal. One root per idle tick,
        // round-robin, so no project starves another of the model.
        if model_available && !any_reconciled && !watched.is_empty() {
            let index = model_checked_at.elapsed().as_millis() as usize % watched.len();
            let root_buf = watched[index].root.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                crate::semantic::run_pass_if_available(&root_buf, 1)
            })
            .await;
            // A model or store hiccup (or a panic on the blocking thread) is
            // logged, never fatal; the next idle tick retries.
            if let Ok(Err(error)) = outcome {
                eprintln!("semantic lane: {error}");
            }
        }

        let debounce = watched
            .first()
            .map(|entry| entry.coordinator.debounce_duration())
            .unwrap_or_else(|| Duration::from_millis(250));
        tokio::time::sleep(debounce).await;
    }

    Ok(())
}
