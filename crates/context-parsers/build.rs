//! Compile the vendored Tree-sitter grammar C sources.
//!
//! Grammars are host-agnostic C copied from creature-clean (see
//! provenance.json). Each grammar directory has a `parser.c` and, when the
//! grammar needs one, a `scanner.c`; each is compiled as its own static library
//! with its own bundled `tree_sitter/` headers on the include path.
//!
//! A grammar on `DENYLIST` is vendored but not compiled — used when a specific
//! grammar will not build cleanly, so the rest still link (spec §17: degrade
//! explicitly, never fail the whole set for one bad grammar).

use std::path::Path;

/// Grammars present in `vendor/` but excluded from compilation, each with a
/// reason. Populated only when a grammar is found not to build.
const DENYLIST: &[(&str, &str)] = &[];

fn main() {
    let vendor = Path::new("vendor");
    let Ok(entries) = std::fs::read_dir(vendor) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("tree_sitter_") || !path.is_dir() {
            continue;
        }
        if DENYLIST.iter().any(|(denied, _)| *denied == name) {
            continue;
        }
        compile_grammar(&path, name);
    }
}

fn compile_grammar(dir: &Path, name: &str) {
    let mut build = cc::Build::new();
    build.include(dir).warnings(false).flag_if_supported("-w");

    let parser = dir.join("parser.c");
    if !parser.exists() {
        return;
    }
    println!("cargo:rerun-if-changed={}", parser.display());
    build.file(&parser);

    let scanner = dir.join("scanner.c");
    if scanner.exists() {
        println!("cargo:rerun-if-changed={}", scanner.display());
        build.file(&scanner);
    }

    build.compile(name);
}
