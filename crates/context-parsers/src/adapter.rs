//! Structural extraction from a parse tree.
//!
//! The traversal is ported from creature-clean's `TreeSitterAdapter.swift`
//! (`traverse` / `declarationName` / `looksLikeDeclaration`): walk the tree,
//! and for each named declaration node capture its name, kind and 1-indexed
//! source span. The Swift glue is not ported — this uses the Rust `tree-sitter`
//! crate over the vendored grammar. Extraction maps onto Creature's own model,
//! not the Swift `TrunkNode`.

pub use crate::constructs::Construct;
use crate::constructs::ConstructRegistry;
use tree_sitter::{Node, Parser};

// The vendored Rust grammar, compiled by build.rs.
unsafe extern "C" {
    fn tree_sitter_rust() -> *const ();
}

/// A canonical declaration kind. Task 2 replaces this with the full construct
/// registry; the skeleton needs only the common shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
    TypeAlias,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-indexed inclusive line range, matching the Swift adapter's `SourceSpan`.
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug)]
pub enum ParseError {
    LanguageLoad,
    Parse,
}

/// Node types that root a whole file — never emitted as declarations.
const ROOT_CONTAINERS: &[&str] = &[
    "source_file",
    "program",
    "translation_unit",
    "compilation_unit",
];

fn kind_for(node_type: &str) -> Option<SymbolKind> {
    // Ported from `looksLikeDeclaration`: family + declaration suffix, plus the
    // Rust grammar's concrete `*_item` node types.
    Some(match node_type {
        "function_item" | "function_signature_item" => SymbolKind::Function,
        "struct_item" => SymbolKind::Struct,
        "enum_item" => SymbolKind::Enum,
        "trait_item" => SymbolKind::Trait,
        "impl_item" => SymbolKind::Impl,
        "mod_item" => SymbolKind::Module,
        "type_item" => SymbolKind::TypeAlias,
        _ => return None,
    })
}

/// Ported from `declarationName`: prefer the `name` field, then recurse into
/// direct identifier children.
fn declaration_name(node: Node, source: &[u8]) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return slice(name, source);
    }
    // A direct identifier child (e.g. Rust `impl Foo`, whose target is a
    // `type_identifier`).
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "type_identifier" | "field_identifier" | "constant"
        ) {
            return slice(child, source);
        }
    }
    // Recurse, as the Swift original does: some grammars nest the name a level
    // down (Go's `type_declaration` → `type_spec` → name). The shallowest name
    // in the subtree is the declaration's own.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = declaration_name(child, source) {
            return Some(name);
        }
    }
    None
}

fn slice(node: Node, source: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(&source[node.byte_range()]).ok()?;
    (!text.is_empty()).then(|| text.to_string())
}

/// Parse Rust source and extract its declarations.
pub fn parse_rust(source: &str) -> Result<Vec<ExtractedSymbol>, ParseError> {
    let language = unsafe { tree_sitter::Language::from_raw(tree_sitter_rust() as *const _) };
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::LanguageLoad)?;

    let tree = parser.parse(source, None).ok_or(ParseError::Parse)?;
    let bytes = source.as_bytes();

    let mut symbols = Vec::new();
    collect(tree.root_node(), bytes, &mut symbols);
    Ok(symbols)
}

/// Parse Rust source and pair each declaration's name with its construct — the
/// cross-language shared construct it participates in, or a native construct
/// when it is unique to Rust.
pub fn parse_rust_typed(source: &str) -> Result<Vec<(String, Construct)>, ParseError> {
    let registry = ConstructRegistry::default_registry();
    let language = unsafe { tree_sitter::Language::from_raw(tree_sitter_rust() as *const _) };
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::LanguageLoad)?;

    let tree = parser.parse(source, None).ok_or(ParseError::Parse)?;
    let bytes = source.as_bytes();

    let mut symbols = Vec::new();
    collect_typed(tree.root_node(), bytes, &registry, &mut symbols);
    Ok(symbols)
}

fn collect_typed(
    node: Node,
    source: &[u8],
    registry: &ConstructRegistry,
    out: &mut Vec<(String, Construct)>,
) {
    if node.is_named() && !ROOT_CONTAINERS.contains(&node.kind()) && kind_for(node.kind()).is_some()
    {
        let name = declaration_name(node, source).unwrap_or_else(|| node.kind().to_string());
        out.push((name, registry.resolve("rust", node.kind())));
    }
    let mut walk = node.walk();
    for child in node.children(&mut walk) {
        collect_typed(child, source, registry, out);
    }
}

