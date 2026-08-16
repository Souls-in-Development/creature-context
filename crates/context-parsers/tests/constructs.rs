//! Milestone 4 Task 2: extraction is construct-typed.
//!
//! Ported from creature-clean's `ConstructRegistry` and its tests. A node type
//! that participates in a cross-language shared construct resolves to that
//! shared construct (so a Rust `struct_item` and a Go `type_declaration` are
//! both recognised as `product_type`); an unrecognised node type stays native.

use creature_context_parsers::adapter::{Construct, parse_rust_typed};
use creature_context_parsers::constructs::ConstructRegistry;

const SOURCE: &str = r#"
pub struct Widget { pub id: u32 }
fn make() -> Widget { Widget { id: 0 } }
trait Render { fn render(&self); }
enum Colour { Red, Green }
"#;

fn canonical(symbols: &[(String, Construct)], name: &str) -> Option<String> {
    symbols
        .iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, c)| match c {
            Construct::Shared(canonical) => Some(canonical.clone()),
            Construct::Native(_) => None,
        })
}

#[test]
fn rust_declarations_resolve_to_their_shared_constructs() {
    let symbols = parse_rust_typed(SOURCE).expect("parse");

    assert_eq!(canonical(&symbols, "make").as_deref(), Some("function"));
    assert_eq!(
        canonical(&symbols, "Widget").as_deref(),
        Some("product_type")
    );
    assert_eq!(
        canonical(&symbols, "Render").as_deref(),
        Some("behavioral_contract")
    );
    assert_eq!(
        canonical(&symbols, "Colour").as_deref(),
        Some("enumeration")
    );
}

#[test]
fn a_shared_construct_spans_languages() {
    // The reuse value: the same canonical construct is reachable from unrelated
    // languages' native names.
    let registry = ConstructRegistry::default_registry();
    let rust = registry.resolve("rust", "struct_item");
    let go = registry.resolve("go", "type_declaration");

    match (rust, go) {
        (Construct::Shared(a), Construct::Shared(b)) => {
            assert_eq!(a, "product_type");
            assert_eq!(b, "product_type");
        }
        other => panic!("both should resolve to the same shared construct, got {other:?}"),
    }
}

#[test]
fn an_unrecognised_node_type_stays_native() {
    let registry = ConstructRegistry::default_registry();
    match registry.resolve("rust", "some_unmapped_node") {
        Construct::Native(n) => {
            assert_eq!(n.language, "rust");
            assert_eq!(n.name, "some_unmapped_node");
        }
        Construct::Shared(s) => panic!("an unmapped node must stay native, got shared {s}"),
    }
}
