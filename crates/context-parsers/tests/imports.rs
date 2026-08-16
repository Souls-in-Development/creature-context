//! Milestone 4 Task 5: import and export extraction feeding sockets.
//!
//! Exports become `provides` shapes and intra-repo imports become `requires`
//! shapes (spec §6.4). External imports are deliberately not extracted: nothing
//! in the scanned repository could provide `std::path::Path`, so treating it as
//! a required socket would fabricate a hole the code does not have.

use creature_context_parsers::adapter::{parse, parse_imports};

#[test]
fn an_intra_crate_import_is_extracted_with_its_item_name() {
    let imports = parse_imports("use crate::widget::Widget;\n", "rust").expect("parse");
    assert_eq!(imports.len(), 1, "one import: {imports:?}");
    assert_eq!(imports[0].item, "Widget");
    assert!(imports[0].intra_repo);
    assert_eq!(imports[0].path, "crate::widget::Widget");
}

#[test]
fn an_external_import_is_not_a_required_socket() {
    let imports = parse_imports("use std::path::Path;\n", "rust").expect("parse");
    assert!(
        imports.is_empty(),
        "std is outside the scanned universe: {imports:?}"
    );
}

#[test]
fn a_brace_group_expands_to_one_import_per_item() {
    let imports = parse_imports("use crate::model::{Entity, Edge};\n", "rust").expect("parse");
    let items: Vec<&str> = imports.iter().map(|i| i.item.as_str()).collect();
    assert!(items.contains(&"Entity"), "{items:?}");
    assert!(items.contains(&"Edge"), "{items:?}");
}

#[test]
fn a_nested_brace_group_expands_without_mangling_names() {
    // The self-scan surfaced this: `use crate::{ a::{B, C}, D };` must yield the
    // items B, C, D — not `{B` and `C}` from a naive single-level split.
    let imports = parse_imports(
        "use crate::{output::{OutputFormat, write_output}, EntityId};\n",
        "rust",
    )
    .expect("parse");
    let items: Vec<&str> = imports.iter().map(|i| i.item.as_str()).collect();
    assert!(items.contains(&"OutputFormat"), "{items:?}");
    assert!(items.contains(&"write_output"), "{items:?}");
    assert!(items.contains(&"EntityId"), "{items:?}");
    assert!(
        imports
            .iter()
            .all(|i| !i.item.contains('{') && !i.item.contains('}')),
        "no braces may leak into item names: {imports:?}"
    );
    // Nested prefixes are carried: the OutputFormat import is under output.
    assert!(
        imports
            .iter()
            .any(|i| i.path == "crate::output::OutputFormat"),
        "nested prefix preserved: {imports:?}"
    );
}

#[test]
fn a_glob_import_names_no_item_and_is_skipped() {
    let imports = parse_imports("use crate::prelude::*;\n", "rust").expect("parse");
    assert!(imports.is_empty(), "a glob names no item: {imports:?}");
}

#[test]
fn a_pub_declaration_is_exported_and_a_bare_one_is_not() {
    let symbols = parse("pub struct Shown {}\nstruct Hidden {}\n", "rust").expect("parse");
    let shown = symbols.iter().find(|s| s.name == "Shown").expect("Shown");
    let hidden = symbols.iter().find(|s| s.name == "Hidden").expect("Hidden");
    assert!(shown.exported, "pub struct is a provides");
    assert!(!hidden.exported, "a private struct exposes no shape");
}
