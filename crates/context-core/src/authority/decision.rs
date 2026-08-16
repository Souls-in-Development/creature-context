//! The outcome of evaluating permission rules against a request, and the
//! resource pattern matching it relies on.

use creature_context_types::PermissionId;

/// What evaluation concluded for one request.
///
/// `Allow` and `Deny` name the rule responsible, so the decision is auditable —
/// a caller can report *which* rule denied, not merely that something did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow(PermissionId),
    Deny(PermissionId),
    /// No rule addresses this request; the human must decide (specification 10:
    /// unknown actions ask).
    Ask,
}

/// Whether a resource pattern matches a concrete path.
///
/// Segment-wise glob: `**` matches zero or more segments (including across `/`),
/// `*` matches exactly one segment, any other segment matches itself literally.
/// This is deliberately simple and total — a permission decision must never
/// depend on a regex engine's edge cases.
pub fn resource_matches(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let seg: Vec<&str> = path.split('/').collect();
    segments_match(&pat, &seg)
}

fn segments_match(pat: &[&str], path: &[&str]) -> bool {
    match pat.first() {
        None => path.is_empty(),
        Some(&"**") => {
            // Consume zero or more path segments.
            (0..=path.len()).any(|consumed| segments_match(&pat[1..], &path[consumed..]))
        }
        Some(&head) => match path.first() {
            None => false,
            Some(&first) if head == "*" || head == first => segments_match(&pat[1..], &path[1..]),
            Some(_) => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::resource_matches;

    #[test]
    fn double_star_matches_any_depth() {
        assert!(resource_matches("**", "a/b/c"));
        assert!(resource_matches("src/**", "src/a.rs"));
        assert!(resource_matches("src/**", "src/deep/a.rs"));
        assert!(resource_matches("secrets/**", "secrets/key.pem"));
    }

    #[test]
    fn literal_matches_only_itself() {
        assert!(resource_matches("src/main.rs", "src/main.rs"));
        assert!(!resource_matches("src/main.rs", "src/other.rs"));
        assert!(!resource_matches("src/**", "tests/a.rs"));
    }

    #[test]
    fn single_star_matches_one_segment() {
        assert!(resource_matches("src/*", "src/a.rs"));
        assert!(!resource_matches("src/*", "src/deep/a.rs"));
    }
}
