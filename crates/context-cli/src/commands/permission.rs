use crate::output::{OutputFormat, write_output_generic};
use creature_context_core::project::ProjectPaths;
use creature_context_store::PermissionLedger;
use creature_context_types::{
    PermissionId,
    authority::{
        AuthoritySource, PermissionAction, PermissionDecision, PermissionRule, PermissionScope,
    },
};
use std::path::{Path, PathBuf};

type CmdResult = Result<(), Box<dyn std::error::Error>>;

/// Parse an action atom. Unknown actions are rejected, never defaulted — an
/// unrecognised permission is a mistake to surface, not a rule to invent.
fn parse_action(action: &str) -> Result<PermissionAction, String> {
    Ok(
        match action.to_ascii_lowercase().replace('-', "_").as_str() {
            "read" => PermissionAction::Read,
            "index" => PermissionAction::Index,
            "enrich" => PermissionAction::Enrich,
            "write_context" => PermissionAction::WriteContext,
            "write_source" => PermissionAction::WriteSource,
            "move" => PermissionAction::Move,
            "delete" => PermissionAction::Delete,
            "execute" => PermissionAction::Execute,
            "transmit" => PermissionAction::Transmit,
            other => return Err(format!("unknown action '{other}'")),
        },
    )
}

fn parse_scope(scope: &str) -> Result<PermissionScope, String> {
    Ok(match scope.to_ascii_lowercase().as_str() {
        "once" => PermissionScope::Once,
        "session" => PermissionScope::Session,
        "project" => PermissionScope::Project,
        "ongoing" => PermissionScope::Ongoing,
        other => return Err(format!("unknown scope '{other}'")),
    })
}

fn build_rule(
    subject: String,
    action: &str,
    resource: String,
    scope: &str,
    decision: PermissionDecision,
) -> Result<PermissionRule, String> {
    Ok(PermissionRule {
        id: PermissionId::new(),
        subject,
        action: parse_action(action)?,
        resource,
        scope: parse_scope(scope)?,
        decision,
        // A rule entered at the CLI is a human decision.
        authority_source: AuthoritySource::Human,
        created_at: chrono::Utc::now().to_rfc3339(),
        expires_at: None,
        supersedes: None,
    })
}

fn record(project: &Path, rule: PermissionRule) -> CmdResult {
    let ledger_path = ProjectPaths::new(project).permissions;
    let mut ledger = PermissionLedger::open(&ledger_path)?;
    ledger.append_rule(&rule)?;
    println!("recorded {:?} {}: {}", rule.decision, rule.subject, rule.id);
    Ok(())
}

pub fn handle_allow(
    project: PathBuf,
    subject: String,
    action: String,
    resource: String,
    scope: String,
) -> CmdResult {
    let rule = build_rule(
        subject,
        &action,
        resource,
        &scope,
        PermissionDecision::Allow,
    )?;
    record(&project, rule)
}

pub fn handle_deny(
    project: PathBuf,
    subject: String,
    action: String,
    resource: String,
    scope: String,
) -> CmdResult {
    let rule = build_rule(subject, &action, resource, &scope, PermissionDecision::Deny)?;
    record(&project, rule)
}

/// Supersede an existing rule. The replacement keeps the old rule's shape but
/// flips its decision; history is retained by the ledger.
pub fn handle_supersede(project: PathBuf, old_id: String, new_id: String) -> CmdResult {
    let old = PermissionId(uuid::Uuid::parse_str(&old_id)?);
    let ledger_path = ProjectPaths::new(&project).permissions;
    let mut ledger = PermissionLedger::open(&ledger_path)?;

    let existing = ledger
        .rules()?
        .into_iter()
        .find(|r| r.id == old)
        .ok_or_else(|| format!("no rule with id {old_id}"))?;

    let replacement = PermissionRule {
        id: PermissionId(uuid::Uuid::parse_str(&new_id)?),
        decision: match existing.decision {
            PermissionDecision::Allow => PermissionDecision::Deny,
            PermissionDecision::Deny => PermissionDecision::Allow,
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        supersedes: Some(old),
        ..existing
    };
    ledger.supersede(old, &replacement)?;
    println!("superseded {old_id} with {new_id}");
    Ok(())
}

pub fn handle_list(project: PathBuf, format: OutputFormat, compatibility: bool) -> CmdResult {
    let ledger_path = ProjectPaths::new(&project).permissions;
    // An unscanned or fresh project has no ledger file yet — an empty list, not
    // an error.
    let rules = if ledger_path.exists() {
        PermissionLedger::open(&ledger_path)?.rules()?
    } else {
        Vec::new()
    };

    match format {
        OutputFormat::Json | OutputFormat::Yaml => {
            write_output_generic(&rules, format, compatibility)?;
        }
        OutputFormat::Idx | OutputFormat::Markdown => {
            for rule in &rules {
                println!(
                    "@permission id:{} subject:{} action:{} resource:{} decision:{} scope:{}",
                    rule.id,
                    rule.subject,
                    serde_json::to_value(&rule.action)?
                        .as_str()
                        .unwrap_or_default(),
                    rule.resource,
                    serde_json::to_value(&rule.decision)?
                        .as_str()
                        .unwrap_or_default(),
                    serde_json::to_value(&rule.scope)?
                        .as_str()
                        .unwrap_or_default(),
                );
            }
        }
    }
    Ok(())
}
