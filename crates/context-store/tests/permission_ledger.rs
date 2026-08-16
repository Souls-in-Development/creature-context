//! Milestone 3 Task 1: the permission ledger is durable, append-only, and
//! records usage separately from consent.
//!
//! Verified before this: migrations 0004 and 0005 create no permission table,
//! and nothing persists permissions anywhere. Specification 10 requires
//! append-only permission history and that auto-approved activity is usage, not
//! a new human decision.

use creature_context_store::PermissionLedger;
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
fn rules_are_appended_and_read_back() {
    let mut ledger = PermissionLedger::in_memory().expect("open");
    let r = rule(PermissionAction::Read, "src/**", PermissionDecision::Allow);
    ledger.append_rule(&r).expect("append");

    let all = ledger.rules().expect("read");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, r.id);
    assert_eq!(all[0].resource, "src/**");
    assert_eq!(all[0].decision, PermissionDecision::Allow);
}

#[test]
fn recording_a_use_does_not_change_the_rule() {
    let mut ledger = PermissionLedger::in_memory().expect("open");
    let r = rule(PermissionAction::Read, "src/**", PermissionDecision::Allow);
    ledger.append_rule(&r).expect("append");

    ledger.record_use(r.id, "read:src/main.rs").expect("use");
    ledger.record_use(r.id, "read:src/lib.rs").expect("use");

    assert_eq!(
        ledger.rules().expect("rules").len(),
        1,
        "usage is not consent; recording a use must not add or alter a rule"
    );
    assert_eq!(ledger.uses_of(r.id).expect("uses").len(), 2);
}

#[test]
fn superseding_is_append_only() {
    let mut ledger = PermissionLedger::in_memory().expect("open");
    let old = rule(PermissionAction::Read, "src/**", PermissionDecision::Allow);
    ledger.append_rule(&old).expect("append");

    let new = rule(PermissionAction::Read, "src/**", PermissionDecision::Deny);
    ledger.supersede(old.id, &new).expect("supersede");

    let all = ledger.rules().expect("rules");
    assert_eq!(all.len(), 2, "history is retained, not overwritten");
    assert_eq!(
        ledger.superseded_ids().expect("superseded"),
        vec![old.id],
        "the superseded rule is recorded as such, not deleted"
    );
}

#[test]
fn a_reopened_ledger_retains_its_history() {
    let dir = std::env::temp_dir().join(format!("cc-permledger-{}-reopen", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let db = dir.join("permissions.db");

    let r = rule(PermissionAction::Transmit, "**", PermissionDecision::Deny);
    {
        let mut ledger = PermissionLedger::open(&db).expect("open");
        ledger.append_rule(&r).expect("append");
    }
    let ledger = PermissionLedger::open(&db).expect("reopen");
    assert_eq!(
        ledger.rules().expect("rules").len(),
        1,
        "the ledger is durable"
    );
    assert_eq!(ledger.rules().expect("rules")[0].id, r.id);

    let _ = std::fs::remove_dir_all(&dir);
}
