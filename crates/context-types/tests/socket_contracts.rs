use creature_context_types::*;

fn shape() -> SocketShape {
    SocketShape {
        qualified_name: "catalog::Lookup".to_string(),
        structural_signature: "fn lookup(ProductId) -> Option<Product>".to_string(),
        version: "1".to_string(),
        hash: "catalog-lookup-v1".to_string(),
    }
}

#[test]
fn socket_contract_round_trips_without_losing_proof_path_state() {
    let socket = AtlasSocket {
        id: SocketId::new(),
        entity_id: EntityId::new(),
        direction: SocketDirection::Requires,
        shape: shape(),
        optional: false,
        resolution: SocketResolution::Fit(SocketFit {
            provided_socket_id: SocketId::new(),
            basis: FitBasis::Unique,
            status: FitStatus::Unconfirmed,
            checked_by: None,
            proof_path: ProofPathState::Unavailable,
            plane: FitPlane::Inferred,
            confidence: 0.8,
        }),
        source_id: "src/catalog.rs:8".to_string(),
        confidence: 1.0,
        observed_at: "2026-08-04T00:00:00Z".to_string(),
        snapshot_id: SnapshotId("snap-1".to_string()),
    };

    let json = serde_json::to_string(&socket).expect("serialize socket");
    let decoded: AtlasSocket = serde_json::from_str(&json).expect("deserialize socket");

    assert_eq!(decoded, socket);
    assert!(json.contains("\"proof_path\":\"unavailable\""));
    assert!(json.contains("\"status\":\"unconfirmed\""));
}

#[test]
fn failed_fit_and_absent_proof_path_are_distinct_states() {
    assert_ne!(FitStatus::Rejected, FitStatus::Unconfirmed);
    assert_ne!(ProofPathState::Unavailable, ProofPathState::Available);
    assert_ne!(ProofPathState::Unavailable, ProofPathState::Unchecked);
}

#[test]
fn fit_confirmation_rejects_declared_planes_and_weak_proof() {
    assert!(serde_json::from_str::<FitPlane>("\"declared\"").is_err());
    assert!(serde_json::from_str::<FitProof>("\"metadata\"").is_err());
    assert_eq!(
        serde_json::from_str::<FitProof>("\"typecheck\"").unwrap(),
        FitProof::Typecheck
    );
}
