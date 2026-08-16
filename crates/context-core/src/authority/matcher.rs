//! Deny-precedence permission evaluation (specification 10).
//!
//! The rule that governs the whole authority model: a specific or broad denial
//! outranks any allowance. There is no "most specific wins" — a deny anywhere in
//! the applicable set decides the outcome. Superseded and expired rules do not
//! participate. An action no rule addresses asks.

use crate::authority::decision::{Decision, resource_matches};
use creature_context_types::{
    PermissionId,
    authority::{PermissionAction, PermissionDecision, PermissionRule},
};

/// Evaluate `action` on `resource` against `rules`, ignoring any rule whose id
/// is in `superseded` or whose expiry has passed.
pub fn evaluate(
    rules: &[PermissionRule],
    superseded: &[PermissionId],
    action: PermissionAction,
    resource: &str,
) -> Decision {
    let now = chrono::Utc::now();

    let applies = |rule: &&PermissionRule| -> bool {
        if superseded.contains(&rule.id) {
            return false;
        }
        if let Some(expiry) = &rule.expires_at {
            // A rule with a parseable expiry in the past does not apply. An
            // unparseable expiry is treated as no expiry: dropping a deny rule
            // because its timestamp is malformed would be the unsafe direction.
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(expiry)
                && parsed < now
            {
                return false;
            }
        }
        rule.action == action && resource_matches(&rule.resource, resource)
    };

    let applicable: Vec<&PermissionRule> = rules.iter().filter(applies).collect();

    // Denial precedence is unconditional: a single applicable deny decides.
    if let Some(deny) = applicable
        .iter()
        .find(|rule| rule.decision == PermissionDecision::Deny)
    {
        return Decision::Deny(deny.id);
    }
    if let Some(allow) = applicable
        .iter()
        .find(|rule| rule.decision == PermissionDecision::Allow)
    {
        return Decision::Allow(allow.id);
    }
    Decision::Ask
}
