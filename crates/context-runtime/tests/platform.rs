//! Milestone 5 Tasks 7–8: platform capability honesty, and portable equivalence.
//!
//! Two properties the platform milestone lives by:
//!
//! - **Honest capability reporting** (spec §16, §18.4): every native capability
//!   reports its true state; nothing that has not run on this platform claims to
//!   be verified.
//! - **Portable equivalence** (spec §16.1, §18.4): the core reasons about one
//!   namespace — canonical, repository-relative paths with `/` separators — so
//!   the Atlas it produces carries no operating-system semantics and is the same
//!   on every platform. Verified here by scanning a fixture and checking that no
//!   platform-specific path leaked into the records.

use creature_context_core::scan::scan_project_configured;
use creature_context_runtime::platform::capabilities;
use creature_context_types::model::CapabilityState;
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cc-platform-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src/inner")).unwrap();
    fs::write(
        root.join("PURPOSE.md"),
        "# Fixture\n\n## Goals\n- portable\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
    fs::write(root.join("src/inner/mod.rs"), "pub fn b() {}\n").unwrap();
    root
}

#[test]
fn the_capability_matrix_reports_true_states() {
    let caps = capabilities();
    assert_eq!(caps.os, std::env::consts::OS);

    if cfg!(target_os = "macos") {
        assert_eq!(
            caps.metadata,
            CapabilityState::Verified,
            "macOS metadata has a real adapter, verified by round-trip"
        );
        assert_eq!(
            caps.watcher,
            CapabilityState::Verified,
            "the portable watcher is verified on macOS by the watcher tests"
        );
    } else {
        // On a platform without a metadata adapter, it must not claim success.
        assert_ne!(
            caps.metadata,
            CapabilityState::Verified,
            "an OS whose metadata adapter has not run never reports Verified"
        );
    }
}

#[test]
fn the_portable_atlas_uses_only_canonical_repo_relative_paths() {
    let root = temp_root("portable");
    let snapshot = scan_project_configured(&root).expect("scan");

    let mut checked = 0;
    for entity in &snapshot.entities {
        let Some(path) = &entity.relative_path else {
            continue;
        };
        checked += 1;
        assert!(
            !path.starts_with('/'),
            "a leaked absolute path: {path:?} — the core must never see one (spec §16.1)"
        );
        assert!(
            !path.contains('\\'),
            "a backslash is a platform separator; the core namespace uses '/': {path:?}"
        );
        assert!(
            !path.contains(':'),
            "a Windows drive/volume prefix leaked into a portable record: {path:?}"
        );
        assert!(
            !path.contains(".."),
            "a path escaping the root is not repository-relative: {path:?}"
        );
        assert!(
            !path.contains(&*root.to_string_lossy()),
            "the OS temp root leaked into a portable record: {path:?}"
        );
    }
    assert!(checked > 0, "the fixture produced file records to check");

    let _ = fs::remove_dir_all(&root);
}
