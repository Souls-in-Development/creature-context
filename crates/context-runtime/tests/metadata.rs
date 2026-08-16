//! Milestone 5 Task 6: native metadata projection.
//!
//! The projection is derived from the Atlas and can be rebuilt from it with no
//! loss (spec §16). The portable half — which tag each entity gets from its Green
//! code — is tested everywhere. The macOS half — writing that tag as a Finder
//! extended attribute and reading, clearing and rebuilding it — is tested on
//! macOS, on real files. Every platform reports its true capability state.

use creature_context_runtime::metadata::{TagAssignment, capability, project, tag_label};
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, EntityId, EntityKind, GreenAssessment, ScopeScale, SnapshotId,
    green::GreenCode,
};
use std::collections::BTreeMap;

const SNAP: &str = "snap-metadata";

fn entity(path: &str, overall: GreenCode) -> AtlasEntity {
    AtlasEntity {
        id: EntityId::new(),
        scale: ScopeScale::Moon,
        kind: EntityKind::File,
        canonical_name: path.into(),
        aliases: vec![],
        relative_path: Some(path.into()),
        parent_id: None,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint: String::new(),
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: Some(GreenAssessment {
            overall,
            axes: BTreeMap::new(),
            snapshot_id: SnapshotId(SNAP.into()),
        }),
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(SNAP.into()),
        observed_at: "2026-08-09T00:00:00Z".into(),
        fresh_until: None,
    }
}

fn snapshot(entities: Vec<AtlasEntity>) -> AtlasSnapshot {
    AtlasSnapshot {
        id: SnapshotId(SNAP.into()),
        timestamp: "2026-08-09T00:00:00Z".into(),
        entities,
        edges: vec![],
        records: vec![],
        conflicts: vec![],
        sources: vec![],
    }
}

#[test]
fn the_projection_derives_one_tag_per_evaluated_file() {
    let snap = snapshot(vec![
        entity("src/a.rs", GreenCode::Green),
        entity("src/b.rs", GreenCode::Red),
    ]);
    let mut assignments = project(&snap);
    assignments.sort_by(|x, y| x.relative_path.cmp(&y.relative_path));

    assert_eq!(
        assignments,
        vec![
            TagAssignment {
                relative_path: "src/a.rs".into(),
                tag: "Green".into()
            },
            TagAssignment {
                relative_path: "src/b.rs".into(),
                tag: "Red".into()
            },
        ]
    );
}

#[test]
fn an_unevaluated_entity_gets_no_tag() {
    let mut e = entity("src/c.rs", GreenCode::Green);
    e.green = None; // never evaluated
    let assignments = project(&snapshot(vec![e]));
    assert!(
        assignments.is_empty(),
        "no Green, no projection — the tag reflects an assessment, not a guess"
    );
}

#[test]
fn tag_labels_cover_every_code() {
    assert_eq!(tag_label(GreenCode::Green), "Green");
    assert_eq!(tag_label(GreenCode::Yellow), "Yellow");
    assert_eq!(tag_label(GreenCode::Red), "Red");
    assert_eq!(tag_label(GreenCode::Unknown), "Unknown");
}

#[test]
fn the_platform_reports_its_true_capability() {
    use creature_context_types::model::CapabilityState;
    let state = capability();
    if cfg!(target_os = "macos") {
        assert_eq!(
            state,
            CapabilityState::Verified,
            "macOS has a real adapter this build runs and the round-trip test verifies"
        );
    } else {
        assert_eq!(
            state,
            CapabilityState::Unavailable,
            "an OS whose adapter has not run reports Unavailable, never fabricated success"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn finder_tags_write_read_clear_and_rebuild_from_the_atlas() {
    use creature_context_runtime::metadata::macos;
    use std::fs;

    let root = std::env::temp_dir().join(format!("cc-metadata-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();

    let snap = snapshot(vec![
        entity("src/a.rs", GreenCode::Green),
        entity("src/b.rs", GreenCode::Red),
    ]);
    let assignments = project(&snap);

    // Write the projection, then read each tag back from the real xattr.
    for a in &assignments {
        macos::write_tag(&root.join(&a.relative_path), &a.tag).expect("write");
    }
    for a in &assignments {
        assert_eq!(
            macos::read_tag(&root.join(&a.relative_path)).as_deref(),
            Some(a.tag.as_str()),
            "the Finder tag reads back as written"
        );
    }

    // Delete the metadata entirely — it carries no truth of its own.
    for a in &assignments {
        macos::clear_tag(&root.join(&a.relative_path)).expect("clear");
        assert_eq!(
            macos::read_tag(&root.join(&a.relative_path)),
            None,
            "the tag is gone after clearing"
        );
    }

    // Rebuild from the Atlas alone — nothing was lost.
    for a in project(&snap) {
        macos::write_tag(&root.join(&a.relative_path), &a.tag).expect("rewrite");
        assert_eq!(
            macos::read_tag(&root.join(&a.relative_path)).as_deref(),
            Some(a.tag.as_str()),
            "rebuilt identically from the Atlas — deleting metadata loses nothing"
        );
    }

    let _ = fs::remove_dir_all(&root);
}
