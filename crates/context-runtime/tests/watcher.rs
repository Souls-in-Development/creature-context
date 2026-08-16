//! Regression guard for the watch-root canonicalisation.
//!
//! An end-to-end proof caught this: the notify backend reports canonical,
//! symlink-resolved paths (on macOS /var is a symlink to /private/var, where
//! temp dirs live), so a non-canonical watch root made every strip_prefix fail
//! and silently dropped every event. `run` stayed alive and reconciled nothing.
//!
//! Testing the drop-path end to end means waiting on FSEvents and is flaky.
//! This instead asserts the invariant that prevents it — the watch root is
//! canonical — which is deterministic and platform-independent (canonicalising
//! a path with no symlink components returns it unchanged).

use creature_context_runtime::watcher::RuntimeWatcher;
use std::fs;

#[test]
fn watch_root_is_canonicalised_so_backend_paths_match() {
    let root = std::env::temp_dir().join(format!("cc-watcher-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temp root");

    let watcher = RuntimeWatcher::new(&root).expect("watcher opens");

    assert_eq!(
        watcher.root(),
        fs::canonicalize(&root).expect("canonicalize").as_path(),
        "the watch root must be canonical; otherwise the backend's symlink-resolved event \
         paths fail strip_prefix and every event is dropped"
    );

    drop(watcher);
    let _ = fs::remove_dir_all(&root);
}
