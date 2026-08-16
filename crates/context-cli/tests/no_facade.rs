//! Mechanical gate against facade implementations.
//!
//! Three failure modes are blocked:
//!   1. A source file that is a lone empty function — scaffolding committed as a feature.
//!   2. A test function containing no assertion — reports success while testing nothing.
//!   3. A test file that never references the product — asserts against values it declared
//!      itself, which an assertion count cannot catch.
//!
//! Known stubs awaiting a later milestone are listed in docs/stub-manifest.txt.
//! That list may shrink. It may never grow.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// CARGO_MANIFEST_DIR is `<workspace>/crates/context-cli`; go up two levels.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root is two levels above the CLI crate")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name == "target" || name == ".git" || name == ".build" {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A file is a stub when every non-comment, non-attribute line is a module
/// declaration or an empty item body.
///
/// A `mod.rs` containing only `pub mod` declarations is deliberately not a stub:
/// it declares structure rather than faking behaviour.
fn is_stub(source: &str) -> bool {
    let mut saw_item = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("pub mod ") || trimmed.starts_with("mod ") {
            continue;
        }
        let empty_item = trimmed.ends_with("{}")
            && (trimmed.contains("fn ")
                || trimmed.contains("struct ")
                || trimmed.contains("enum "));
        if empty_item {
            saw_item = true;
            continue;
        }
        return false;
    }
    saw_item
}

fn read_manifest(root: &Path) -> BTreeSet<String> {
    let path = root.join("docs/stub-manifest.txt");
    let contents =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    contents
        .lines()
        // Strip trailing `# explanation` so entries compare as bare paths.
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn no_undeclared_stub_source_files() {
    let root = workspace_root();
    // Whole-file entries only; `path::function` entries belong to the
    // constant-body gate below.
    let allowed: BTreeSet<String> = read_manifest(&root)
        .into_iter()
        .filter(|e| !e.contains("::"))
        .collect();

    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);

    let mut found = BTreeSet::new();
    for path in &files {
        let rel = relative(&root, path);
        if rel.contains("/tests/") {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        if is_stub(&source) {
            found.insert(rel);
        }
    }

    let new_stubs: Vec<_> = found.difference(&allowed).collect();
    assert!(
        new_stubs.is_empty(),
        "stub source files not declared in docs/stub-manifest.txt:\n{new_stubs:#?}\n\
         A stub is a file whose every item is an empty body. Implement it, or add it to the \
         manifest with a comment naming the milestone that will."
    );

    let stale: Vec<_> = allowed.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "docs/stub-manifest.txt lists files that are no longer stubs — remove these entries:\n{stale:#?}"
    );
}

