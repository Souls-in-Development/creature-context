package context

/**
 * On-device producer for Android: **Gemini Nano**, served by the **AICore** system
 * service and reached through the **ML Kit GenAI** APIs. This is the Kotlin body
 * behind the Rust `ModelPartner` adapter in
 * `crates/context-model/src/platform/android.rs` (specification 8, 16).
 *
 * Unlike Apple (a C ABI over Swift) and Windows (a C ABI over C++/WinRT), AICore is
 * a Kotlin/Java surface, so the producer lives here and a thin JNI layer exports
 * the `cc_aicore_*` C symbols the Rust `bridge` module binds to:
 *
 * ```
 * cc_aicore_availability() -> availableBlocking() ? 1 : 0
 * cc_aicore_summarize(utf8) -> summarizeBlocking(prompt)   // null → propose nothing
 * cc_aicore_free(ptr)       -> frees the JNI-owned C string
 * ```
 *
 * ── Honesty boundary ────────────────────────────────────────────────────────
 * This file is written against the documented ML Kit GenAI surface but has NOT
 * been compiled or run: the development host is macOS with no Android toolchain,
 * no AICore, and no Gemini Nano. It is a faithful starting point for an on-device
 * build, not a verified component. Two things must be settled on real hardware
 * before it is trusted:
 *   1. Exact ML Kit GenAI class/method names against the pinned SDK version. The
 *      GenAI APIs are young — Summarization/Prompt are separate entry points and
 *      the Prompt API was still alpha as of late 2025 — so the calls below may
 *      need to be renamed to match the version you depend on.
 *   2. The JNI export of the `cc_aicore_*` C ABI over `availableBlocking` /
 *      `summarizeBlocking` (a `System.loadLibrary` native module). That glue is
 *      specified above and in README.md; it is deliberately not hand-written here
 *      as un-compilable native marshalling.
 *
 * The Rust adapter reports its capability by measurement, so until this producer
 * is wired and running it is honestly `Unavailable`; on a device where AICore
 * reports the feature ready it is `ImplementedUnverified`; and it reaches
 * `Verified` only after the calibration battery runs against the live model on
 * that device (spec §8) — never from this repository on non-Android hardware.
 */
class AICorePartner {

    /**
     * Whether Gemini Nano is usable right now, measured via AICore's feature
     * status — never assumed. Returns true only when the feature is already
     * downloaded and available; a downloadable-but-absent model is not "ready",
     * because forcing the (consent-gated, network) download is the host app's
     * decision, not the background semantic lane's.
     *
     * Maps to `cc_aicore_availability`.
     */
    fun availableBlocking(): Boolean =
        try {
            // ML Kit GenAI: `summarizer.checkFeatureStatus()` returns a
            // ListenableFuture<Int> whose value is one of the FeatureStatus
            // constants (UNAVAILABLE / DOWNLOADABLE / DOWNLOADING / AVAILABLE).
            // Verify the exact type and constant against the pinned SDK.
            generativeClient().checkFeatureStatusBlocking() == FeatureStatus.AVAILABLE
        } catch (t: Throwable) {
            // Any failure degrades to "not available" so the Rust adapter falls
            // back to proposing nothing, exactly as with no model at all.
            false
        }

    /**
     * Run one prompt and return the model's text, or null on unavailability or
     * error. Blocks on the ML Kit inference call; the caller runs it off the main
     * thread (the daemon's semantic pass). Trims surrounding whitespace so the
     * Rust adapter receives a bare description.
     *
     * Maps to `cc_aicore_summarize`.
     */
    fun summarizeBlocking(prompt: String): String? =
        try {
            if (!availableBlocking()) {
                null
            } else {
                // The task here is free-form generation ("describe what X does"),
                // which is the ML Kit GenAI Prompt API's shape; the Summarization
                // API is the alternative when the input is an article/conversation.
                // Confirm the generate call for the version you depend on.
                val text = generativeClient().generateBlocking(prompt).trim()
                text.ifEmpty { null }
            }
        } catch (t: Throwable) {
            null
        }

    /**
     * The ML Kit GenAI client handle. Isolated behind one function so the exact
     * builder (`Summarization.getClient(options)` / the Prompt API's
     * `Generation`/`GenerativeModel`) is the only thing to adjust when the SDK
     * version is pinned. Left `TODO` rather than faked so this cannot masquerade
     * as verified: the on-device build supplies the real client and the JNI export.
     */
    private fun generativeClient(): GenerativeClient =
        TODO(
            "Bind to the pinned ML Kit GenAI client (com.google.mlkit.genai.*) and " +
                "export cc_aicore_* over availableBlocking/summarizeBlocking via JNI. " +
                "See README.md — unverified until built and run on a supported device."
        )
}

/**
 * The minimal shape this producer needs from the ML Kit GenAI client, named
 * locally so the flow above type-checks in isolation. On device these calls are
 * satisfied by the real ML Kit GenAI client (`checkFeatureStatus`,
 * `runInference`/`generateContent`) over its ListenableFutures; this interface is
 * the seam where that binding is made, not a reimplementation of the model.
 */
private interface GenerativeClient {
    fun checkFeatureStatusBlocking(): Int
    fun generateBlocking(prompt: String): String
}

/** ML Kit GenAI FeatureStatus constants (mirror the SDK's values; verify on pin). */
private object FeatureStatus {
    const val UNAVAILABLE = 0
    const val DOWNLOADABLE = 1
    const val DOWNLOADING = 2
    const val AVAILABLE = 3
}
