// C ABI bridge to Apple Foundation Models (specification 8, 16).
//
// This is the I/O skin: a stable C surface the portable Rust core calls to reach
// the on-device model. It holds no Atlas state and decides nothing — it reports
// the model's measured availability and, when asked, returns a single
// natural-language summary for a prompt. Everything it returns is a proposal the
// Rust side wraps as a CandidateRecord and sends through admission; the model
// never writes to the Atlas.
//
// The model API is async; the C ABI is synchronous, so each call blocks the
// caller on a semaphore while the inference runs on the cooperative pool. The
// returned C string is heap-allocated with strdup and must be freed by the
// caller (cc_foundation_free).

import Foundation
import FoundationModels

/// 1 when the on-device model is available and usable on this machine, 0
/// otherwise. Measured, never assumed (spec §8, §16).
@_cdecl("cc_foundation_availability")
public func cc_foundation_availability() -> Int32 {
    switch SystemLanguageModel.default.availability {
    case .available: return 1
    default: return 0
    }
}

/// Run one prompt through the on-device model and return its response as a
/// freshly allocated C string, or NULL on unavailability or error. Synchronous:
/// blocks until the model responds.
@_cdecl("cc_foundation_summarize")
public func cc_foundation_summarize(_ promptC: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
    guard let promptC else { return nil }
    guard case .available = SystemLanguageModel.default.availability else { return nil }
    let prompt = String(cString: promptC)

    let semaphore = DispatchSemaphore(value: 0)
    var output: String? = nil
    Task {
        defer { semaphore.signal() }
        do {
            let session = LanguageModelSession()
            let response = try await session.respond(to: prompt)
            output = response.content
        } catch {
            output = nil
        }
    }
    semaphore.wait()

    guard let output else { return nil }
    return strdup(output)
}

/// Free a string returned by this bridge.
@_cdecl("cc_foundation_free")
public func cc_foundation_free(_ ptr: UnsafeMutablePointer<CChar>?) {
    if let ptr { free(ptr) }
}
