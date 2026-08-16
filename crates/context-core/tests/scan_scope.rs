//! Scan configuration is real: limits are read from the project, exceeding one
//! truncates honestly instead of failing, and scope decides what is walked.
//!
//! Before this, `.creature/config.toml` was written at init and never read by
//! anything, so the only way to change the scanner was to edit Rust and
//! recompile — and the default ceiling of 100,000 files was below the size of
//! real projects, with a breach killing the resident daemon rather than
//! degrading.

use creature_context_core::config::ScanConfig;
use creature_context_core::scan::{scan_project_configured, scan_project_with};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cc-scope-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("keep")).unwrap();
    fs::create_dir_all(root.join("drop")).unwrap();
    fs::create_dir_all(root.join("noisy")).unwrap();
    fs::write(root.join("PURPOSE.md"), "# Fixture\n\n## Goals\n- scope\n").unwrap();
    fs::write(root.join("keep/wanted.rs"), "pub fn wanted() {}\n").unwrap();
    fs::write(root.join("drop/unwanted.rs"), "pub fn unwanted() {}\n").unwrap();
    fs::write(root.join("noisy/skipme.rs"), "pub fn skipme() {}\n").unwrap();
    root
}

fn write_config(root: &Path, body: &str) {
    let creature = root.join(".creature");
    fs::create_dir_all(&creature).unwrap();
    fs::write(creature.join("config.toml"), body).unwrap();
}

fn paths(snapshot: &creature_context_types::AtlasSnapshot) -> Vec<String> {
    snapshot
        .entities
        .iter()
        .filter_map(|e| e.relative_path.clone())
        .collect()
}