/// Whether a node type names a declaration in any language. Ported from
/// creature-clean's `looksLikeDeclaration`: an exact set, or a `family` +
/// `declaration suffix`. This is what lets extraction work across languages
/// without per-language node knowledge.
fn is_declaration(node_type: &str) -> bool {
    const EXACT: &[&str] = &[
        "class",
        "struct",
        "trait",
        "protocol",
        "interface",
        "enum",
        "impl",
        "namespace",
        "module",
        "package",
        "type_declaration",
    ];
    if EXACT.contains(&node_type) {
        return true;
    }
    const FAMILIES: &[&str] = &[
        "function",
        "method",
        "class",
        "struct",
        "trait",
        "protocol",
        "interface",
        "enum",
        "impl",
        "namespace",
        "module",
        "type",
    ];
    const SUFFIXES: &[&str] = &[
        "_declaration",
        "_definition",
        "_item",
        "_specifier",
        "_statement",
        "_signature",
        "_binding",
    ];
    FAMILIES.iter().any(|family| {
        SUFFIXES
            .iter()
            .any(|suffix| node_type == format!("{family}{suffix}"))
    })
}

/// One extracted declaration: its name, the construct it participates in, its
/// 1-indexed source span, and whether it is exported (visible outside its
/// module). An exported declaration becomes a `provides` socket — a shape the
/// entity offers for others to require (spec §6.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSymbol {
    pub name: String,
    pub construct: Construct,
    pub start_line: usize,
    pub end_line: usize,
    pub exported: bool,
}

/// An `import` found in a source file: a shape the file *needs* another entity
/// to provide (spec §6.4). `intra_repo` is true when the path is repo-relative
/// by syntax — a Rust `crate::`/`self::`/`super::` path — so its target is
/// definitionally inside the scanned repository and its absence is a real,
/// adjudicable finding. External imports (`std`, third-party crates) are left
/// out: nothing in the scanned universe could provide them, so asserting a hole
/// about one would fabricate a broken link the repository does not have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedImport {
    /// The path as written, e.g. `crate::widget::Widget`.
    pub path: String,
    /// The leaf item name matched on, e.g. `Widget`.
    pub item: String,
    pub intra_repo: bool,
    pub start_line: usize,
}

/// Whether a declaration node is exported. Rust marks visibility with a
/// `visibility_modifier` child (`pub`, `pub(crate)`, …); its presence is export
/// enough for a `provides` shape. Import/export detection is Rust-first — the
/// self-hosting language, and the one the self-scan exercises — mirroring the
/// walking-skeleton approach that brought up one language end to end first.
fn is_exported(node: Node) -> bool {
    let mut walk = node.walk();
    node.children(&mut walk)
        .any(|child| child.kind() == "visibility_modifier")
}

/// Extract the intra-repository imports of `source`. Rust `use_declaration`
/// paths only, for now (see `is_exported`): every other language returns no
/// imports, so it contributes symbols but no required sockets yet.
pub fn parse_imports(source: &str, language_key: &str) -> Result<Vec<ParsedImport>, ParseError> {
    if language_key != "rust" {
        return Ok(vec![]);
    }
    let language = crate::languages::language_for(language_key).ok_or(ParseError::LanguageLoad)?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::LanguageLoad)?;
    let tree = parser.parse(source, None).ok_or(ParseError::Parse)?;
    let bytes = source.as_bytes();

    let mut out = Vec::new();
    collect_imports(tree.root_node(), bytes, &mut out);
    Ok(out)
}

fn collect_imports(node: Node, source: &[u8], out: &mut Vec<ParsedImport>) {
    if node.kind() == "use_declaration"
        && let Some(argument) = node.child_by_field_name("argument")
        && let Some(text) = slice(argument, source)
    {
        let line = node.start_position().row + 1;
        expand_use_path(&text, line, out);
    }
    let mut walk = node.walk();
    for child in node.children(&mut walk) {
        collect_imports(child, source, out);
    }
}

/// Turn one `use` path's text into the item(s) it imports, expanding nested
/// brace groups (`a::{b::{C, D}, E}`) by balanced parsing — a naive split on
/// `,` or the first `{` mangles nesting. Aliases import the pre-`as` name;
/// globs (`a::*`) and bare module keywords name no item and are skipped. Only
/// paths rooted at `crate`/`self`/`super` are intra-repo; everything else is
/// external and yields nothing.
fn expand_use_path(text: &str, line: usize, out: &mut Vec<ParsedImport>) {
    expand_use(String::new(), text, line, out);
}

