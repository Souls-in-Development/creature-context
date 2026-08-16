# Windows

On-device producer for Windows: **Phi Silica**, the NPU-tuned small language
model shipped in the Windows App SDK, reached through the WinRT class
`Microsoft.Windows.AI.Text.LanguageModel`.

## Pieces

| Piece | Where | Status |
|-------|-------|--------|
| Rust `ModelPartner` adapter | [`crates/context-model/src/platform/windows.rs`](../../crates/context-model/src/platform/windows.rs) | Written; unit-tested on the (non-Windows) dev host through its `Unavailable` fallback |
| C-ABI native shim (C++/WinRT) | [`CcPhiSilicaBridge.cpp`](CcPhiSilicaBridge.cpp) | Written against the documented API; **not compiled or run** — see honesty boundary |

The adapter is the product surface; the shim is the on-device implementation of
the `cc_phi_silica_*` C ABI the adapter binds to. This mirrors the Apple pattern
(`apple.rs` + `CBridge.swift`), with one difference: on macOS the Apple shim is
built by `build.rs` because the host *is* the target, so it is verified here. No
Windows target can be built on this macOS host, so the shim is compiled and
linked by the **Windows App SDK app project**, not by cargo.

## Honesty boundary

`windows.rs` reports its capability by measurement, so on any host without Phi
Silica — this development machine included — it is `Unavailable` and proposes
nothing. On a Copilot+ PC where the shim is linked and the model is ready it is
`ImplementedUnverified`, and it reaches `Verified` only after the calibration
battery runs against the live model on that device (spec §8). It is never
`Verified` from this repository's CI on non-Windows hardware.

`CcPhiSilicaBridge.cpp` has not been compiled or executed. The dev host lacks the
Windows App SDK, the C++/WinRT projection headers, and an NPU. Treat it as a
faithful starting point to verify on a Copilot+ PC, not a finished component.
Re-check method and enum names against the installed Windows App SDK first — the
surface has moved before (`LanguageModelAvailability` → `AIFeatureReadyState`),
and Phi Silica is scheduled to be superseded by **Aion Instruct** in late 2026
behind the same `LanguageModel` entry point.

## Wiring it on a Copilot+ PC

Prerequisites: Windows 11 26100+ on a Copilot+ PC (NPU), the Windows App SDK
(2.0.0-preview1 or later), a packaged (MSIX) app identity, and the
`systemAIModels` capability declared in `Package.appxmanifest`.

1. Compile `CcPhiSilicaBridge.cpp` with the app project (C++/WinRT, `/std:c++20`),
   producing a static lib or object linked into the process that hosts the
   Creature Context runtime.
2. Build the `creature-context-model` crate for `*-pc-windows-msvc` with the shim
   on the link line and the cfg enabled, e.g.
   `RUSTFLAGS="--cfg phi_silica_bridge -L native=<dir-with-shim> -l static=ccphisilica"`.
   (`build.rs` already declares `phi_silica_bridge` as a known cfg, so this stays
   warning-clean under `-D warnings`.)
3. First run is consent-gated: call `EnsureReadyAsync()` from the host app to
   download/prepare the model. Until then `GetReadyState()` is `NotReady` and the
   adapter honestly reports `Unavailable`.
4. Verify: run the calibration battery on-device and record the measured profile
   as the evidence that turns `ImplementedUnverified` into `Verified`.
