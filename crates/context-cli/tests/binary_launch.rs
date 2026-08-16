//! The built binary must launch on its own, without cargo's help.
//!
//! Every other CLI test spawns `CARGO_BIN_EXE_creature-context` from inside a
//! test process that cargo itself launched, and cargo injects the build script's
//! `rustc-link-search` directories into the dynamic-library search path. The
//! spawned binary inherits that, so a natively-linked library resolves even when
//! the binary carries no runtime search path of its own.
//!
//! A user running the same binary from a shell — or a daemon started by the
//! system — inherits nothing. This test removes that prop so the failure lands
//! here instead of in the user's terminal. It caught exactly that: the Apple
//! Foundation bridge emitted its rpath for test targets only, so the shipped
//! binary died with `Library not loaded: @rpath/libccfoundation.dylib — no
//! LC_RPATH's found` before parsing a single argument, while the suite stayed
//! green.

use std::process::Command;

/// `--help` is the cheapest possible launch: it links and loads everything the
/// binary needs, then exits 0 without touching a project. If the dynamic linker
/// cannot resolve a library, the process dies before clap ever runs.
#[test]
fn the_built_binary_launches_without_cargos_library_path() {
    let binary = env!("CARGO_BIN_EXE_creature-context");
    let output = Command::new(binary)
        .arg("--help")
        // Strip the loader hints cargo hands down, on every platform's spelling,
        // so this asserts what a user's shell actually provides: nothing.
        .env_remove("DYLD_FALLBACK_LIBRARY_PATH")
        .env_remove("DYLD_LIBRARY_PATH")
        .env_remove("LD_LIBRARY_PATH")
        .output()
        .expect("spawn the built binary");

    assert!(
        output.status.success(),
        "the built binary does not launch outside cargo — a user running it from \
         a shell gets this:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