#[test]
fn every_test_function_asserts() {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);

    let mut offenders = Vec::new();
    for path in &files {
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.trim() != "#[test]" {
                continue;
            }
            let mut depth = 0usize;
            let mut body = String::new();
            let mut started = false;
            for line in lines.iter().skip(i + 1) {
                depth += line.matches('{').count();
                body.push_str(line);
                body.push('\n');
                if depth > 0 {
                    started = true;
                }
                depth = depth.saturating_sub(line.matches('}').count());
                if started && depth == 0 {
                    break;
                }
            }
            let asserts = body.contains("assert!")
                || body.contains("assert_eq!")
                || body.contains("assert_ne!")
                || body.contains("#[should_panic]")
                || body.contains(".expect(")
                || body.contains("panic!");
            if !asserts {
                offenders.push(format!("{}:{}", relative(&root, path), i + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "test functions with no assertion — these report success while testing nothing:\n{offenders:#?}"
    );
}

/// Catches the class an assertion count cannot: a test that asserts against
/// values it declared itself.
///
/// `admission.rs` held five assertions and tested nothing —
/// `let is_stale_rejected = true; assert!(is_stale_rejected);` — while
/// `authority.rs` and `universe.rs` defined mock types in the test file and
/// asserted against those. None imported the crate under test.
///
/// An integration test must touch the product: either a `creature_context*`
/// path, or the binary via `CARGO_BIN_EXE_*`.
#[test]
fn every_test_file_references_the_product() {
    let root = workspace_root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);

    let mut offenders = Vec::new();
    for path in &files {
        let rel = relative(&root, path);
        if !rel.contains("/tests/") {
            continue;
        }
        // Shared helper modules are exempt; they support tests rather than being one.
        if rel.ends_with("/mod.rs") {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        if !source.contains("#[test]") {
            continue;
        }
        if !source.contains("creature_context") && !source.contains("CARGO_BIN_EXE") {
            offenders.push(rel);
        }
    }

    assert!(
        offenders.is_empty(),
        "test files that never reference the product — they assert against values defined \
         inside the test itself:\n{offenders:#?}"
    );
}

/// Every function in `source` whose body is trivially constant, as
/// `(name, description)`.
///
/// Trait method declarations (`fn f(&self) -> T;`) are skipped — they have no
/// body by definition and are not facades.
fn constant_body_functions(source: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let Some(name) = lines[i]
            .split_once("fn ")
            .map(|(_, rest)| {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .filter(|n| !n.is_empty())
        else {
            i += 1;
            continue;
        };

        // Signature terminating in `;` before any `{` is a declaration, not a body.
        let mut probe = i;
        let mut declaration_only = false;
        while probe < lines.len() {
            if lines[probe].contains('{') {
                break;
            }
            if lines[probe].contains(';') {
                declaration_only = true;
                break;
            }
            probe += 1;
        }
        if declaration_only {
            i = probe + 1;
            continue;
        }

        let mut depth = 0usize;
        let mut started = false;
        let mut body = String::new();
        let mut j = i;
        while j < lines.len() {
            depth += lines[j].matches('{').count();
            if depth > 0 {
                started = true;
            }
            depth = depth.saturating_sub(lines[j].matches('}').count());
            if started {
                body.push_str(lines[j]);
                body.push('\n');
                if depth == 0 {
                    break;
                }
            }
            j += 1;
        }

        let inner = match (body.find('{'), body.rfind('}')) {
            (Some(a), Some(b)) if b > a => &body[a + 1..b],
            _ => "",
        };
        let stmts: Vec<&str> = inner
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .collect();

        const CONSTANTS: [&str; 9] = [
            "Ok(())",
            "Ok(String::new())",
            "Ok(vec![])",
            "Ok(Vec::new())",
            "String::new()",
            "Vec::new()",
            "vec![]",
            "None",
            "false",
        ];

        let described = if stmts.is_empty() {
            Some("empty body".to_string())
        } else if stmts.len() == 1 && CONSTANTS.contains(&stmts[0]) {
            Some(format!("returns {}", stmts[0]))
        } else if stmts.len() <= 3
            && stmts.contains(&"Ok(())")
            && stmts
                .iter()
                .all(|s| s.starts_with("println!") || s.starts_with("eprintln!") || *s == "Ok(())")
        {
            Some("prints then returns Ok(())".to_string())
        } else {
            None
        };

        if let Some(description) = described {
            out.push((name, description));
        }
        i = j.max(i + 1);
    }
    out
}

/// Catches a facade that a whole-file check cannot: one no-op function sitting
/// in an otherwise real file.
///
/// `creature-context run` dispatched to `handle_run`, whose entire body was
/// `Ok(())` — the resident service exited zero having done nothing, and a CLI
/// test asserting `status.success()` passed. `permission list` printed a
/// hardcoded `["permission1", "permission2"]`.
///
/// Entries are declared in docs/stub-manifest.txt as `path::function`.
#[test]
fn no_undeclared_constant_body_functions() {
    let root = workspace_root();
    let allowed = read_manifest(&root);

    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);

    let mut found = BTreeSet::new();
    for path in &files {
        let rel = relative(&root, path);
        if rel.contains("/tests/") {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        // Whole-file stubs are already declared by path; don't double-report.
        if is_stub(&source) {
            continue;
        }
        for (name, _) in constant_body_functions(&source) {
            found.insert(format!("{rel}::{name}"));
        }
    }

    let declared: BTreeSet<String> = allowed
        .iter()
        .filter(|e| e.contains("::"))
        .cloned()
        .collect();

    let undeclared: Vec<_> = found.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "constant-body functions not declared in docs/stub-manifest.txt:\n{undeclared:#?}\n\
         A function whose body is only `Ok(())` or a constant does nothing while reporting \
         success. Implement it, or declare it with the milestone that will."
    );

    let stale: Vec<_> = declared.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "docs/stub-manifest.txt declares functions that are no longer constant — \
         remove these entries:\n{stale:#?}"
    );
}