fn expand_use(prefix: String, text: &str, line: usize, out: &mut Vec<ParsedImport>) {
    for part in split_top_level(text) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(brace) = part.find('{') {
            let segment = part[..brace].trim().trim_end_matches("::").trim();
            let close = part.rfind('}').unwrap_or(part.len());
            let inner = &part[brace + 1..close];
            expand_use(join_path(&prefix, segment), inner, line, out);
        } else {
            let name = part.split(" as ").next().unwrap_or(part).trim();
            let leaf = name.rsplit("::").next().unwrap_or(name).trim();
            if leaf.is_empty() || leaf == "*" || matches!(leaf, "self" | "super" | "crate") {
                continue;
            }
            let full = join_path(&prefix, name);
            let root = full.split("::").next().unwrap_or("");
            if !matches!(root, "crate" | "self" | "super") {
                continue; // external — out of the scanned universe
            }
            out.push(ParsedImport {
                path: full,
                item: leaf.to_string(),
                intra_repo: true,
                start_line: line,
            });
        }
    }
}

/// Split on commas that are not inside a `{…}` group.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn join_path(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else if segment.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}::{segment}")
    }
}

/// Identifiers that appear as arguments inside a macro invocation, e.g. the
/// `EntityId` in `id_type!(EntityId)`. Tree-sitter cannot see the declarations
/// a macro expands to, so these names would otherwise look undefined; collecting
/// them lets socket resolution tell "generated elsewhere" from "absent" without
/// pretending a macro-defined type is a broken import. Rust only.
pub fn macro_defined_names(source: &str, language_key: &str) -> Result<Vec<String>, ParseError> {
    if language_key != "rust" {
        return Ok(vec![]);
    }
    let language = crate::languages::language_for(language_key).ok_or(ParseError::LanguageLoad)?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::LanguageLoad)?;
    let tree = parser.parse(source, None).ok_or(ParseError::Parse)?;
    let bytes = source.as_bytes();

    let mut out = Vec::new();
    collect_macro_identifiers(tree.root_node(), bytes, false, &mut out);
    Ok(out)
}

fn collect_macro_identifiers(node: Node, source: &[u8], in_macro: bool, out: &mut Vec<String>) {
    let in_macro = in_macro || node.kind() == "token_tree";
    if in_macro
        && matches!(node.kind(), "identifier" | "type_identifier")
        && let Some(name) = slice(node, source)
    {
        out.push(name);
    }
    let mut walk = node.walk();
    for child in node.children(&mut walk) {
        collect_macro_identifiers(child, source, in_macro, out);
    }
}

/// Parse `source` in the given language and extract its declarations with
/// construct and span. The general, cross-language entry point — `parse_rust`
/// is the Rust-specific shortcut kept for the skeleton tests.
pub fn parse(source: &str, language_key: &str) -> Result<Vec<ParsedSymbol>, ParseError> {
    let language = crate::languages::language_for(language_key).ok_or(ParseError::LanguageLoad)?;
    let registry = ConstructRegistry::default_registry();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::LanguageLoad)?;
    let tree = parser.parse(source, None).ok_or(ParseError::Parse)?;
    let bytes = source.as_bytes();

    // Collect from the root's children, never the root itself: a file's top node
    // is the file, and in some grammars it is named like a declaration (Python's
    // root is `module`), which would otherwise be mistaken for one.
    let root = tree.root_node();
    let mut out = Vec::new();
    let mut walk = root.walk();
    for child in root.children(&mut walk) {
        collect_declarations(child, bytes, language_key, &registry, &mut out);
    }
    Ok(out)
}

fn collect_declarations(
    node: Node,
    source: &[u8],
    language_key: &str,
    registry: &ConstructRegistry,
    out: &mut Vec<ParsedSymbol>,
) {
    if node.is_named()
        && !ROOT_CONTAINERS.contains(&node.kind())
        && is_declaration(node.kind())
        && let Some(name) = declaration_name(node, source)
    {
        out.push(ParsedSymbol {
            name,
            construct: registry.resolve(language_key, node.kind()),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            exported: is_exported(node),
        });
    }
    let mut walk = node.walk();
    for child in node.children(&mut walk) {
        collect_declarations(child, source, language_key, registry, out);
    }
}

fn collect(node: Node, source: &[u8], out: &mut Vec<ExtractedSymbol>) {
    if node.is_named()
        && !ROOT_CONTAINERS.contains(&node.kind())
        && let Some(kind) = kind_for(node.kind())
    {
        let name = declaration_name(node, source).unwrap_or_else(|| node.kind().to_string());
        out.push(ExtractedSymbol {
            name,
            kind,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
        });
    }
    let mut walk = node.walk();
    for child in node.children(&mut walk) {
        collect(child, source, out);
    }
}
