use creature_context_types::*;

#[test]
fn orbit_defaults_preserve_project_focus_behaviour() {
    let request: OrbitRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(request.scale, OrbitScale::Adaptive);
    assert_eq!(request.mode, OrbitMode::Focus);
    assert_eq!(request.token_budget, 64_000);
}

#[test]
fn all_scales_round_trip_as_snake_case() {
    let scales = [
        ScopeScale::Universe,
        ScopeScale::Galaxy,
        ScopeScale::System,
        ScopeScale::Planet,
        ScopeScale::Moon,
    ];
    let yaml = serde_yaml::to_string(&scales).unwrap();
    assert!(yaml.contains("universe"));
    assert!(yaml.contains("galaxy"));
    assert_eq!(
        serde_yaml::from_str::<Vec<ScopeScale>>(&yaml).unwrap(),
        scales
    );
}

#[test]
fn unknown_orbit_fields_fail_closed() {
    let error = serde_json::from_str::<OrbitRequest>(r#"{"command":"rm -rf"}"#).unwrap_err();
    assert!(error.to_string().contains("unknown field"));
}
