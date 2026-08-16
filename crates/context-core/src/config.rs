//! Project scan configuration — `.creature/config.toml`, actually read.
//!
//! This file was written at `init` and never loaded by anything, so the limits it
//! displayed were decorative: the only way to change what the scanner did was to
//! edit Rust and recompile, and exceeding a limit killed the daemon rather than
//! degrading. Both are fixed here — the file is parsed, and a limit is a ceiling
//! that truncates and *says so*, never an error.
//!
//! Scope is the other half. A repository is not always the unit a person wants
//! indexed: a home directory or a Library folder is a legitimate root with only a
//! few interesting subtrees inside it, and a large project may hold hundreds of
//! thousands of data files that are not the code. `include` names the subtrees to
//! walk; `exclude` names directories to skip anywhere beneath them.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Hard ceilings on a scan. A zero means "no limit" — the honest way to say
/// unbounded, and the default for counts, because a project that is genuinely
/// large is not thereby a mistake.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ScanLimits {
    /// Files larger than this are skipped. Kept bounded by default: a multi-gigabyte
    /// artefact is not source and reading it would stall the scan.
    pub max_file_bytes: u64,
    /// Ceiling on files scanned. 0 = unlimited.
    pub max_files: usize,
    /// Ceiling on bytes read. 0 = unlimited.
    pub max_total_bytes: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1_048_576,
            // Unbounded by default. The previous 100_000 was below the size of
            // real projects this tool is meant to serve, and could not be raised
            // without recompiling.
            max_files: 0,
            max_total_bytes: 0,
        }
    }
}

impl ScanLimits {
    /// Whether `count` files is already at the ceiling.
    pub fn files_exhausted(&self, count: usize) -> bool {
        self.max_files != 0 && count >= self.max_files
    }

    /// Whether `bytes` has passed the ceiling.
    pub fn bytes_exhausted(&self, bytes: u64) -> bool {
        self.max_total_bytes != 0 && bytes > self.max_total_bytes
    }
}

/// Which parts of the root to index.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ScanScope {
    /// Root-relative directories to walk. Empty means the whole root. Naming any
    /// makes the root itself a container rather than the subject — which is what
    /// lets a home directory be a project root without indexing all of it.
    #[serde(default)]
    pub include: Vec<String>,
    /// Directory names skipped anywhere in the tree, added to the built-in list.
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl ScanScope {
    /// Whether a directory named `name` is excluded by configuration.
    pub fn excludes(&self, name: &str) -> bool {
        self.exclude.iter().any(|entry| entry == name)
    }
}

/// The whole of `.creature/config.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct ScanConfig {
    pub schema_version: u32,
    #[serde(flatten)]
    pub limits: ScanLimitsFields,
    #[serde(default)]
    pub scope: ScanScope,
}

/// The limit fields, flattened to the file's top level so the format written by
/// earlier versions still parses.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct ScanLimitsFields {
    pub max_file_bytes: u64,
    pub max_files: usize,
    pub max_total_bytes: u64,
}

impl Default for ScanLimitsFields {
    fn default() -> Self {
        let limits = ScanLimits::default();
        Self {
            max_file_bytes: limits.max_file_bytes,
            max_files: limits.max_files,
            max_total_bytes: limits.max_total_bytes,
        }
    }
}

impl ScanConfig {
    pub fn limits(&self) -> ScanLimits {
        ScanLimits {
            max_file_bytes: self.limits.max_file_bytes,
            max_files: self.limits.max_files,
            max_total_bytes: self.limits.max_total_bytes,
        }
    }

    /// Load `root`'s configuration. A missing or unparseable file falls back to
    /// the defaults rather than failing the scan: configuration is a preference,
    /// and losing it should degrade to sane behaviour, not stop the daemon.
    pub fn load(root: &Path) -> Self {
        let path = root.join(".creature").join("config.toml");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }
}

/// The file written at `init`: the defaults, with every field documented so the
/// knobs are discoverable in the place people look for them.
pub const DEFAULT_CONFIG_TOML: &str = "\
schema_version = 1

# Files larger than this are skipped (bytes).
max_file_bytes = 1048576

# Ceilings on a scan. 0 means unlimited. Exceeding one truncates the scan and
# records the truncation on the Atlas root — it never fails the scan.
max_files = 0
max_total_bytes = 0

[scope]
# Root-relative directories to index. Empty indexes the whole root. Naming any
# lets a large or general root (a home directory, a Library folder) be indexed
# without walking all of it.
include = []

# Directory names skipped anywhere beneath the root, in addition to the built-in
# list (.git, .creature, target, .build, node_modules, dist, build, .cache,
# vendor, and similar).
exclude = []
";

#[cfg(test)]
mod tests {
    use super::*;

    /// The format written by earlier versions, which had no `[scope]` table and
    /// top-level limits, must still parse — a config on disk predates this code.
    #[test]
    fn the_legacy_flat_format_still_parses() {
        let legacy = "schema_version = 1\nmax_file_bytes = 1048576\nmax_files = 100000\nmax_total_bytes = 536870912\n";
        let config: ScanConfig = toml::from_str(legacy).expect("parse legacy config");
        assert_eq!(config.limits().max_files, 100_000);
        assert_eq!(config.limits().max_total_bytes, 536_870_912);
        assert!(config.scope.include.is_empty());
    }

    /// The file `init` writes must parse into exactly the defaults it claims to
    /// document, or the documentation is a lie.
    #[test]
    fn the_written_default_config_parses_to_the_defaults() {
        let config: ScanConfig = toml::from_str(DEFAULT_CONFIG_TOML).expect("parse default config");
        assert_eq!(config.limits(), ScanLimits::default());
        assert!(config.scope.include.is_empty());
        assert!(config.scope.exclude.is_empty());
    }

    /// Zero means unlimited, and that is what makes a large project scannable.
    #[test]
    fn zero_means_unlimited() {
        let limits = ScanLimits {
            max_file_bytes: 1,
            max_files: 0,
            max_total_bytes: 0,
        };
        assert!(!limits.files_exhausted(800_000), "0 must mean unlimited");
        assert!(!limits.bytes_exhausted(u64::MAX), "0 must mean unlimited");

        let bounded = ScanLimits {
            max_file_bytes: 1,
            max_files: 10,
            max_total_bytes: 100,
        };
        assert!(bounded.files_exhausted(10));
        assert!(!bounded.files_exhausted(9));
        assert!(bounded.bytes_exhausted(101));
        assert!(!bounded.bytes_exhausted(100));
    }

    #[test]
    fn scope_round_trips_through_toml() {
        let text = "schema_version = 1\n\n[scope]\ninclude = [\"crates\", \"docs\"]\nexclude = [\"fixtures\"]\n";
        let config: ScanConfig = toml::from_str(text).expect("parse");
        assert_eq!(config.scope.include, vec!["crates", "docs"]);
        assert!(config.scope.excludes("fixtures"));
        assert!(!config.scope.excludes("crates"));
    }
}
