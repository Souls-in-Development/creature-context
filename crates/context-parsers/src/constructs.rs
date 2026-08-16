//! The construct registry, ported from creature-clean's `ConstructRegistry`.
//!
//! Its value is a cross-language catalogue: a *shared construct* (canonical
//! "function", "product_type", "behavioral_contract"…) names a concept, and
//! lists the *native constructs* — one language's node type — that participate.
//! Resolving a language's node type yields the shared construct it belongs to,
//! or a native construct when it is unique.
//!
//! Correction applied during the port (spec §9.1 permits this): the Swift source
//! keyed some Rust members by keyword (`fn`, `mod`) and others by tree-sitter
//! node type (`struct_item`). The Rust adapter emits node types, so the actual
//! node types (`function_item`, `mod_item`) are registered here alongside the
//! originals. Full 44-language member coverage lands with the grammars in
//! Task 3; Task 2 covers the categories the Rust adapter emits, with a
//! representative cross-language slice to prove the shared-construct mechanism.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeConstruct {
    pub language: String,
    pub name: String,
}

impl NativeConstruct {
    fn new(language: &str, name: &str) -> Self {
        Self {
            language: language.to_string(),
            name: name.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SharedConstruct {
    pub canonical_name: String,
    pub members: Vec<NativeConstruct>,
}

/// A node type resolved either to the shared construct it participates in
/// (carrying the canonical name) or to a native construct unique to its
/// language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Construct {
    Shared(String),
    Native(NativeConstruct),
}

pub struct ConstructRegistry {
    shared: Vec<SharedConstruct>,
}

impl ConstructRegistry {
    /// Resolve a language's node type to its construct.
    pub fn resolve(&self, language: &str, node_type: &str) -> Construct {
        let native = NativeConstruct::new(language, node_type);
        match self.shared.iter().find(|s| s.members.contains(&native)) {
            Some(shared) => Construct::Shared(shared.canonical_name.clone()),
            None => Construct::Native(native),
        }
    }

    pub fn default_registry() -> Self {
        Self {
            shared: default_shared_constructs(),
        }
    }
}

/// Build the shared-construct catalogue. Each entry: canonical name, and the
/// native `(language, node_type)` members. Ported from `buildDefaultRegistry`
/// and `addExtendedCategories`, scoped to the categories the current adapter
/// emits, with a cross-language slice per category.
fn default_shared_constructs() -> Vec<SharedConstruct> {
    let make = |canonical: &str, members: &[(&str, &str)]| SharedConstruct {
        canonical_name: canonical.to_string(),
        members: members
            .iter()
            .map(|(lang, name)| NativeConstruct::new(lang, name))
            .collect(),
    };

    vec![
        make(
            "function",
            &[
                // Corrected Rust node type alongside the source's keyword form.
                ("rust", "function_item"),
                ("rust", "fn"),
                ("python", "function_definition"),
                ("go", "function_declaration"),
                ("typescript", "function_declaration"),
                ("javascript", "function_declaration"),
                ("c", "function_definition"),
                ("java", "method_declaration"),
                ("swift", "function_declaration"),
            ],
        ),
        make(
            "product_type",
            &[
                ("rust", "struct_item"),
                ("go", "type_declaration"),
                ("c", "struct_specifier"),
                ("cpp", "struct_specifier"),
                ("swift", "struct_declaration"),
                ("csharp", "struct_declaration"),
                ("haskell", "data_declaration"),
            ],
        ),
        make(
            "behavioral_contract",
            &[
                ("rust", "trait_item"),
                ("go", "interface_type"),
                ("java", "interface_declaration"),
                ("typescript", "interface_declaration"),
                ("swift", "protocol_declaration"),
                ("scala", "trait_definition"),
            ],
        ),
        make(
            "enumeration",
            &[
                ("rust", "enum_item"),
                ("swift", "enum_declaration"),
                ("java", "enum_declaration"),
                ("typescript", "enum_declaration"),
                ("c", "enum_specifier"),
            ],
        ),
        make(
            "type_alias",
            &[
                ("rust", "type_item"),
                ("typescript", "type_alias_declaration"),
                ("go", "type_alias"),
                ("swift", "typealias_declaration"),
            ],
        ),
        make(
            "extension",
            &[
                ("rust", "impl_item"),
                ("swift", "extension_declaration"),
                ("csharp", "extension"),
            ],
        ),
        make(
            "module",
            &[
                // Corrected Rust node type alongside the source's keyword form.
                ("rust", "mod_item"),
                ("rust", "mod"),
                ("python", "module"),
                ("go", "package_clause"),
                ("java", "package_declaration"),
            ],
        ),
        make(
            "class",
            &[
                ("java", "class_declaration"),
                ("python", "class_definition"),
                ("cpp", "class_specifier"),
                ("swift", "class_declaration"),
                ("typescript", "class_declaration"),
                ("csharp", "class_declaration"),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{Construct, ConstructRegistry};

    #[test]
    fn a_member_resolves_to_its_shared_construct() {
        let registry = ConstructRegistry::default_registry();
        assert_eq!(
            registry.resolve("rust", "trait_item"),
            Construct::Shared("behavioral_contract".to_string())
        );
    }

    #[test]
    fn an_unmapped_node_stays_native() {
        let registry = ConstructRegistry::default_registry();
        assert!(matches!(
            registry.resolve("rust", "nonsense"),
            Construct::Native(_)
        ));
    }
}
