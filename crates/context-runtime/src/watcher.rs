//! Filesystem watcher.
//!
//! Translates raw `notify` events into normalised `RuntimeEvent`s and drops the
//! noise a context service must never react to: its own `ATLAS.idx` writes, the
//! `.creature` state directory, VCS and build output. Without those exclusions
//! the service would watch its own output and reconcile forever.
//!
//! A watcher error becomes an `Overflow` event rather than being swallowed: the
//! stream may have dropped notifications, so truth must be re-established by a
//! rescan (specification 7.1).

use crate::events::{RuntimeEvent, RuntimeEventKind};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Directories whose contents are never project source. `.creature` is excluded
/// because the service writes the database and journal there; `ATLAS.idx` files
/// are excluded because the service writes those too.
const EXCLUDED_DIRS: &[&str] = &[".git", ".creature", "target", "node_modules", ".build"];

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub struct RuntimeWatcher {
    _watcher: RecommendedWatcher,
    receiver: mpsc::Receiver<RuntimeEvent>,
    root: PathBuf,
}

impl RuntimeWatcher {
    pub fn new(root: &Path) -> notify::Result<Self> {
        let (sender, receiver) = mpsc::channel();
        // Canonicalise the root. The backend reports canonical, symlink-resolved
        // paths (on macOS /var is a symlink to /private/var, and temp dirs live
        // under it), so a non-canonical watch root makes every strip_prefix fail
        // and silently drops every event. Without this the service watches but
        // never reconciles.
        //
        // This is adapter work living in the portable core as a bridge — see
        // specification 16.1. Its eventual home is the native watcher under
        // platform/* in the platform milestone; until then this is an
        // explicitly-marked stand-in, not a denial that the boundary exists.
        let root_buf = std::fs::canonicalize(root).map_err(notify::Error::io)?;
        let watch_root = root_buf.clone();

        let mut watcher = RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| {
                let event = match result {
                    Ok(event) => event,
                    Err(_) => {
                        // A dropped or failed notification: the stream is no
                        // longer trustworthy, so demand a rescan.
                        let _ = sender.send(RuntimeEvent::new(RuntimeEventKind::Overflow, now()));
                        return;
                    }
                };

                let kind = match event.kind {
                    EventKind::Create(_) => RuntimeEventKind::FileAdded,
                    EventKind::Modify(_) => RuntimeEventKind::FileModified,
                    EventKind::Remove(_) => RuntimeEventKind::FileRemoved,
                    // `Any` means the backend could not classify the change; a
                    // rescan is the safe response.
                    EventKind::Any => RuntimeEventKind::RescanRequired,
                    EventKind::Access(_) | EventKind::Other => return,
                };

                for path in event.paths {
                    let Ok(relative) = path.strip_prefix(&watch_root) else {
                        continue;
                    };
                    let relative = relative.to_string_lossy().replace('\\', "/");

                    let excluded = Path::new(&relative).components().any(|component| {
                        EXCLUDED_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
                    });
                    if excluded {
                        continue;
                    }
                    // The service's own outputs must never feed back into it.
                    if relative == "ATLAS.idx"
                        || relative.ends_with("/ATLAS.idx")
                        || relative.ends_with(".tmp")
                    {
                        continue;
                    }

                    let _ = sender.send(RuntimeEvent::new(kind, now()).with_path(relative));
                }
            },
            Config::default(),
        )?;

        watcher.watch(&root_buf, RecursiveMode::Recursive)?;

        Ok(Self {
            _watcher: watcher,
            receiver,
            root: root_buf,
        })
    }

    /// Drain one pending event, if any. Non-blocking.
    pub fn try_recv(&self) -> Option<RuntimeEvent> {
        self.receiver.try_recv().ok()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}
