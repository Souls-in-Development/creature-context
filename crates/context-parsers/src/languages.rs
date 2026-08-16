//! Language dispatch for the vendored grammars.
//!
//! Generated from the vendored grammars' real `tree_sitter_*` symbols and a
//! curated extension map (ported from creature-clean's `language(for:)`).
//! Every grammar in `vendor/` is compiled by build.rs; this maps a language
//! key and a file extension to its loader.

use tree_sitter::Language;

unsafe extern "C" {
    fn tree_sitter_ada() -> *const ();
    fn tree_sitter_asm() -> *const ();
    fn tree_sitter_bash() -> *const ();
    fn tree_sitter_c() -> *const ();
    fn tree_sitter_clojure() -> *const ();
    fn tree_sitter_cpp() -> *const ();
    fn tree_sitter_crystal() -> *const ();
    fn tree_sitter_c_sharp() -> *const ();
    fn tree_sitter_d() -> *const ();
    fn tree_sitter_dart() -> *const ();
    fn tree_sitter_elixir() -> *const ();
    fn tree_sitter_erlang() -> *const ();
    fn tree_sitter_fortran() -> *const ();
    fn tree_sitter_fsharp() -> *const ();
    fn tree_sitter_go() -> *const ();
    fn tree_sitter_groovy() -> *const ();
    fn tree_sitter_haskell() -> *const ();
    fn tree_sitter_java() -> *const ();
    fn tree_sitter_javascript() -> *const ();
    fn tree_sitter_julia() -> *const ();
    fn tree_sitter_kotlin() -> *const ();
    fn tree_sitter_commonlisp() -> *const ();
    fn tree_sitter_lua() -> *const ();
    fn tree_sitter_matlab() -> *const ();
    fn tree_sitter_nim() -> *const ();
    fn tree_sitter_objc() -> *const ();
    fn tree_sitter_ocaml() -> *const ();
    fn tree_sitter_pascal() -> *const ();
    fn tree_sitter_perl() -> *const ();
    fn tree_sitter_php() -> *const ();
    fn tree_sitter_powershell() -> *const ();
    fn tree_sitter_python() -> *const ();
    fn tree_sitter_r() -> *const ();
    fn tree_sitter_ruby() -> *const ();
    fn tree_sitter_rust() -> *const ();
    fn tree_sitter_scala() -> *const ();
    fn tree_sitter_scheme() -> *const ();
    fn tree_sitter_solidity() -> *const ();
    fn tree_sitter_sql() -> *const ();
    fn tree_sitter_swift() -> *const ();
    fn tree_sitter_typescript() -> *const ();
    fn tree_sitter_vb_dotnet() -> *const ();
    fn tree_sitter_zig() -> *const ();
}

/// Load a language by its key (e.g. "rust", "python").
pub fn language_for(key: &str) -> Option<Language> {
    let raw: *const () = match key {
        "ada" => unsafe { tree_sitter_ada() },
        "assembly" => unsafe { tree_sitter_asm() },
        "bash" => unsafe { tree_sitter_bash() },
        "c" => unsafe { tree_sitter_c() },
        "clojure" => unsafe { tree_sitter_clojure() },
        "cpp" => unsafe { tree_sitter_cpp() },
        "crystal" => unsafe { tree_sitter_crystal() },
        "csharp" => unsafe { tree_sitter_c_sharp() },
        "d" => unsafe { tree_sitter_d() },
        "dart" => unsafe { tree_sitter_dart() },
        "elixir" => unsafe { tree_sitter_elixir() },
        "erlang" => unsafe { tree_sitter_erlang() },
        "fortran" => unsafe { tree_sitter_fortran() },
        "fsharp" => unsafe { tree_sitter_fsharp() },
        "go" => unsafe { tree_sitter_go() },
        "groovy" => unsafe { tree_sitter_groovy() },
        "haskell" => unsafe { tree_sitter_haskell() },
        "java" => unsafe { tree_sitter_java() },
        "javascript" => unsafe { tree_sitter_javascript() },
        "julia" => unsafe { tree_sitter_julia() },
        "kotlin" => unsafe { tree_sitter_kotlin() },
        "lisp" => unsafe { tree_sitter_commonlisp() },
        "lua" => unsafe { tree_sitter_lua() },
        "matlab" => unsafe { tree_sitter_matlab() },
        "nim" => unsafe { tree_sitter_nim() },
        "objc" => unsafe { tree_sitter_objc() },
        "ocaml" => unsafe { tree_sitter_ocaml() },
        "pascal" => unsafe { tree_sitter_pascal() },
        "perl" => unsafe { tree_sitter_perl() },
        "php" => unsafe { tree_sitter_php() },
        "powershell" => unsafe { tree_sitter_powershell() },
        "python" => unsafe { tree_sitter_python() },
        "r" => unsafe { tree_sitter_r() },
        "ruby" => unsafe { tree_sitter_ruby() },
        "rust" => unsafe { tree_sitter_rust() },
        "scala" => unsafe { tree_sitter_scala() },
        "scheme" => unsafe { tree_sitter_scheme() },
        "solidity" => unsafe { tree_sitter_solidity() },
        "sql" => unsafe { tree_sitter_sql() },
        "swift" => unsafe { tree_sitter_swift() },
        "typescript" => unsafe { tree_sitter_typescript() },
        "vbnet" => unsafe { tree_sitter_vb_dotnet() },
        "zig" => unsafe { tree_sitter_zig() },
        _ => return None,
    };
    Some(unsafe { Language::from_raw(raw as *const _) })
}

