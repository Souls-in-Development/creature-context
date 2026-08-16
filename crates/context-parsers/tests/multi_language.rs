//! Milestone 4 Task 3: the full grammar set — extraction across languages, with
//! a deterministic fallback for languages that have no grammar.
//!
//! The generic path uses the ported `looksLikeDeclaration` heuristic plus the
//! construct registry, so a Python `function_definition` and a Go
//! `function_declaration` both resolve to the `function` shared construct
//! without Rust-specific node knowledge.

use creature_context_parsers::adapter::{Construct, parse};
use creature_context_parsers::languages::{
    language_for, language_for_extension, supported_languages,
};

use creature_context_parsers::adapter::ParsedSymbol;

fn canonical(symbols: &[ParsedSymbol], name: &str) -> Option<String> {
    symbols
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| match &s.construct {
            Construct::Shared(c) => Some(c.clone()),
            Construct::Native(_) => None,
        })
}

#[test]
fn python_declarations_are_extracted_and_typed() {
    let source = "def greet(name):\n    return name\n\nclass Widget:\n    pass\n";
    let symbols = parse(source, "python").expect("parse python");

    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"greet"),
        "python function not extracted: {names:?}"
    );
    assert!(
        names.contains(&"Widget"),
        "python class not extracted: {names:?}"
    );
    assert_eq!(canonical(&symbols, "greet").as_deref(), Some("function"));
    assert_eq!(canonical(&symbols, "Widget").as_deref(), Some("class"));
}

#[test]
fn go_declarations_are_extracted() {
    let source = "package main\n\nfunc Add(a int, b int) int { return a + b }\n\ntype Point struct { X int }\n";
    let symbols = parse(source, "go").expect("parse go");
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"Add"),
        "go function not extracted: {names:?}"
    );
    assert!(names.contains(&"Point"), "go type not extracted: {names:?}");
}

#[test]
fn typescript_declarations_are_extracted() {
    let source = "function hello(): string { return \"hi\"; }\nclass Box { }\n";
    let symbols = parse(source, "typescript").expect("parse ts");
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&"hello"),
        "ts function not extracted: {names:?}"
    );
    assert!(names.contains(&"Box"), "ts class not extracted: {names:?}");
}

#[test]
fn an_unsupported_language_degrades_without_error() {
    // No grammar for this key — a caller must be able to tell, not crash, and
    // the deterministic file-level scan still stands (spec §17).
    assert!(language_for("klingon").is_none());
    assert!(parse("whatever", "klingon").is_err());
}

#[test]
fn the_full_grammar_set_is_available() {
    let langs = supported_languages();
    assert!(
        langs.len() >= 40,
        "expected the full vendored set, got {}",
        langs.len()
    );
    // A spot check that a language loads (not just that its key is listed).
    assert!(language_for("rust").is_some());
    assert!(language_for("haskell").is_some());
    assert!(
        language_for("assembly").is_some(),
        "the asm symbol alias must resolve"
    );
}

#[test]
fn extensions_map_to_languages() {
    assert_eq!(language_for_extension("rs"), Some("rust"));
    assert_eq!(language_for_extension("py"), Some("python"));
    assert_eq!(language_for_extension("ts"), Some("typescript"));
    assert_eq!(language_for_extension("xyz"), None);
}
