# Android

On-device producer for Android: **Gemini Nano**, served by the **AICore** system
service and reached through the **ML Kit GenAI** APIs. Because AICore already
hosts a shared Gemini Nano, apps use it without bundling or downloading their own
model.

## Pieces

| Piece | Where | Status |
|-------|-------|--------|
| Rust `ModelPartner` adapter | [`crates/context-model/src/platform/android.rs`](../../crates/context-model/src/platform/android.rs) | Written; unit-tested on the (non-Android) dev host through its `Unavailable` fallback |
| Kotlin producer | [`src/main/kotlin/context/AICorePartner.kt`](src/main/kotlin/context/AICorePartner.kt) | Written against the documented ML Kit GenAI surface; **not compiled or run** |
| JNI export of `cc_aicore_*` | not yet written | Specified below; the remaining seam |

Apple exposes a C ABI over Swift and Windows over C++/WinRT, so their adapters
bind a native library directly. AICore is a Kotlin/Java surface, so the producer
is Kotlin and a thin JNI layer must export the three `cc_aicore_*` C symbols the
Rust adapter binds to:

```
cc_aicore_availability() -> AICorePartner.availableBlocking() ? 1 : 0
cc_aicore_summarize(utf8) -> AICorePartner.summarizeBlocking(prompt)   // null → propose nothing
cc_aicore_free(ptr)       -> free the JNI-owned C string
```

## Honesty boundary

Nothing here has been compiled or executed. The dev host is macOS with no Android
toolchain, no AICore, and no Gemini Nano. `android.rs` reports its capability by
measurement, so on any host without AICore — this machine included — it is
`Unavailable` and proposes nothing. On a device where AICore reports the feature
ready it is `ImplementedUnverified`, and it reaches `Verified` only after the
calibration battery runs against the live model on that device (spec §8) — never
from this repository on non-Android hardware.

`AICorePartner.kt` deliberately leaves the ML Kit client binding as a `TODO(...)`
rather than faked code: it must not masquerade as verified. Two things are settled
on real hardware, not here:

1. **Exact ML Kit GenAI class/method names** against the pinned SDK version. The
   GenAI APIs are young — Summarization and Prompt are separate entry points and
   the Prompt API was still alpha as of late 2025 — so the calls may need
   renaming to match your dependency.
2. **The JNI export** of the `cc_aicore_*` C ABI over `availableBlocking` /
   `summarizeBlocking`, as a `System.loadLibrary` native module.

## Wiring it on a supported device

Prerequisites: an AICore-capable device (Pixel 9/10, or a MediaTek Dimensity /
Qualcomm Snapdragon / Google Tensor platform on the supported list), and the ML
Kit GenAI dependency for the capability you use, e.g.
`com.google.mlkit:genai-summarization` (or the Prompt API artifact).

1. Bind `AICorePartner.generativeClient()` to the real ML Kit GenAI client and
   confirm `checkFeatureStatus` / generate signatures against the pinned SDK.
2. Add a JNI native module that exports `cc_aicore_availability`,
   `cc_aicore_summarize`, and `cc_aicore_free` over the two blocking methods, and
   load it with `System.loadLibrary`.
3. Build `creature-context-model` for `aarch64-linux-android` with that library on
   the link line and the cfg enabled, e.g.
   `RUSTFLAGS="--cfg aicore_bridge -L native=<dir> -l <jni-lib>"`. (`build.rs`
   already declares `aicore_bridge` as a known cfg, so this stays warning-clean
   under `-D warnings`.)
4. First use is consent-gated: AICore reports `DOWNLOADABLE` until the shared
   model is present. The adapter honestly reports `Unavailable` until it is
   `AVAILABLE`.
5. Verify: run the calibration battery on-device and record the measured profile
   as the evidence that turns `ImplementedUnverified` into `Verified`.
