# Architecture

Creature Context is a standalone Rust workspace. Coding platforms consume it through the command line and serialized contracts; no client must link Creature Context or CreatureIDE.

## Data flow

```text
PURPOSE.md + repository files
          ↓
deterministic scanner and identity reconciliation
          ↓
multiscale Atlas hierarchy + canonical relationship graph
          ↓
evidence-gated Green evaluation
          ↓
SQLite snapshot + .atlas.yaml + .module-map.yaml projection
          ↓
zoomed, compared or health-focused Orbit packet
```

Atlas owns hierarchy, module relationships and evidence. Orbit owns no canonical facts; it selects a snapshot-pinned, token-bounded view.

## Scale

- Universe: all registered projects.
- Galaxy: one product or codebase.
- System: a subsystem or module cluster.
- Planet: a module, service, package or component.
- Moon: a file, type, function, test or resource.

Paths are mutable attributes. Stable UUID identities are canonical. Exact-content rename reconciliation preserves file identity when the match is unique.

## Trust

Green is calculated independently for Content, Structure, Integration, Verification and Freshness. Required failures and insufficient proof roll upward. Model-inferred evidence cannot independently satisfy a Green gate.

Build systems, test runners, humans and other clients submit results through the
`evidence` command. Records are snapshot-bound in `.creature/evidence.json` and
are merged into the canonical graph during scanning. A changed repository gets
a new snapshot, making prior evidence stale rather than silently reusing it.
