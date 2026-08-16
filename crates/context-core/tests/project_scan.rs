use creature_context_core::{
    project::{init_project, load_identity},
    purpose::parse_purpose,
    scan::scan_project_configured,
};
use creature_context_types::ScopeScale;
use std::{fs, path::PathBuf};

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("creature-context-{name}-{}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).unwrap();
    }
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn purpose_preserves_protected_decisions() {
    let parsed = parse_purpose(
        "# Demo\n\n## Goals\n- Ship safely\n\n## Protected decisions\n- Models are optional\n",
    );
    assert_eq!(parsed.goals, ["Ship safely"]);
    assert_eq!(parsed.protected_decisions, ["Models are optional"]);
}

#[test]
fn init_is_idempotent() {
    let root = temp_root("init");
    let first = init_project(&root).unwrap();
    let second = init_project(&root).unwrap();
    assert_eq!(first, second);
    assert_eq!(load_identity(&root).unwrap(), first);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scan_builds_all_scales_and_preserves_file_id_after_rename() {
    let root = temp_root("scan");
    fs::write(
        root.join("PURPOSE.md"),
        "# Demo\n\n## Goals\n- Test scanning\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::write(root.join("src/auth/login.rs"), "pub fn login() {}\n").unwrap();
    fs::write(root.join(".DS_Store"), b"finder metadata").unwrap();
    let first = scan_project_configured(&root).unwrap();
    assert!(
        first
            .entities
            .iter()
            .all(|entity| entity.relative_path.as_deref() != Some(".DS_Store"))
    );
    let file = first
        .entities
        .iter()
        .find(|e| e.relative_path.as_deref() == Some("src/auth/login.rs"))
        .unwrap();
    let old_id = file.id;
    let old_system_id = first
        .entities
        .iter()
        .find(|e| e.scale == ScopeScale::System && e.canonical_name == "src")
        .unwrap()
        .id;
    let old_planet_id = first
        .entities
        .iter()
        .find(|e| e.scale == ScopeScale::Planet && e.canonical_name == "src/auth")
        .unwrap()
        .id;
    fs::rename(
        root.join("src/auth/login.rs"),
        root.join("src/auth/session.rs"),
    )
    .unwrap();
    let second = scan_project_configured(&root).unwrap();
    let renamed = second
        .entities
        .iter()
        .find(|e| e.relative_path.as_deref() == Some("src/auth/session.rs"))
        .unwrap();
    assert_eq!(renamed.id, old_id);
    fs::rename(root.join("src"), root.join("source")).unwrap();
    let third = scan_project_configured(&root).unwrap();
    assert_eq!(
        third
            .entities
            .iter()
            .find(|e| e.scale == ScopeScale::System && e.canonical_name == "source")
            .unwrap()
            .id,
        old_system_id
    );
    assert_eq!(
        third
            .entities
            .iter()
            .find(|e| e.scale == ScopeScale::Planet && e.canonical_name == "source/auth")
            .unwrap()
            .id,
        old_planet_id
    );
    for scale in [
        ScopeScale::Universe,
        ScopeScale::Galaxy,
        ScopeScale::System,
        ScopeScale::Planet,
        ScopeScale::Moon,
    ] {
        assert!(third.entities.iter().any(|e| e.scale == scale));
    }
    fs::remove_dir_all(root).unwrap();
}
