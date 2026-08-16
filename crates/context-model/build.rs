//! Build the Apple Foundation Models C-ABI bridge (specification 8, 16).
//!
//! On macOS with the Swift toolchain and the FoundationModels framework present,
//! this compiles `platform/apple/.../CBridge.swift` into a dylib and links it,
//! enabling the `foundation_bridge` cfg. Anywhere else — a non-macOS host, no
//! Swift, no framework — it does nothing, and the adapter compiles to its
//! honest fallback that reports the capability as unavailable. The bridge is
//! never assumed; it is built only where it can actually run.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Declare the bridge cfgs so `-D warnings` does not flag them as unexpected.
    // `foundation_bridge` is set below when this build compiles the Swift shim on
    // macOS. The other two name the same seam on their platforms — Windows Phi
    // Silica (`phi_silica_bridge`) and Android AICore (`aicore_bridge`) — but
    // their native shims are built and linked by that platform's own app build
    // (the Windows App SDK project; the Android module's Gradle), which sets the
    // cfg there. Declaring them here keeps the adapters' `#[cfg(...)]` seams warning
    // -clean everywhere while cargo builds no cross-platform native code it cannot
    // verify on this host.
    println!("cargo::rustc-check-cfg=cfg(foundation_bridge)");
    println!("cargo::rustc-check-cfg=cfg(phi_silica_bridge)");
    println!("cargo::rustc-check-cfg=cfg(aicore_bridge)");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let bridge = manifest
        .join("../../platform/apple/Sources/CreatureContextFoundation/CBridge.swift")
        .canonicalize()
        .ok();
    let framework = std::path::Path::new("/System/Library/Frameworks/FoundationModels.framework");
    let have_swiftc = Command::new("swiftc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let Some(bridge) = bridge else { return };
    if !have_swiftc || !framework.exists() {
        return; // honest degradation: no bridge, the adapter reports Unavailable
    }

    println!("cargo:rerun-if-changed={}", bridge.display());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dylib = out.join("libccfoundation.dylib");

    let status = Command::new("swiftc")
        .args(["-emit-library", "-o"])
        .arg(&dylib)
        .arg(&bridge)
        .args(["-framework", "FoundationModels"])
        // Record where the dylib actually is, as an absolute path, so *anything*
        // that links it resolves it with no runtime search path of its own.
        //
        // The previous `@rpath/...` install_name needed every consumer to carry a
        // matching rpath, and a build script can only add link args to its own
        // package's targets. `creature-context`'s binary lives in another crate
        // (and depends on this one only transitively, through context-runtime),
        // so it could never receive one: the shipped binary died with
        // "no LC_RPATH's found" before parsing an argument. It appeared to work
        // only because cargo injects this directory into the loader's search path
        // for processes *it* launches, which propped up every test.
        // `tests/binary_launch.rs` now strips that prop.
        .args([
            "-Xlinker",
            "-install_name",
            "-Xlinker",
            dylib.to_str().expect("dylib path is valid UTF-8"),
        ])
        .status()
        .expect("invoke swiftc");
    assert!(
        status.success(),
        "swiftc failed to build the Foundation bridge"
    );

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=dylib=ccfoundation");
    // No rpath directive is needed: the absolute install_name above tells the
    // loader where the dylib is, for every consumer in every crate.
    //
    // The trade-off is honest: the binary is bound to this build tree. Packaging
    // a relocatable artefact means shipping the dylib alongside it and rewriting
    // the install_name (`install_name_tool -change`) to `@executable_path/...` —
    // a distribution step this repository does not yet have.
    println!("cargo:rustc-cfg=foundation_bridge");
}
