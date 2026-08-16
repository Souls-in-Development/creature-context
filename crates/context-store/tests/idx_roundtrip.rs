use creature_context_store::{IdxScope, decode_atlas_idx, encode_atlas_idx};
use creature_context_types::{RelationshipKind, ScopeScale};

mod support;
use support::{edge, entity, snapshot_with};

#[test]
fn round_trip_preserves_entities_and_edges() {
    let alpha = entity("alpha.rs", "src/alpha.rs", ScopeScale::Moon);
    let beta = entity("beta", "src/beta", ScopeScale::Planet);
    let name_with_spaces = entity(
        "name with spaces",
        "src/dir with space/c.rs",
        ScopeScale::Moon,
    );

    let mut original = snapshot_with(vec![alpha.clone(), beta.clone(), name_with_spaces.clone()]);
    original
        .edges
        .push(edge(&alpha, &beta, RelationshipKind::Imports));

    let encoded = encode_atlas_idx(&original, IdxScope::Galaxy, &project_id()).expect("encode");
    let decoded = decode_atlas_idx(&encoded).expect("decode");

    assert_eq!(decoded.snapshot.id, original.id, "snapshot id must survive");
    assert_eq!(
        decoded.snapshot.entities.len(),
        3,
        "decoder must return the encoded entities, not a hardcoded empty snapshot"
    );

    let names: Vec<_> = decoded
        .snapshot
        .entities
        .iter()
        .map(|e| e.canonical_name.as_str())
        .collect();
    assert!(
        names.contains(&"name with spaces"),
        "escaped fields must decode"
    );
}

#[test]
fn re_encoding_a_decoded_snapshot_is_byte_identical() {
    let original = snapshot_with(vec![entity("alpha.rs", "src/alpha.rs", ScopeScale::Moon)]);
    let project_id = project_id();
    let once = encode_atlas_idx(&original, IdxScope::Galaxy, &project_id).expect("encode");
    let decoded = decode_atlas_idx(&once).expect("decode");
    let twice =
        encode_atlas_idx(&decoded.snapshot, IdxScope::Galaxy, &project_id).expect("re-encode");
    assert_eq!(once, twice, "encode/decode must be a true identity");
}

#[test]
fn unknown_record_types_are_preserved_not_dropped() {
    let original = snapshot_with(vec![entity("alpha.rs", "src/alpha.rs", ScopeScale::Moon)]);
    let mut encoded = encode_atlas_idx(&original, IdxScope::Galaxy, &project_id()).expect("encode");
    encoded.push_str("@future-record id:x novel-field:1\n");

    let decoded = decode_atlas_idx(&encoded).expect("decode");
    assert_eq!(
        decoded.opaque_records.len(),
        1,
        "forward-compatible records must be retained, per specification 5.1"
    );
}

#[test]
fn malformed_input_fails_closed() {
    let err = decode_atlas_idx("@entity id:not-a-uuid scale:nonsense\n");
    assert!(
        err.is_err(),
        "invalid records must error, never yield a fabricated snapshot"
    );
}

fn project_id() -> creature_context_types::ProjectId {
    creature_context_types::ProjectId(
        uuid::Uuid::parse_str("019fcb87-5aa3-74f2-aed2-1a8e998986c5").unwrap(),
    )
}
