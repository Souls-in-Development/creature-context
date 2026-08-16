use crate::{AtlasSnapshot, RelationshipKind, RelationshipPlane};

/// Architectural violations that must halt the build.
///
/// Only the `declared` plane is enforced: a human wrote the rule, so it does not
/// change when a scan runs. `observed` and `inferred` evidence — including all
/// Green assessment — informs but never gates, otherwise a stale or Red scan
/// would make the toolchain unbuildable (see the design note in the plan).
pub fn violations(snapshot: &AtlasSnapshot) -> Vec<String> {
    if std::env::var_os("CREATURE_CONTEXT_NO_ENFORCE").is_some() {
        return Vec::new();
    }

    let name = |id| {
        snapshot
            .entities
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.canonical_name.as_str())
            .unwrap_or("<unknown>")
    };

    // A declared `conflicts` edge marked required is a prohibition:
    // "these two must never be connected."
    let prohibitions: Vec<_> = snapshot
        .edges
        .iter()
        .filter(|e| {
            e.plane == RelationshipPlane::Declared
                && e.required
                && e.kind == RelationshipKind::Conflicts
        })
        .collect();

    let mut found = Vec::new();
    for rule in prohibitions {
        let breached = snapshot.edges.iter().any(|e| {
            e.plane == RelationshipPlane::Observed
                && matches!(
                    e.kind,
                    RelationshipKind::Imports
                        | RelationshipKind::Calls
                        | RelationshipKind::References
                )
                && e.source_entity_id == rule.source_entity_id
                && e.target_entity_id == rule.target_entity_id
        });
        if breached {
            found.push(format!(
                "Creature Context: declared rule forbids {} -> {}, but an observed dependency exists. \
                 Remove the dependency, or amend the rule in PURPOSE.md / .atlas. \
                 Set CREATURE_CONTEXT_NO_ENFORCE=1 to build anyway.",
                name(rule.source_entity_id),
                name(rule.target_entity_id),
            ));
        }
    }
    found
}
