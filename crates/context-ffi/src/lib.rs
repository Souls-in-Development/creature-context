//! Core C ABI over the creature-context engine — the outward "port" that lets
//! native apps (macOS/iOS Swift, Android via JNI, Windows/Linux C) embed the
//! engine directly instead of shelling out to the CLI.
//!
//! Every entry point is `extern "C"`, panic-guarded (a panic must never unwind
//! across the FFI boundary), and returns either a status code (`0` = ok,
//! negative = error) or a heap-allocated C string the caller frees with
//! `cc_string_free`. The C header is `include/creature_context.h`.
//!
//! This is the engine-facing port (creature-context → native). The other
//! direction — native on-device model bridges the core calls out to — lives in
//! `creature-context-model`'s platform adapters (`cc_foundation_*` on Apple,
//! `cc_aicore_*` on Android, …).

use std::ffi::{CStr, CString, c_char};
use std::panic::catch_unwind;
use std::path::Path;

use creature_context_core::project::{ProjectPaths, atomic_write, init_project, load_identity};
use creature_context_store::{AtlasRepository, texture, write_projections};

/// Borrow a UTF-8 path from a C string; `None` on null or invalid UTF-8.
///
/// # Safety
/// `p` must be null or a valid, NUL-terminated C string that outlives the borrow.
unsafe fn as_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

fn init_impl(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    init_project(root)?;
    Ok(())
}

fn scan_impl(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let paths = ProjectPaths::new(root);
    if let Some(dir) = paths.database.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut repository = AtlasRepository::open(&paths.database)?;
    let previous = repository.load_snapshot().ok();
    let snapshot = creature_context_parsers::index::index_project(root, previous.as_ref())?;
    repository.replace_snapshot(&snapshot)?;
    let project_id = load_identity(root)?.project_id;
    write_projections(root, &snapshot, &project_id)?;
    Ok(())
}

fn map_impl(root: &Path, galaxy: bool) -> Result<usize, Box<dyn std::error::Error>> {
    let paths = ProjectPaths::new(root);
    let repository = AtlasRepository::open(&paths.database)?;
    let (png, count) = if galaxy {
        let nodes = repository.entity_tree_nodes()?;
        let edges = repository.entity_edges()?;
        let support = repository.support_entity_ids()?;
        let docs = repository.doc_entity_ids()?;
        let ages = repository.entity_ages(&paths.root)?;
        let count = nodes.len();
        (
            texture::force::galaxy_png(&nodes, &edges, &support, &docs, &ages, 1024),
            count,
        )
    } else {
        let rows = repository.entity_green_codes()?;
        let count = rows.len();
        let codes = texture::order_codes(rows);
        let (rgba, side) = texture::render_square(&codes);
        (texture::png::encode_rgba_png(side, side, &rgba), count)
    };
    atomic_write(&paths.root.join("ATLAS.png"), &png)?;
    Ok(count)
}

fn status_impl(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let repository = AtlasRepository::open(&ProjectPaths::new(root).database)?;
    let (entities, edges) = repository.counts()?;
    let snapshot = repository
        .current_snapshot_id()?
        .map(|s| s.0)
        .unwrap_or_default();
    Ok(format!(
        "{{\"snapshot\":\"{snapshot}\",\"entities\":{entities},\"edges\":{edges}}}"
    ))
}

/// Initialize a project's identity at `root`. `0` = ok, negative = error.
#[unsafe(no_mangle)]
pub extern "C" fn cc_init(root: *const c_char) -> i32 {
    catch_unwind(|| match unsafe { as_str(root) } {
        Some(root) => match init_impl(Path::new(root)) {
            Ok(()) => 0,
            Err(_) => -2,
        },
        None => -1,
    })
    .unwrap_or(-99)
}

/// Index `root` and persist the Atlas (store + portable `ATLAS.idx`). `0` = ok.
#[unsafe(no_mangle)]
pub extern "C" fn cc_scan(root: *const c_char) -> i32 {
    catch_unwind(|| match unsafe { as_str(root) } {
        Some(root) => match scan_impl(Path::new(root)) {
            Ok(()) => 0,
            Err(_) => -2,
        },
        None => -1,
    })
    .unwrap_or(-99)
}

