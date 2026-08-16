# Apple platform matrix

| Capability | State | Evidence boundary |
| --- | --- | --- |
| Deterministic Rust core | Verified by repository tests | Host build and test suite |
| Filesystem watcher | Verified on macOS | Runtime watcher tests |
| Finder-tag projection | Verified on macOS | Native round-trip tests |
| launchd supervisor | Verified on macOS | Definition and live-host tests |
| Foundation Models bridge | Runtime measured | Available only when the framework and system model respond; otherwise unavailable |
| Non-Apple native adapters | Not included | Separate repositories own those surfaces |

Model proposals never write directly. They pass through the deterministic
admission lane, and absence of an on-device model leaves that lane idle.
