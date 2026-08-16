//! Incremental parsing: reuse a file's parse while its content is unchanged.
//!
//! The resident daemon re-indexes on every settled change, and indexing parses
//! every source file in the project. One keystroke in one file therefore paid
//! for a full re-parse of the tree — affordable for a one-shot `scan`, not for
//! something meant to run continuously (spec §7.1: the daemon is the constant
//! background lane).
//!
//! What is cached is the *parse*, never the entities built from it. Entity
//! construction stamps the current `SnapshotId` into every entity and every
//! piece of evidence, so a cached entity would carry a stale snapshot id and
//! quietly corrupt provenance. Rebuilding entities from a cached parse is pure,
//! cheap, and produces exactly what a fresh parse produces — which is what
//! `an_incrementally_enriched_snapshot_matches_a_full_one` proves.
//!
//! The key is the file's blake3 content fingerprint, which `scan_project`
//! already computes and stores on the file entity. Keying on content rather than
//! on watcher events is deliberate: watcher notifications are hints and may be
//! incomplete (spec §7.1), whereas a fingerprint cannot claim a file is
//! unchanged when it is not. A missing or evicted entry costs a re-parse and is
//! never wrong — the cache is an optimisation, not a source of truth.

use crate::adapter::{ParsedImport, ParsedSymbol};
use std::collections::HashMap;

/// Everything parsing a source file yields that enrichment needs. All three come
/// from the same tree-sitter pass, so they are cached together: `macro_defined_names`
/// parses the file exactly as `parse` does, and caching only the symbols would
/// save nothing.
#[derive(Clone, Debug, Default)]
pub struct ParsedFile {
    /// Top-level declarations, which become Moon entities.
    pub symbols: Vec<ParsedSymbol>,
    /// Intra-repo imports, which become `requires` sockets.
    pub imports: Vec<ParsedImport>,
    /// Identifiers a macro expands from. These feed the humility guard, so
    /// losing them would turn a defined-but-invisible name back into a
    /// fabricated broken link (spec §6.4).
    pub macro_names: Vec<String>,
}

/// Parse results keyed by file content fingerprint.
///
/// Held by the daemon across reconciliations. A one-shot `scan` builds an empty
/// one and throws it away, so the CLI parses everything exactly as before.
#[derive(Debug, Default)]
pub struct ParseCache {
    entries: HashMap<String, ParsedFile>,
    hits: usize,
    misses: usize,
}

impl ParseCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The parse for `fingerprint`, if this content has been parsed before.
    /// Records the hit so a caller can prove the cache is doing work.
    pub fn get(&mut self, fingerprint: &str) -> Option<&ParsedFile> {
        if self.entries.contains_key(fingerprint) {
            self.hits += 1;
            self.entries.get(fingerprint)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Record a freshly parsed file under its content fingerprint.
    pub fn insert(&mut self, fingerprint: String, parsed: ParsedFile) {
        self.entries.insert(fingerprint, parsed);
    }

    /// Drop entries for content no longer present in the project, so a long-lived
    /// daemon does not accumulate the parse of every version of every file it has
    /// ever seen. Called once per index with the current fingerprint set.
    pub fn retain_fingerprints(&mut self, live: &std::collections::HashSet<String>) {
        self.entries
            .retain(|fingerprint, _| live.contains(fingerprint));
    }

    /// Cached parses currently held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Files served from cache, and files that had to be parsed. The daemon's
    /// evidence that an unchanged tree is no longer being re-parsed.
    pub fn stats(&self) -> (usize, usize) {
        (self.hits, self.misses)
    }
}