/// Render `ATLAS.png`. `galaxy` selects the circle-packed layout (else the
/// Hilbert square). Returns the entity count (`>= 0`) or a negative error code.
#[unsafe(no_mangle)]
pub extern "C" fn cc_map(root: *const c_char, galaxy: bool) -> i64 {
    catch_unwind(|| match unsafe { as_str(root) } {
        Some(root) => match map_impl(Path::new(root), galaxy) {
            Ok(n) => n as i64,
            Err(_) => -2,
        },
        None => -1,
    })
    .unwrap_or(-99)
}

/// Status JSON (`{"snapshot","entities","edges"}`) as a heap C string the caller
/// frees with `cc_string_free`, or NULL on error.
#[unsafe(no_mangle)]
pub extern "C" fn cc_status(root: *const c_char) -> *mut c_char {
    catch_unwind(|| {
        let Some(root) = (unsafe { as_str(root) }) else {
            return std::ptr::null_mut();
        };
        match status_impl(Path::new(root))
            .ok()
            .and_then(|s| CString::new(s).ok())
        {
            Some(cs) => cs.into_raw(),
            None => std::ptr::null_mut(),
        }
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Free a string returned by this library (e.g. from `cc_status`).
///
/// # Safety
/// `ptr` must be null or a pointer returned by this library and not yet freed.
#[unsafe(no_mangle)]
pub extern "C" fn cc_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

/// JNI surface: the same engine port for the JVM / Android. These wrap the
/// `*_impl` functions for a `context.CreatureContext` Kotlin object (see
/// `platform/android/.../CreatureContext.kt`). They compile on any host — the
/// `jni` crate is host-agnostic — but only *run* inside a JVM that has loaded the
/// `creature_context_ffi` shared library.
mod jni_exports {
    use super::{init_impl, map_impl, scan_impl, status_impl};
    use jni::JNIEnv;
    use jni::objects::{JClass, JString};
    use jni::sys::{jboolean, jint, jlong, jstring};
    use std::path::Path;

    fn arg(env: &mut JNIEnv, root: &JString) -> Option<String> {
        env.get_string(root).ok().map(|s| s.into())
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_context_CreatureContext_init(
        mut env: JNIEnv,
        _class: JClass,
        root: JString,
    ) -> jint {
        match arg(&mut env, &root) {
            Some(root) => match init_impl(Path::new(&root)) {
                Ok(()) => 0,
                Err(_) => -2,
            },
            None => -1,
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_context_CreatureContext_scan(
        mut env: JNIEnv,
        _class: JClass,
        root: JString,
    ) -> jint {
        match arg(&mut env, &root) {
            Some(root) => match scan_impl(Path::new(&root)) {
                Ok(()) => 0,
                Err(_) => -2,
            },
            None => -1,
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_context_CreatureContext_map(
        mut env: JNIEnv,
        _class: JClass,
        root: JString,
        galaxy: jboolean,
    ) -> jlong {
        match arg(&mut env, &root) {
            Some(root) => match map_impl(Path::new(&root), galaxy != 0) {
                Ok(n) => n as jlong,
                Err(_) => -2,
            },
            None => -1,
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_context_CreatureContext_status(
        mut env: JNIEnv,
        _class: JClass,
        root: JString,
    ) -> jstring {
        let Some(root) = arg(&mut env, &root) else {
            return std::ptr::null_mut();
        };
        match status_impl(Path::new(&root)) {
            Ok(text) => env
                .new_string(text)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            Err(_) => std::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_scan_map_status_round_trip() {
        let dir = std::env::temp_dir().join(format!("cc-ffi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("PURPOSE.md"), "# F\n\n## Goals\n- x\n").unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn a() {}\n").unwrap();

        let c = CString::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(cc_init(c.as_ptr()), 0, "init");
        assert_eq!(cc_scan(c.as_ptr()), 0, "scan");
        assert!(cc_map(c.as_ptr(), true) >= 1, "map returns entity count");
        assert!(dir.join("ATLAS.png").exists(), "ATLAS.png written");

        let s = cc_status(c.as_ptr());
        assert!(!s.is_null(), "status non-null");
        let json = unsafe { CStr::from_ptr(s) }.to_str().unwrap().to_string();
        cc_string_free(s);
        assert!(json.contains("\"entities\""), "status json: {json}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
