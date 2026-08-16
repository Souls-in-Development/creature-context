//! Milestone 3 Task 2: deny-precedence permission evaluation (specification 10).
//!
//! Replaces the test deleted in Milestone 1, which defined a mock `Rule`/`evaluate`
//! inside the test file and never imported the crate. This one drives the real
//! `creature_context_core::authority::evaluate`.
//!
//! The rules under test: a specific or broad denial outranks any allowance,
//! superseded and expired rules are ignored, and an unmatched action asks.

use creature_context_core::authority::{Decision, evaluate};
use creature_context_types::{
    PermissionId,
    authority::{
        AuthoritySource, PermissionAction, PermissionDecision, PermissionRule, PermissionScope,
    },
};

fn rule(action: PermissionAction, resource: &str, decision: PermissionDecision) -> PermissionRule {
    PermissionRule {
        id: PermissionId::new(),
        subject: "agent:codex".into(),
        action,
        resource: resource.into(),
        scope: PermissionScope::Ongoing,
        decision,
        authority_source: AuthoritySource::Human,
        created_at: "2026-08-05T00:00:00Z".into(),
        expires_at: None,
        supersedes: None,
    }
}

#[test]
fn an_unmatched_action_asks() {
    let rules = vec![rule(
        PermissionAction::Read,
        "src/**",
        PermissionDecision::Allow,
    )];
    assert_eq!(
        evaluate(&rules, &[], PermissionAction::WriteSource, "src/a.rs"),
        Decision::Ask,
        "an action no rule addresses is neither allowed nor denied"
    );
}

#[test]
fn a_matching_allow_allows() {
    let rules = vec![rule(
        PermissionAction::Read,
        "src/**",
        PermissionDecision::Allow,
    )];
    assert!(matches!(
        evaluate(&rules, &[], PermissionAction::Read, "src/a.rs"),
        Decision::Allow(_)
    ));
}

#[test]
fn a_specific_deny_outranks_a_broad_allow() {
    let rules = vec![
        rule(PermissionAction::Read, "**", PermissionDecision::Allow),
        rule(
            PermissionAction::Read,
            "secrets/**",
            PermissionDecision::Deny,
        ),
    ];
    assert!(matches!(
        evaluate(&rules, &[], PermissionAction::Read, "secrets/key.pem"),
        Decision::Deny(_)
    ));
}

#[test]
fn a_broad_deny_outranks_a_specific_allow() {
    let rules = vec![
        rule(
            PermissionAction::Transmit,
            "reports/a.md",
            PermissionDecision::Allow,
        ),
        rule(PermissionAction::Transmit, "**", PermissionDecision::Deny),
    ];
    assert!(
        matches!(
            evaluate(&rules, &[], PermissionAction::Transmit, "reports/a.md"),
            Decision::Deny(_)
        ),
        "denial precedence is unconditional, independent of specificity"
    );
}

#[test]
fn a_superseded_rule_does_not_grant() {
    let old = rule(PermissionAction::Read, "src/**", PermissionDecision::Allow);
    let rules = vec![old.clone()];
    assert_eq!(
        evaluate(&rules, &[old.id], PermissionAction::Read, "src/a.rs"),
        Decision::Ask,
        "a superseded allow no longer grants"
    );
}

#[test]
fn an_expired_rule_is_ignored() {
    let mut expired = rule(PermissionAction::Read, "src/**", PermissionDecision::Allow);
    expired.expires_at = Some("2020-01-01T00:00:00Z".into());
    assert_eq!(
        evaluate(&[expired], &[], PermissionAction::Read, "src/a.rs"),
        Decision::Ask,
        "a rule past its expiry does not apply"
    );
}

#[test]
fn a_literal_resource_matches_only_itself() {
    let rules = vec![rule(
        PermissionAction::Read,
        "src/main.rs",
        PermissionDecision::Allow,
    )];
    assert!(matches!(
        evaluate(&rules, &[], PermissionAction::Read, "src/main.rs"),
        Decision::Allow(_)
    ));
    assert_eq!(
        evaluate(&rules, &[], PermissionAction::Read, "src/other.rs"),
        Decision::Ask,
        "a literal pattern does not match a different path"
    );
}
