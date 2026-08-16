/// Render an IDX field value.
///
/// Bare atoms — no whitespace, no quote, nothing that would make the
/// `key:value` split ambiguous — are emitted unquoted. Everything else uses
/// JSON string escaping, per specification 5.1.
pub fn field(value: &str) -> String {
    let bare = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if bare {
        value.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::field;

    #[test]
    fn bare_atoms_are_unquoted() {
        assert_eq!(field("src/a.rs"), "src/a.rs");
        assert_eq!(field("kebab-case_name.rs"), "kebab-case_name.rs");
    }

    #[test]
    fn values_needing_escape_are_quoted() {
        assert_eq!(field("has space"), r#""has space""#);
        assert_eq!(field(r#"has "quote""#), r#""has \"quote\"""#);
        assert_eq!(field(""), r#""""#);
    }
}