/// Map a file extension (without the dot) to a language key.
pub fn language_for_extension(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "adb" => "ada",
        "ads" => "ada",
        "asm" => "assembly",
        "s" => "assembly",
        "sh" => "bash",
        "bash" => "bash",
        "c" => "c",
        "h" => "c",
        "clj" => "clojure",
        "cljs" => "clojure",
        "cpp" => "cpp",
        "cc" => "cpp",
        "cxx" => "cpp",
        "hpp" => "cpp",
        "hh" => "cpp",
        "cr" => "crystal",
        "cs" => "csharp",
        "d" => "d",
        "dart" => "dart",
        "ex" => "elixir",
        "exs" => "elixir",
        "erl" => "erlang",
        "f90" => "fortran",
        "f95" => "fortran",
        "f" => "fortran",
        "fs" => "fsharp",
        "fsx" => "fsharp",
        "go" => "go",
        "groovy" => "groovy",
        "hs" => "haskell",
        "java" => "java",
        "js" => "javascript",
        "jsx" => "javascript",
        "mjs" => "javascript",
        "jl" => "julia",
        "kt" => "kotlin",
        "kts" => "kotlin",
        "lisp" => "lisp",
        "lsp" => "lisp",
        "lua" => "lua",
        "m" => "matlab",
        "nim" => "nim",
        "mm" => "objc",
        "ml" => "ocaml",
        "mli" => "ocaml",
        "pas" => "pascal",
        "pl" => "perl",
        "pm" => "perl",
        "php" => "php",
        "ps1" => "powershell",
        "py" => "python",
        "r" => "r",
        "rb" => "ruby",
        "rs" => "rust",
        "scala" => "scala",
        "sc" => "scala",
        "scm" => "scheme",
        "sol" => "solidity",
        "sql" => "sql",
        "swift" => "swift",
        "ts" => "typescript",
        "tsx" => "typescript",
        "vb" => "vbnet",
        "zig" => "zig",
        _ => return None,
    })
}

/// Every language key with a compiled grammar.
pub fn supported_languages() -> &'static [&'static str] {
    &[
        "ada",
        "assembly",
        "bash",
        "c",
        "clojure",
        "cpp",
        "crystal",
        "csharp",
        "d",
        "dart",
        "elixir",
        "erlang",
        "fortran",
        "fsharp",
        "go",
        "groovy",
        "haskell",
        "java",
        "javascript",
        "julia",
        "kotlin",
        "lisp",
        "lua",
        "matlab",
        "nim",
        "objc",
        "ocaml",
        "pascal",
        "perl",
        "php",
        "powershell",
        "python",
        "r",
        "ruby",
        "rust",
        "scala",
        "scheme",
        "solidity",
        "sql",
        "swift",
        "typescript",
        "vbnet",
        "zig",
    ]
}