/// `include` makes the root a container rather than the subject — the thing that
/// lets a home directory be a root without indexing all of it.
#[test]
fn include_limits_the_walk_to_named_subtrees() {
    let root = fixture("include");
    // init first so writing the config does not race the scanner creating it.
    scan_project_configured(&root).expect("seed");
    write_config(
        &root,
        "schema_version = 1\n\n[scope]\ninclude = [\"keep\"]\n",
    );

    let snapshot = scan_project_configured(&root).expect("scan");
    let files = paths(&snapshot);

    assert!(
        files.iter().any(|p| p.contains("keep/wanted.rs")),
        "the included subtree must be indexed: {files:?}"
    );
    assert!(
        !files.iter().any(|p| p.contains("drop/unwanted.rs")),
        "a subtree outside `include` must not be indexed: {files:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// `exclude` drops a directory anywhere beneath the root, on top of the built-ins.
#[test]
fn exclude_drops_a_named_directory() {
    let root = fixture("exclude");
    scan_project_configured(&root).expect("seed");
    write_config(
        &root,
        "schema_version = 1\n\n[scope]\nexclude = [\"noisy\"]\n",
    );

    let files = paths(&scan_project_configured(&root).expect("scan"));

    assert!(
        !files.iter().any(|p| p.contains("noisy/skipme.rs")),
        "an excluded directory must not be indexed: {files:?}"
    );
    assert!(
        files.iter().any(|p| p.contains("keep/wanted.rs")),
        "everything else must still be indexed: {files:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// The load-bearing change: a ceiling truncates and records why, rather than
/// returning an error that kills the daemon. The Atlas is real but partial, and
/// says so.
#[test]
fn exceeding_a_limit_truncates_and_records_it_rather_than_failing() {
    let root = fixture("truncate");
    let config: ScanConfig = toml::from_str("schema_version = 1\nmax_files = 1\n").expect("config");

    let snapshot = scan_project_with(&root, &config).expect("a limit must not fail the scan");

    let galaxy = snapshot
        .entities
        .iter()
        .find(|e| e.scale == creature_context_types::ScopeScale::Galaxy)
        .expect("the project entity");
    assert!(
        galaxy
            .uncertainty
            .iter()
            .any(|u| u.contains("truncated") && u.contains("max_files")),
        "truncation must be recorded on the project entity, got {:?}",
        galaxy.uncertainty
    );

    let _ = fs::remove_dir_all(&root);
}

/// Zero means unlimited, which is what makes a project larger than the old
/// 100,000-file ceiling scannable at all.
#[test]
fn a_zero_limit_is_unlimited_and_records_no_truncation() {
    let root = fixture("unlimited");
    let config: ScanConfig =
        toml::from_str("schema_version = 1\nmax_files = 0\nmax_total_bytes = 0\n").expect("config");

    let snapshot = scan_project_with(&root, &config).expect("scan");
    let galaxy = snapshot
        .entities
        .iter()
        .find(|e| e.scale == creature_context_types::ScopeScale::Galaxy)
        .expect("the project entity");

    assert!(
        !galaxy.uncertainty.iter().any(|u| u.contains("truncated")),
        "an unlimited scan must not report truncation: {:?}",
        galaxy.uncertainty
    );
    assert!(
        paths(&snapshot).len() >= 3,
        "every fixture file should be indexed"
    );

    let _ = fs::remove_dir_all(&root);
}

/// The config on disk must actually drive the scan — the whole point. Writing a
/// restrictive scope and rescanning must change the result without recompiling.
#[test]
fn the_config_file_on_disk_changes_what_is_scanned() {
    let root = fixture("readsconfig");
    let before = paths(&scan_project_configured(&root).expect("scan"));
    assert!(
        before.iter().any(|p| p.contains("drop/unwanted.rs")),
        "baseline should index everything: {before:?}"
    );

    write_config(
        &root,
        "schema_version = 1\n\n[scope]\ninclude = [\"keep\"]\n",
    );
    let after = paths(&scan_project_configured(&root).expect("rescan"));

    assert!(
        !after.iter().any(|p| p.contains("drop/unwanted.rs")),
        "editing config.toml must change the scan: {after:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// A Python virtual environment (whatever it is named) and the common tool caches
/// are installed dependencies or generated files, not project source, so they are
/// skipped like `node_modules`. The venv is recognised by its `pyvenv.cfg`, so a
/// non-standard name like `.venv311` is still caught.
#[test]
fn virtualenvs_and_python_caches_are_not_indexed() {
    let root = std::env::temp_dir().join(format!("cc-venv-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.py"), "def app():\n    return 1\n").unwrap();
    // A virtual environment under a non-standard name, marked by pyvenv.cfg.
    fs::create_dir_all(root.join(".venv311/lib/site-packages/dep")).unwrap();
    fs::write(root.join(".venv311/pyvenv.cfg"), "home = /usr\n").unwrap();
    fs::write(
        root.join(".venv311/lib/site-packages/dep/mod.py"),
        "def dep():\n    return 2\n",
    )
    .unwrap();
    // A bytecode cache directory next to real source.
    fs::create_dir_all(root.join("src/__pycache__")).unwrap();
    fs::write(root.join("src/__pycache__/app.cpython-311.pyc"), "junk").unwrap();
    fs::write(root.join("PURPOSE.md"), "# Fixture\n\n## Goals\n- venv\n").unwrap();

    let files = paths(&scan_project_configured(&root).expect("scan"));

    assert!(
        files.iter().any(|p| p.contains("src/app.py")),
        "project source must be indexed: {files:?}"
    );
    assert!(
        !files.iter().any(|p| p.contains(".venv311")),
        "a virtualenv (pyvenv.cfg) must not be indexed: {files:?}"
    );
    assert!(
        !files.iter().any(|p| p.contains("__pycache__")),
        "a bytecode cache must not be indexed: {files:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

/// Xcode build output — a `*.noindex` folder and a repo-local DerivedData dir —
/// is generated build material and vendored dependency checkouts, not project
/// source, so it is skipped like `node_modules`.
#[test]
fn xcode_build_artifacts_are_not_indexed() {
    let root = std::env::temp_dir().join(format!("cc-noindex-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.swift"), "func app() {}\n").unwrap();
    // An Xcode build-intermediates folder (Spotlight-excluded via .noindex).
    fs::create_dir_all(root.join("app/Build/Intermediates.noindex/x")).unwrap();
    fs::write(root.join("app/Build/Intermediates.noindex/x/gen.swift"), "func gen() {}\n").unwrap();
    // A repo-local DerivedData with a vendored dependency checkout.
    fs::create_dir_all(root.join(".build-derived/SourcePackages/checkouts/dep")).unwrap();
    fs::write(root.join(".build-derived/SourcePackages/checkouts/dep/lib.swift"), "func dep() {}\n").unwrap();
    fs::write(root.join("PURPOSE.md"), "# Fixture\n\n## Goals\n- build\n").unwrap();

    let files = paths(&scan_project_configured(&root).expect("scan"));
    assert!(files.iter().any(|p| p.contains("src/app.swift")), "source indexed: {files:?}");
    assert!(!files.iter().any(|p| p.contains(".noindex")), "*.noindex must be skipped: {files:?}");
    assert!(!files.iter().any(|p| p.contains(".build-derived")), "DerivedData must be skipped: {files:?}");

    let _ = fs::remove_dir_all(&root);
}
