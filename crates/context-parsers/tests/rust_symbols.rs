//! Milestone 4 Task 1: the walking skeleton — parse one language (Rust) end to
//! end and extract its declarations with names and spans.
//!
//! This exercises the whole real mechanism on one grammar before scaling to
//! ~44: the vendored grammar C compiled by build.rs, the tree-sitter crate at
//! an ABI that loads it, and the extraction traversal ported from
//! creature-clean's TreeSitterAdapter.

use creature_context_parsers::adapter::{SymbolKind, parse_rust};

const SOURCE: &str = r#"
use std::fmt;

pub struct Widget {
    pub id: u32,
}

fn make_widget(id: u32) -> Widget {
    Widget { id }
}

trait Render {
    fn render(&self) -> String;
}
"#;

#[test]
fn extracts_functions_structs_and_traits_with_names() {
    let symbols = parse_rust(SOURCE).expect("parse");
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"Widget"), "struct not extracted: {names:?}");
    assert!(
        names.contains(&"make_widget"),
        "function not extracted: {names:?}"
    );
    assert!(names.contains(&"Render"), "trait not extracted: {names:?}");
}

#[test]
fn extracted_symbols_carry_their_kind() {
    let symbols = parse_rust(SOURCE).expect("parse");
    let kind_of = |name: &str| symbols.iter().find(|s| s.name == name).map(|s| s.kind);

    assert_eq!(kind_of("Widget"), Some(SymbolKind::Struct));
    assert_eq!(kind_of("make_widget"), Some(SymbolKind::Function));
    assert_eq!(kind_of("Render"), Some(SymbolKind::Trait));
}

#[test]
fn spans_are_one_indexed_and_bound_the_declaration() {
    let symbols = parse_rust(SOURCE).expect("parse");
    let widget = symbols.iter().find(|s| s.name == "Widget").expect("Widget");

    // The struct starts on source line 4 (1-indexed), after the blank line and
    // the `use`.
    assert_eq!(
        widget.start_line, 4,
        "start line must be 1-indexed: {widget:?}"
    );
    assert!(
        widget.end_line >= widget.start_line,
        "end line must not precede start: {widget:?}"
    );
}

#[test]
fn empty_source_yields_no_symbols_without_error() {
    let symbols = parse_rust("").expect("parse empty");
    assert!(
        symbols.is_empty(),
        "empty source has no declarations, and is not an error"
    );
}
