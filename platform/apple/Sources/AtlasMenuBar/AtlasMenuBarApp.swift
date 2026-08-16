// A macOS menu-bar app for creature-context: keeps the live green-pixels atlas
// (ATLAS.png) one click away, and drives `scan-layered` / `map` on the chosen
// project by shelling out to the `creature-context` CLI. The image and the CLI
// are the contract — this app owns no atlas state of its own.

import AppKit
import CoreServices
import SwiftUI

/// Observable state: which project we're pointed at, the current atlas image, and
/// a one-line status. Atlas bytes come only from the CLI + ATLAS.png on disk.
final class AtlasModel: ObservableObject {
    @Published var projectPath: URL?
    @Published var image: NSImage?
    @Published var status: String = "Choose a project to begin."

    private let defaultsKey = "AtlasProjectPath"
    private var stream: FSEventStreamRef?
    private var lastModified: Date?

    init() {
        if let saved = UserDefaults.standard.url(forKey: defaultsKey) {
            projectPath = saved
            reload()
        }
        startWatching()
    }

    deinit { stopWatching() }

    /// Watch the project directory with FSEvents and reload when `ATLAS.png` is
    /// rewritten. The resident `creature-context run` daemon rewrites the file each
    /// re-index; a filesystem event stream (vs. a timer poll) makes the menu bar
    /// react the instant the daemon finishes its render, with no idle wakeups in
    /// between. FSEvents watches directories, so we watch the project dir and let
    /// `reloadIfChanged` filter for actual `ATLAS.png` mtime changes.
    private func startWatching() {
        stopWatching()
        guard let dir = projectPath else { return }

        var context = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passUnretained(self).toOpaque(),
            retain: nil,
            release: nil,
            copyDescription: nil
        )
        // C callback: no captured state, so hop back to `self` through `info`.
        let callback: FSEventStreamCallback = { _, info, _, _, _, _ in
            guard let info else { return }
            let model = Unmanaged<AtlasModel>.fromOpaque(info).takeUnretainedValue()
            DispatchQueue.main.async { model.reloadIfChanged() }
        }
        let flags = FSEventStreamCreateFlags(
            kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagNoDefer
        )
        guard let created = FSEventStreamCreate(
            kCFAllocatorDefault,
            callback,
            &context,
            [dir.path] as CFArray,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            0.2, // latency: coalesce the daemon's burst of render writes into one reload
            flags
        ) else {
            status = "Couldn't watch \(dir.lastPathComponent) — use Rescan/Remap."
            return
        }
        stream = created
        FSEventStreamSetDispatchQueue(created, DispatchQueue.global(qos: .utility))
        FSEventStreamStart(created)
    }

    /// Tear down the current event stream (on project change and on deinit).
    private func stopWatching() {
        guard let stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
        self.stream = nil
    }

    private func reloadIfChanged() {
        guard let dir = projectPath else { return }
        let png = dir.appendingPathComponent("ATLAS.png")
        let mod = (try? FileManager.default.attributesOfItem(atPath: png.path)[.modificationDate]) as? Date
        if let mod, mod != lastModified {
            lastModified = mod
            reload()
        }
    }

    /// Pick the project directory (the one holding `.creature/` and `ATLAS.png`).
    func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose project"
        if panel.runModal() == .OK, let url = panel.url {
            projectPath = url
            UserDefaults.standard.set(url, forKey: defaultsKey)
            reload()
            startWatching()
        }
    }

    /// Load `<project>/ATLAS.png` from disk into the view.
    func reload() {
        guard let dir = projectPath else { return }
        let png = dir.appendingPathComponent("ATLAS.png")
        if let img = NSImage(contentsOf: png) {
            image = img
            status = "Showing \(png.lastPathComponent)."
        } else {
            image = nil
            status = "No ATLAS.png yet — press Remap."
        }
    }

    /// Re-render the galaxy from the current snapshot.
    func remap() { run(["map", pathArg(), "--galaxy"]) }

    /// Re-index the project (bounded memory), then re-render.
    func rescan() { run(["scan-layered", pathArg()]) { [weak self] in self?.remap() } }

    /// Reveal the project in Finder.
    func revealInFinder() {
        guard let dir = projectPath else { return }
        _ = NSWorkspace.shared.open(dir)
    }

    private func pathArg() -> String { projectPath?.path ?? "." }

    /// Run the `creature-context` CLI off the main thread, then refresh on main.
    /// The binary is resolved via `env` so it works wherever the user's PATH puts
    /// it (e.g. a `cargo install`ed release build).
    private func run(_ args: [String], then continuation: (() -> Void)? = nil) {
        guard projectPath != nil else {
            status = "Choose a project first."
            return
        }
        status = "Running: creature-context \(args.joined(separator: " "))…"
        DispatchQueue.global(qos: .userInitiated).async {
            let task = Process()
            task.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            task.arguments = ["creature-context"] + args
            do {
                try task.run()
                task.waitUntilExit()
                let code = task.terminationStatus
                DispatchQueue.main.async {
                    self.status = code == 0 ? "Done." : "Failed (exit \(code)). Is creature-context on your PATH?"
                    self.reload()
                    continuation?()
                }
            } catch {
                DispatchQueue.main.async {
                    self.status = "Error: \(error.localizedDescription)"
                }
            }
        }
    }
}

/// The popover shown from the menu-bar item.
struct AtlasMenuView: View {
    @ObservedObject var model: AtlasModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("creature-context atlas")
                .font(.headline)

            Group {
                if let img = model.image {
                    Image(nsImage: img)
                        .resizable()
                        .interpolation(.none)
                        .aspectRatio(contentMode: .fit)
                } else {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.secondary.opacity(0.15))
                        .overlay(Text("No atlas image").foregroundStyle(.secondary))
                }
            }
            .frame(width: 320, height: 320)
            .clipShape(RoundedRectangle(cornerRadius: 8))

            Text(model.projectPath?.path ?? "No project selected")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)

            Text(model.status)
                .font(.caption2)
                .foregroundStyle(.secondary)

            HStack {
                Button("Choose…") { model.choose() }
                Button("Rescan") { model.rescan() }.disabled(model.projectPath == nil)
                Button("Remap") { model.remap() }.disabled(model.projectPath == nil)
            }

            HStack {
                Button("Reveal in Finder") { model.revealInFinder() }
                    .disabled(model.projectPath == nil)
                Spacer()
                Button("Quit") { NSApplication.shared.terminate(nil) }
            }
        }
        .padding(12)
        .frame(width: 344)
    }
}

/// Makes the app a menu-bar-only agent: no Dock icon, no app-switcher entry —
/// it lives entirely in the menu bar (`.accessory` activation policy, the
/// programmatic equivalent of `LSUIElement`).
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }
}

@main
struct AtlasMenuBarApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AtlasModel()

    var body: some Scene {
        MenuBarExtra("Atlas", systemImage: "map.fill") {
            AtlasMenuView(model: model)
        }
        .menuBarExtraStyle(.window)
    }
}
