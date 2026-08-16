# Creature Context for Apple

Creature Context is a local-first repository-context engine with a deterministic
Rust core and Apple-native integration. It maintains a multiscale Atlas, module
relationships, evidence-backed Green status, and token-bounded Orbit packets.

This is the Apple-first canonical repository. Its native surfaces are:

- macOS Finder-tag metadata projection;
- a launchd resident-service adapter;
- an Apple Foundation Models partner bridged through Swift.

Foundation Models availability is measured at runtime. If the framework, model,
or supported hardware is unavailable, semantic enrichment remains idle while the
deterministic core continues to work.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
swift test --package-path platform/apple
```

See [docs/platform-matrix.md](docs/platform-matrix.md) for the exact capability
boundary.
