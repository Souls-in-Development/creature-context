package context

/**
 * Android/JVM binding to the creature-context engine — the outward "port".
 *
 * These are the Kotlin `external` declarations for the JNI symbols exported by
 * the Rust `creature-context-ffi` cdylib
 * (`Java_context_CreatureContext_*` in `crates/context-ffi/src/lib.rs`). Load
 * the shared library once, then call the engine directly instead of shelling out
 * to the CLI.
 *
 * ── Honesty boundary ────────────────────────────────────────────────────────
 * The **Rust** side of this binding is compiled and symbol-verified on the
 * development host (macOS): the `Java_context_CreatureContext_*` exports are
 * present in the cdylib. This **Kotlin** side has NOT been compiled or run — the
 * host has no Android/Gradle toolchain. It is a faithful, matching declaration,
 * not a verified component. Before trusting it on device, confirm:
 *   1. The cdylib is built for the target ABI (`aarch64-linux-android`, …) and
 *      packaged as `libcreature_context_ffi.so` under `jniLibs/<abi>/`.
 *   2. `System.loadLibrary("creature_context_ffi")` resolves at runtime.
 *   3. The method signatures below match the JNI exports exactly (they do as of
 *      this commit; keep them in lockstep).
 *
 * Return conventions mirror the C ABI: `0` / a non-negative count on success,
 * negative on error; `status` returns null on error.
 */
object CreatureContext {
    init {
        System.loadLibrary("creature_context_ffi")
    }

    /** Initialize a project's identity at [root] (once, before scanning). */
    external fun init(root: String): Int

    /** Index [root] and persist the Atlas (cache + portable ATLAS.idx). */
    external fun scan(root: String): Int

    /** Render <root>/ATLAS.png; [galaxy] selects the circle-packed layout.
     *  Returns the entity count, or negative on error. */
    external fun map(root: String, galaxy: Boolean): Long

    /** Status JSON {"snapshot","entities","edges"}, or null on error. */
    external fun status(root: String): String?
}
