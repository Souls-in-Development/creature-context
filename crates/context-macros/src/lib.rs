extern crate proc_macro;

use proc_macro::TokenStream;
use std::env;
use std::fs;
use std::path::PathBuf;

/// `#[context_enforce]`
///
/// Verifies the compiling crate against the declared-plane architectural rules
/// in the project's `ATLAS.idx`. A declared, required `conflicts` edge is a
/// human-authored prohibition; if an observed dependency breaches it, the build
/// halts with a descriptive error.
///
/// Observed and inferred evidence, including all Green assessment, never gates
/// compilation. Otherwise a stale or Red scan could make the toolchain
/// unbuildable. Set `CREATURE_CONTEXT_NO_ENFORCE=1` to disable enforcement.
#[proc_macro_attribute]
pub fn context_enforce(_attr: TokenStream, item: TokenStream) -> TokenStream {
    if env::var_os("CREATURE_CONTEXT_NO_ENFORCE").is_some() {
        return item;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let mut current_dir = PathBuf::from(manifest_dir);

    // Walk upward to the project root, identified by the presence of a
    // `.creature` directory. The root ATLAS.idx holds project-wide declared
    // rules; per-folder ATLAS.idx files are local scope and must not be used
    // for build-time enforcement.
    let mut atlas_path = None;
    loop {
        if current_dir.join(".creature").is_dir() {
            let root_candidate = current_dir.join("ATLAS.idx");
            if root_candidate.exists() {
                atlas_path = Some(root_candidate);
                break;
            }
            let hidden_candidate = current_dir.join(".creature").join("ATLAS.idx");
            if hidden_candidate.exists() {
                atlas_path = Some(hidden_candidate);
                break;
            }
        }
        if !current_dir.pop() {
            break;
        }
    }

    let path = match atlas_path {
        Some(p) => p,
        None => return item,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return item,
    };

    let decoded = match creature_context_store::decode_atlas_idx(&content) {
        Ok(d) => d,
        Err(_) => return item,
    };

    let violations = creature_context_types::rules::violations(&decoded.snapshot);
    if violations.is_empty() {
        return item;
    }

    let message = violations.join("\n");
    let error = syn::Error::new(proc_macro2::Span::call_site(), message);
    error.to_compile_error().into()
}
