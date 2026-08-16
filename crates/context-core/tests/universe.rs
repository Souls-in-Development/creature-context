//! Milestone 3 Task 5: one Universe holds multiple unrelated Galaxies that
//! never share identity, and a cross-galaxy dependency is isolated unless
//! explicitly declared (specification 2, 7.3).
//!
//! Replaces the test deleted in Milestone 1, which asserted against a mock
//! `Universe` whose `resolve_dependency` unconditionally returned `Isolated` —
//! a tautology. This drives the real registry.

use creature_context_core::universe::{RegisteredGalaxy, Resolution, UniverseRegistry};
use creature_context_types::{EntityId, ProjectId};

fn galaxy(name: &str) -> RegisteredGalaxy {
    RegisteredGalaxy {
        project_id: ProjectId::new(),
        universe_id: EntityId::new(),
        galaxy_id: EntityId::new(),
        root: format!("/projects/{name}").into(),
    }
}

#[test]
fn two_registered_galaxies_keep_distinct_identities() {
    let a = galaxy("alpha");
    let b = galaxy("beta");
    let mut registry = UniverseRegistry::default();
    registry.register(a.clone());
    registry.register(b.clone());

    assert_eq!(
        registry.find(a.project_id).map(|g| g.galaxy_id),
        Some(a.galaxy_id)
    );
    assert_eq!(
        registry.find(b.project_id).map(|g| g.galaxy_id),
        Some(b.galaxy_id)
    );
    assert_ne!(
        a.galaxy_id, b.galaxy_id,
        "unrelated projects must not share a galaxy identity"
    );
}

#[test]
fn a_cross_galaxy_dependency_is_isolated_unless_declared() {
    let a = galaxy("alpha");
    let b = galaxy("beta");
    let mut registry = UniverseRegistry::default();
    registry.register(a.clone());
    registry.register(b.clone());

    assert_eq!(
        registry.resolve_dependency(a.project_id, b.project_id),
        Resolution::Isolated,
        "resemblance is not a relationship; cross-galaxy edges are never inferred"
    );

    registry.link(a.project_id, b.project_id);
    assert_eq!(
        registry.resolve_dependency(a.project_id, b.project_id),
        Resolution::Resolved,
        "an explicitly declared cross-galaxy relationship resolves"
    );
}

#[test]
fn an_unregistered_project_resolves_to_nothing() {
    let a = galaxy("alpha");
    let registry = UniverseRegistry::default();
    assert!(registry.find(a.project_id).is_none());
    assert_eq!(
        registry.resolve_dependency(a.project_id, ProjectId::new()),
        Resolution::Isolated
    );
}

#[test]
fn the_registry_persists_and_reloads() {
    let dir = std::env::temp_dir().join(format!("cc-universe-{}-persist", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("universe.json");

    let a = galaxy("alpha");
    let b = galaxy("beta");
    let mut registry = UniverseRegistry::default();
    registry.register(a.clone());
    registry.register(b.clone());
    registry.link(a.project_id, b.project_id);
    registry.save(&path).expect("save");

    let reloaded = UniverseRegistry::load(&path).expect("load");
    assert_eq!(
        reloaded.find(a.project_id).map(|g| g.galaxy_id),
        Some(a.galaxy_id)
    );
    assert_eq!(
        reloaded.resolve_dependency(a.project_id, b.project_id),
        Resolution::Resolved,
        "an explicit link survives reload"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
