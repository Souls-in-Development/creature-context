//! Registering the resident service with the OS supervisor (spec §7.1, §16).
//!
//! The definition is generated as data before anything is written, so these
//! assert the exact contents and hand them to the platform's own validator.
//! Nothing here installs anything: `install` boots a real background process, so
//! it is exercised deliberately, not by the test suite.

use creature_context_runtime::daemon;
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cc-daemon-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

/// The label must be stable for a project and distinct between projects — it is
/// what makes installing twice a replacement rather than a second daemon.
#[test]
fn the_label_is_stable_per_project_and_distinct_between_projects() {
    let a = temp_root("label-a");
    let b = temp_root("label-b");

    let first = daemon::label_for(&a).expect("label");
    let again = daemon::label_for(&a).expect("label");
    let other = daemon::label_for(&b).expect("label");

    assert_eq!(
        first, again,
        "the same project must always get the same label"
    );
    assert_ne!(first, other, "two projects must not share a label");
    assert!(
        first.starts_with("com.creature-context."),
        "unexpected label form: {first}"
    );

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

/// The definition must name the real binary, the `run` subcommand and the
/// project root — that triple is the whole contract with the supervisor. A
/// definition that omits the root would supervise the wrong project.
#[test]
fn the_definition_invokes_run_against_this_project() {
    let root = temp_root("definition");
    let binary = PathBuf::from("/usr/local/bin/creature-context");
    let definition =
        daemon::definition_with_binary(std::slice::from_ref(&root), &binary).expect("definition");

    let canonical = root.canonicalize().unwrap();
    let contents = &definition.contents;
    assert!(
        contents.contains("/usr/local/bin/creature-context"),
        "the binary must be named absolutely:\n{contents}"
    );
    assert!(contents.contains("run"), "the run subcommand must be named");
    assert!(
        contents.contains(&canonical.to_string_lossy().to_string()),
        "the project root must be named:\n{contents}"
    );
    assert!(
        definition
            .unit_path
            .to_string_lossy()
            .contains(&definition.label),
        "the unit file must be named for the label"
    );

    let _ = fs::remove_dir_all(&root);
}

/// The daemon has no terminal, so its output must land in the project where it
/// can be read.
#[test]
fn the_daemon_logs_into_the_project() {
    let root = temp_root("log");
    let log = daemon::log_path(&root);
    assert!(
        log.starts_with(&root),
        "the log must live under the project"
    );
    assert!(
        log.to_string_lossy().contains(".creature"),
        "the log belongs in .creature, got {}",
        log.display()
    );
    let _ = fs::remove_dir_all(&root);
}

/// A generated plist is only trustworthy if launchd's own parser accepts it.
/// `plutil -lint` is that parser — the same check the Finder-tag projection uses
/// to prove its binary plists are real.
#[cfg(target_os = "macos")]
#[test]
fn the_generated_plist_is_valid_according_to_plutil() {
    let root = temp_root("plutil");
    let binary = PathBuf::from("/usr/local/bin/creature-context");
    let definition =
        daemon::definition_with_binary(std::slice::from_ref(&root), &binary).expect("definition");

    let path = root.join("candidate.plist");
    fs::write(&path, &definition.contents).unwrap();

    let output = std::process::Command::new("plutil")
        .arg("-lint")
        .arg(&path)
        .output()
        .expect("run plutil");
    assert!(
        output.status.success(),
        "plutil rejected the generated plist: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&root);
}

/// A path containing XML metacharacters must still produce a plist launchd can
/// parse. Repository paths are arbitrary user strings; `&` in one of them must
/// not silently corrupt the definition.
#[cfg(target_os = "macos")]
#[test]
fn a_path_with_xml_metacharacters_still_produces_a_valid_plist() {
    let root = temp_root("meta & <chars>");
    let binary = PathBuf::from("/usr/local/bin/creature-context");
    let definition =
        daemon::definition_with_binary(std::slice::from_ref(&root), &binary).expect("definition");

    assert!(
        !definition.contents.contains(" & "),
        "a raw ampersand must have been escaped"
    );

    let path = std::env::temp_dir().join(format!("cc-daemon-meta-{}.plist", std::process::id()));
    fs::write(&path, &definition.contents).unwrap();
    let output = std::process::Command::new("plutil")
        .arg("-lint")
        .arg(&path)
        .output()
        .expect("run plutil");
    assert!(
        output.status.success(),
        "plutil rejected a plist built from a path with metacharacters: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir_all(&root);
}

/// Status must be readable for a project that was never installed, and must say
/// so rather than erroring.
#[test]
fn status_of_an_uninstalled_project_is_not_installed() {
    let root = temp_root("status");
    let status = daemon::status(std::slice::from_ref(&root)).expect("status");
    assert!(!status.installed, "nothing was installed");
    assert!(!status.loaded, "nothing was loaded");
    let _ = fs::remove_dir_all(&root);
}

/// One daemon can watch several projects, and the registration is identified by
/// the whole set — so the same projects in a different order are the same
/// daemon, not a second one.
#[test]
fn a_root_set_has_one_stable_order_independent_label() {
    let a = temp_root("set-a");
    let b = temp_root("set-b");

    let forward = daemon::label_for_roots(&[a.clone(), b.clone()]).expect("label");
    let reversed = daemon::label_for_roots(&[b.clone(), a.clone()]).expect("label");
    let duplicated = daemon::label_for_roots(&[a.clone(), b.clone(), a.clone()]).expect("label");
    let single = daemon::label_for_roots(std::slice::from_ref(&a)).expect("label");

    assert_eq!(forward, reversed, "order must not change the identity");
    assert_eq!(duplicated, forward, "a repeated root must not change it");
    assert_ne!(
        forward, single,
        "a different set of roots must be a different daemon"
    );

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

/// Every watched root must reach the supervised command line, or the daemon
/// would silently watch fewer projects than were installed.
#[test]
fn the_definition_names_every_watched_root() {
    let a = temp_root("multi-a");
    let b = temp_root("multi-b");
    let binary = PathBuf::from("/usr/local/bin/creature-context");

    let definition =
        daemon::definition_with_binary(&[a.clone(), b.clone()], &binary).expect("definition");

    for root in [&a, &b] {
        let canonical = root.canonicalize().unwrap();
        assert!(
            definition
                .contents
                .contains(&canonical.to_string_lossy().to_string()),
            "every root must appear in the definition; {} missing from:\n{}",
            canonical.display(),
            definition.contents
        );
    }

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}

/// The multi-root plist must still be something launchd will parse.
#[cfg(target_os = "macos")]
#[test]
fn a_multi_root_plist_is_valid_according_to_plutil() {
    let a = temp_root("multi-plist-a");
    let b = temp_root("multi-plist-b");
    let binary = PathBuf::from("/usr/local/bin/creature-context");
    let definition =
        daemon::definition_with_binary(&[a.clone(), b.clone()], &binary).expect("definition");

    let path = a.join("multi.plist");
    fs::write(&path, &definition.contents).unwrap();
    let output = std::process::Command::new("plutil")
        .arg("-lint")
        .arg(&path)
        .output()
        .expect("run plutil");
    assert!(
        output.status.success(),
        "plutil rejected the multi-root plist: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(&a);
    let _ = fs::remove_dir_all(&b);
}
