// C-ABI bridge to Windows Phi Silica for the Rust adapter
// `crates/context-model/src/platform/windows.rs` (specification 8, 16).
//
// This is the Windows analogue of `platform/apple/.../CBridge.swift`: it exposes
// three C symbols the Rust `bridge` module binds to when a Windows build sets the
// `phi_silica_bridge` cfg. It calls the on-device small language model through the
// Windows App SDK WinRT surface — `Microsoft.Windows.AI.Text.LanguageModel`
// (GetReadyState / EnsureReadyAsync / CreateAsync / GenerateResponseAsync).
// See https://learn.microsoft.com/en-us/windows/ai/apis/phi-silica
//
// ── Honesty boundary ─────────────────────────────────────────────────────────
// This file is written against the documented API but has NOT been compiled or
// run: the development host is macOS, which has no Windows App SDK, no C++/WinRT
// projection headers, and no NPU. It is a faithful starting point for a Copilot+
// PC build, not a verified component. Nothing in the Rust workspace compiles it;
// the Windows App SDK app project is responsible for compiling and linking it and
// for setting `phi_silica_bridge` (see README.md). Method and enum names should be
// re-checked against the installed Windows App SDK version before first build —
// the surface has moved once already (e.g. LanguageModelAvailability →
// AIFeatureReadyState) and Phi Silica itself is scheduled to be replaced by Aion
// Instruct in late 2026 behind the same `LanguageModel` entry point.
//
// Contract mirrored from the Rust side:
//   cc_phi_silica_availability() -> 1 iff the model is Ready, else 0 (never throws)
//   cc_phi_silica_summarize(utf8) -> malloc'd UTF-8 response, or nullptr on error
//   cc_phi_silica_free(ptr)       -> frees a buffer returned by summarize

#include <cstdlib>
#include <cstring>
#include <string>

#include <winrt/base.h>
#include <winrt/Microsoft.Windows.AI.h>
#include <winrt/Microsoft.Windows.AI.Text.h>

using namespace winrt;
using namespace winrt::Microsoft::Windows::AI;
using namespace winrt::Microsoft::Windows::AI::Text;

namespace {

// Duplicate a std::string into a malloc'd C buffer the Rust side frees via
// cc_phi_silica_free. Returns nullptr if empty so callers treat "no text" and
// "error" identically — both mean "propose nothing".
char* dup_utf8(const std::string& text) {
    if (text.empty()) {
        return nullptr;
    }
    char* out = static_cast<char*>(std::malloc(text.size() + 1));
    if (out != nullptr) {
        std::memcpy(out, text.c_str(), text.size() + 1);
    }
    return out;
}

}  // namespace

extern "C" {

// Measured readiness — never assumed. Any WinRT failure is reported as
// "not available" rather than propagated, so the Rust adapter degrades to its
// honest Unavailable fallback.
__declspec(dllexport) int cc_phi_silica_availability() {
    try {
        return LanguageModel::GetReadyState() == AIFeatureReadyState::Ready ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

// Run one prompt and return the model's text. Blocks on the WinRT async calls;
// this runs on the daemon's off-thread semantic pass, not a UI thread. Any
// failure returns nullptr → the adapter proposes nothing.
__declspec(dllexport) char* cc_phi_silica_summarize(const char* prompt) {
    if (prompt == nullptr) {
        return nullptr;
    }
    try {
        if (LanguageModel::GetReadyState() != AIFeatureReadyState::Ready) {
            // Preparing/downloading the model is a first-run, consent-gated step
            // the host app drives; the semantic lane does not force it here.
            return nullptr;
        }
        LanguageModel model = LanguageModel::CreateAsync().get();
        hstring request = to_hstring(std::string(prompt));
        LanguageModelResponse response = model.GenerateResponseAsync(request).get();
        std::string text = to_string(response.Text());
        // Trim leading/trailing ASCII whitespace to match the Rust adapter's
        // expectation of a bare description.
        size_t begin = text.find_first_not_of(" \t\r\n");
        size_t end = text.find_last_not_of(" \t\r\n");
        if (begin == std::string::npos) {
            return nullptr;
        }
        return dup_utf8(text.substr(begin, end - begin + 1));
    } catch (...) {
        return nullptr;
    }
}

__declspec(dllexport) void cc_phi_silica_free(char* ptr) {
    std::free(ptr);
}

}  // extern "C"
