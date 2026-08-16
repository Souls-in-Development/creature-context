//! Enrich a scanned snapshot with parsed structure.
//!
//! Applied *after* `scan_project`, and from this crate rather than the scanner:
//! the scanner lives in `context-core`, and `context-core` does not depend on
//! `context-parsers` — the dependency runs the other way. So the CLI scans
//! (core), then enriches (here), then commits.
//!
//! For each source file the scanner produced as a Moon entity, the file is
//! parsed and its top-level declarations are added as Moon entities under it,
//! joined by an `observed` `contains` edge. A file Moon containing symbol Moons
//! is valid same-scale nesting (specification 3.4, hierarchy `allowed`).
//! Parsing is enrichment: an unsupported language or a parse failure leaves the
//! deterministic file entity exactly as the scanner produced it (spec §17).

use crate::adapter::{Construct, ParsedImport, macro_defined_names, parse, parse_imports};
use crate::incremental::{ParseCache, ParsedFile};
use crate::languages::language_for_extension;
use creature_context_types::{
    AtlasEdge, AtlasEntity, AtlasSnapshot, AtlasSocket, EdgeId, EntityId, EntityKind, Evidence,
    EvidenceOutcome, FactSource, GreenAxis, HoleReason, ProofStrength, RelationshipKind,
    RelationshipPlane, ScanProgress, ScanStage, ScopeScale, SnapshotId, SocketDirection, SocketId,
    SocketResolution, SocketShape, SourceSpan,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Enrich `snapshot` in place with parsed symbols, reading sources under `root`.
/// Returns the number of symbol entities added. Every file is parsed; this is
/// what a one-shot `scan` wants.
pub fn enrich_snapshot(root: &Path, snapshot: &mut AtlasSnapshot) -> usize {
    let mut cache = ParseCache::new();
    enrich_snapshot_cached(root, snapshot, &mut cache)
}

/// Enrich `snapshot`, reusing `cache` for any file whose content fingerprint has
/// been parsed before. Identical in output to `enrich_snapshot` — the cache holds
/// parses, and entities are rebuilt from them against the current snapshot id —
/// but a file whose content has not changed is not read or parsed again. This is
/// the entry point the resident daemon uses (spec §7.1).
pub fn enrich_snapshot_cached(
    root: &Path,
    snapshot: &mut AtlasSnapshot,
    cache: &mut ParseCache,
) -> usize {
    let files = moon_files(snapshot);

    // Bound the cache to content the project still contains, before this pass
    // adds to it.
    let live: HashSet<String> = files
        .iter()
        .map(|(_, _, fingerprint)| fingerprint.clone())
        .collect();
    cache.retain_fingerprints(&live);

    // Sequential, cache-reusing parse: the daemon re-indexes on every settled
    // change, and a file whose content is unchanged is served from the cache
    // rather than re-read and re-parsed (spec §7.1). Each parse is assembled and
    // dropped before the next, so only one parse is held at a time.
    let snapshot_id = snapshot.id.clone();
    let mut acc = Assembly::default();
    for (file_id, relative_path, fingerprint) in &files {
        if let Some(parsed) = parse_cached(root, relative_path, fingerprint, cache) {
            acc.assemble_file(snapshot, &snapshot_id, *file_id, relative_path, &parsed);
        }
    }
    acc.finish(snapshot, None)
}

/// Number of files parsed in parallel before assembling and dropping the batch.
/// Bounds transient memory to this many `ParsedFile`s at once, rather than the
/// whole tree — the difference between a bounded footprint and holding every
/// file's parse in RAM. Large enough to keep every core busy between barriers.
const PARSE_BATCH: usize = 4096;

/// As `enrich_snapshot_cached`, but parses in parallel across the available
/// cores, in bounded batches, reporting progress. A one-shot `scan` builds an
/// empty cache and parses everything regardless, so there is nothing to reuse —
/// dropping the cache costs nothing and lets the parses run on separate threads.
/// Each batch is parsed in parallel, then assembled sequentially in file order
/// and dropped before the next batch, so at most `PARSE_BATCH` parses are held at
/// once and the snapshot is byte-for-byte identical to the cached path (the
/// `enrich_matches_*` tests cover both).
pub fn enrich_snapshot_parallel(
    root: &Path,
    snapshot: &mut AtlasSnapshot,
    progress: Option<&dyn ScanProgress>,
    defined_names_out: Option<&mut HashSet<String>>,
) -> usize {
    let files = moon_files(snapshot);
    let total = files.len();
    if let Some(progress) = progress {
        progress.stage(ScanStage::Folders, &format!("{total} files"));
    }
    let snapshot_id = snapshot.id.clone();
    let done = AtomicUsize::new(0);
    let mut acc = Assembly::default();
    for batch in files.chunks(PARSE_BATCH) {
        let parsed = parse_batch_parallel(root, batch, &done, total, progress);
        for ((file_id, relative_path, _), parsed) in batch.iter().zip(&parsed) {
            if let Some(parsed) = parsed {
                acc.assemble_file(snapshot, &snapshot_id, *file_id, relative_path, parsed);
            }
        }
        // `parsed` is dropped here, before the next batch is read — this is what
        // bounds the footprint.
    }
    acc.finish(snapshot, defined_names_out)
}

/// The scanner's file entities, as `(id, relative path, content fingerprint)`.
/// The fingerprint is the blake3 of the file's bytes the scanner already stored,
/// which is the parse cache's key.
fn moon_files(snapshot: &AtlasSnapshot) -> Vec<(EntityId, String, String)> {
    snapshot
        .entities
        .iter()
        .filter(|e| e.scale == ScopeScale::Moon)
        .filter_map(|e| {
            e.relative_path
                .clone()
                .map(|p| (e.id, p, e.structural_fingerprint.clone()))
        })
        .collect()
}

/// Read and parse one file into everything enrichment needs. `None` when the
/// file has no grammar, cannot be read, or fails to parse — each of which leaves
/// the deterministic file entity untouched, exactly as the old loop's `continue`
/// arms did. Pure and side-effect free, so it is safe to call from many threads.
fn parse_one(root: &Path, relative_path: &str) -> Option<ParsedFile> {
    let language = language_of(relative_path)?;
    let source = std::fs::read_to_string(root.join(relative_path)).ok()?;
    let symbols = parse(&source, language).ok()?;
    Some(ParsedFile {
        symbols,
        imports: parse_imports(&source, language).unwrap_or_default(),
        macro_names: macro_defined_names(&source, language).unwrap_or_default(),
    })
}

/// Parse one file, serving and populating `cache`. A file with no grammar never
/// touches the cache, and content with no fingerprint is parsed every time and
/// never cached — so the hit/miss counts mean what they say and an empty key
/// cannot serve one file's parse for another.
fn parse_cached(
    root: &Path,
    relative_path: &str,
    fingerprint: &str,
    cache: &mut ParseCache,
) -> Option<ParsedFile> {
    language_of(relative_path)?;
    if !fingerprint.is_empty() {
        if let Some(parsed) = cache.get(fingerprint) {
            return Some(parsed.clone());
        }
    }
    let parsed = parse_one(root, relative_path)?;
    if !fingerprint.is_empty() {
        cache.insert(fingerprint.to_string(), parsed.clone());
    }
    Some(parsed)
}

/// The grammar key for a path's extension, or `None` when no grammar matches.
fn language_of(relative_path: &str) -> Option<&'static str> {
    Path::new(relative_path)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(language_for_extension)
}

/// Parse one batch of files across the available cores, preserving each file's
/// slot so the result is ordered identically to a sequential parse. The output
/// vector is split into one contiguous chunk per worker, so each thread writes
/// only its own range and no lock guards the output; `done` is a shared atomic
/// carried across batches so the progress count is cumulative over the whole run.
fn parse_batch_parallel(
    root: &Path,
    batch: &[(EntityId, String, String)],
    done: &AtomicUsize,
    total: usize,
    progress: Option<&dyn ScanProgress>,
) -> Vec<Option<ParsedFile>> {
    let count = batch.len();
    let mut results: Vec<Option<ParsedFile>> = (0..count).map(|_| None).collect();
    if count == 0 {
        return results;
    }
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(count);
    let chunk = count.div_ceil(workers);
    std::thread::scope(|scope| {
        for (chunk_index, out) in results.chunks_mut(chunk).enumerate() {
            let base = chunk_index * chunk;
            // A spawned thread's default 2 MiB stack is smaller than the main
            // thread's 8 MiB, and tree-sitter recurses with input depth: a large
            // or deeply nested file that parses fine on the main thread overflows
            // a worker's. Give each worker a 16 MiB stack, with headroom.
            std::thread::Builder::new()
                .stack_size(16 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    for (offset, slot) in out.iter_mut().enumerate() {
                        let relative_path = &batch[base + offset].1;
                        *slot = parse_one(root, relative_path);
                        let seen = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if let Some(progress) = progress {
                            progress.tick(seen, total);
                        }
                    }
                })
                .expect("spawn parse worker thread");
        }
    });
    results
}

/// The state accumulated while turning parsed files into entities, edges and
/// sockets. Kept across batches so per-file assembly can happen incrementally
/// (parse a batch, assemble it, drop it) while the cross-file work — socket
/// attachment, resolution and the humility guard — still runs once over the
/// whole snapshot in `finish`.
#[derive(Default)]
struct Assembly {
    /// Required sockets, attached to their file entities in `finish` — the file
    /// entities already exist, but new symbol entities are being pushed as we go.
    pending_requires: Vec<(EntityId, AtlasSocket)>,
    /// Every name the repository defines that a required socket might target —
    /// declarations (public or not) and identifiers a macro expands from. The
    /// provides index sees only parsed public declarations, so a required import
    /// of a macro-generated or private name would otherwise look like proof of
    /// absence. This set is the humility guard: a `no_match` for a name defined
    /// but invisible here is downgraded to Unknown rather than reported as a
    /// broken link (spec §6.4, §17 — degrade explicitly, never fabricate).
    defined_names: HashSet<String>,
    /// Symbol ids already emitted, so a second declaration with the same name on
    /// the same line — which hashes to the same id — is given a distinct
    /// occurrence suffix rather than producing a duplicate the hierarchy rejects.
    seen_symbol_ids: HashSet<EntityId>,
    /// Symbol entities added so far.
    added: usize,
}

impl Assembly {
    /// Add one parsed file's symbols, edges and required sockets, in file order.
    /// Called once per file; the file order across batches reproduces exactly the
    /// order a single sequential pass would produce.
    fn assemble_file(
        &mut self,
        snapshot: &mut AtlasSnapshot,
        snapshot_id: &SnapshotId,
        file_id: EntityId,
        relative_path: &str,
        parsed: &ParsedFile,
    ) {
        for symbol in &parsed.symbols {
            self.defined_names.insert(symbol.name.clone());
            // Two declarations with the same name on the same line hash to the
            // same id; the first keeps the bare id (so a file without such a
            // collision is unchanged), and later ones take an occurrence suffix so
            // every id is unique. Deterministic: parse order is stable, so the Nth
            // collision always gets suffix N.
            let mut symbol_id = symbol_entity_id(file_id, &symbol.name, symbol.start_line);
            let mut occurrence = 1u32;
            while !self.seen_symbol_ids.insert(symbol_id) {
                symbol_id =
                    symbol_entity_id_occurrence(file_id, &symbol.name, symbol.start_line, occurrence);
                occurrence += 1;
            }
            snapshot.entities.push(symbol_entity(
                symbol_id,
                file_id,
                relative_path,
                symbol,
                snapshot_id,
            ));
            snapshot
                .edges
                .push(contains_edge(file_id, symbol_id, snapshot_id));
            self.added += 1;
        }

        self.defined_names
            .extend(parsed.macro_names.iter().cloned());

        // An intra-repo import is a shape this file requires. External imports
        // are not extracted (adapter::parse_imports), so a required socket here
        // always names something the repository itself is expected to provide.
        for import in &parsed.imports {
            self.pending_requires
                .push((file_id, requires_socket(file_id, import, snapshot_id)));
        }
    }

    /// The cross-file tail, run once: attach required sockets, resolve them
    /// against the provides index, then apply the humility guard. Returns the
    /// number of symbol entities added. When `defined_names_out` is given, the
    /// full `defined_names` set (symbol names plus macro-expanded names) is copied
    /// into it — the layered scan's global humility pass needs the macro names,
    /// which are otherwise transient and never persisted.
    fn finish(
        self,
        snapshot: &mut AtlasSnapshot,
        defined_names_out: Option<&mut HashSet<String>>,
    ) -> usize {
        // Attach required sockets by entity-id index rather than a linear `find`
        // per import. The scan pushes a symbol entity for every declaration, so by
        // now `entities` is millions long on a large repo and the old per-import
        // scan was O(imports × entities). One id→position map makes each attach
        // O(1) and preserves push order for a given file.
        let entity_index: HashMap<EntityId, usize> = snapshot
            .entities
            .iter()
            .enumerate()
            .map(|(position, entity)| (entity.id, position))
            .collect();
        for (file_id, socket) in self.pending_requires {
            if let Some(&position) = entity_index.get(&file_id) {
                snapshot.entities[position].sockets.push(socket);
            }
        }

        // The deterministic reconciler decides which required shapes fit which
        // provided ones (spec §6.4). The Milestone 2 evaluator then darkens the
        // integration axis from these resolutions when Green is next computed.
        creature_context_core::sockets::resolve_sockets(snapshot);

        // Humility pass: a `no_match` is only proof of absence when the provides
        // index is authoritative. It is not — Tree-sitter cannot see macro-expanded
        // or private declarations — so a required name that the repository defines
        // by some means invisible here is Unknown, not a broken link. A name absent
        // everywhere stays a `no_match`, which is what makes the hole trustworthy.
        for entity in &mut snapshot.entities {
            for socket in &mut entity.sockets {
                if socket.direction == SocketDirection::Requires
                    && matches!(
                        &socket.resolution,
                        SocketResolution::Hole(hole) if hole.reason == HoleReason::NoMatch
                    )
                    && self
                        .defined_names
                        .contains(leaf_name(&socket.shape.qualified_name))
                {
                    socket.resolution = SocketResolution::Unresolved;
                }
            }
        }

        // Hand the full defined-names set to a caller that asked for it (the
        // layered stitch), after the humility loop's immutable borrow of it ends.
        if let Some(out) = defined_names_out {
            out.extend(self.defined_names.iter().cloned());
        }

        self.added
    }
}

/// The item name a socket shape is matched on: the final `::`/`.`/`/` segment.
fn leaf_name(qualified_name: &str) -> &str {
    qualified_name
        .rsplit([':', '.', '/'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(qualified_name)
}

/// A UUID derived deterministically from a key, so a rescan produces the same
/// ids (blake3, since the workspace uuid has no v5 feature).
fn deterministic_uuid(key: &str) -> uuid::Uuid {
    let hash = blake3::hash(key.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    uuid::Uuid::from_bytes(bytes)
}

/// Deterministic id: the same file, symbol and line always yield the same
/// entity id, so a rescan is stable.
fn symbol_entity_id(file: EntityId, name: &str, start_line: usize) -> EntityId {
    EntityId(deterministic_uuid(&format!("{file}/{name}/{start_line}")))
}

/// The disambiguated id for the `occurrence`-th (1-based) declaration that shares
/// a file, name and line with an earlier one. The key differs from
/// `symbol_entity_id`'s by the `#{occurrence}` suffix, so it never collides with a
/// bare id and stays stable across rescans.
fn symbol_entity_id_occurrence(
    file: EntityId,
    name: &str,
    start_line: usize,
    occurrence: u32,
) -> EntityId {
    EntityId(deterministic_uuid(&format!(
        "{file}/{name}/{start_line}#{occurrence}"
    )))
}

fn entity_kind(construct: &Construct) -> EntityKind {
    match construct {
        Construct::Shared(canonical) => match canonical.as_str() {
            "function" | "closure" | "procedure" | "method" => EntityKind::Function,
            "test" => EntityKind::Test,
            "product_type" | "class" | "enumeration" | "behavioral_contract" | "type_alias" => {
                EntityKind::Type
            }
            _ => EntityKind::Component,
        },
        Construct::Native(_) => EntityKind::Component,
    }
}

fn construct_label(construct: &Construct) -> String {
    match construct {
        Construct::Shared(c) => c.clone(),
        Construct::Native(n) => format!("native:{}", n.name),
    }
}

fn symbol_entity(
    id: EntityId,
    parent: EntityId,
    file_path: &str,
    symbol: &crate::adapter::ParsedSymbol,
    snapshot: &SnapshotId,
) -> AtlasEntity {
    let span = SourceSpan {
        source_id: file_path.to_string(),
        relative_path: file_path.to_string(),
        start_line: symbol.start_line as u32,
        start_column: 1,
        end_line: symbol.end_line as u32,
        end_column: 1,
        content_hash: String::new(),
    };
    // Content and Structure, both from parsing — the same two axes the scanner
    // asserts for a file, so a symbol starts on equal footing with the file that
    // contains it. The other axes (integration, verification, freshness,
    // coherence) remain Unknown until evidence is recorded, so a symbol is not
    // Green merely for having been parsed.
    let evidence: Vec<Evidence> = [GreenAxis::Content, GreenAxis::Structure]
        .into_iter()
        .map(|axis| Evidence {
            axis,
            source: FactSource::Parsed,
            proof: ProofStrength::Syntax,
            outcome: EvidenceOutcome::Pass,
            confidence: 1.0,
            fingerprint: snapshot.0.clone(),
            observed_at: "2026-08-07T00:00:00Z".into(),
            producer: "creature-context-parsers".into(),
            snapshot_id: snapshot.clone(),
            message: String::new(),
        })
        .collect();
    AtlasEntity {
        id,
        scale: ScopeScale::Moon,
        kind: entity_kind(&symbol.construct),
        canonical_name: symbol.name.clone(),
        aliases: vec![],
        parent_id: Some(parent),
        relative_path: Some(file_path.to_string()),
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        // An exported declaration provides its shape for others to require; a
        // private one exposes nothing to match against.
        sockets: if symbol.exported {
            vec![provides_socket(id, symbol, snapshot)]
        } else {
            vec![]
        },
        source_spans: vec![span],
        structural_fingerprint: construct_label(&symbol.construct),
        local_evidence: evidence,
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: snapshot.clone(),
        observed_at: "2026-08-07T00:00:00Z".into(),
        fresh_until: None,
    }
}

fn contains_edge(file: EntityId, symbol: EntityId, snapshot: &SnapshotId) -> AtlasEdge {
    let key = format!("contains/{file}/{symbol}");
    AtlasEdge {
        id: EdgeId(deterministic_uuid(&key)),
        source_entity_id: file,
        target_entity_id: symbol,
        kind: RelationshipKind::Contains,
        // Observed: a parser saw the file contain this symbol.
        plane: RelationshipPlane::Observed,
        proof_record_ids: vec![],
        evidence: vec![Evidence {
            axis: GreenAxis::Integration,
            source: FactSource::Parsed,
            proof: ProofStrength::Syntax,
            outcome: EvidenceOutcome::Pass,
            confidence: 1.0,
            fingerprint: snapshot.0.clone(),
            observed_at: "2026-08-07T00:00:00Z".into(),
            producer: "creature-context-parsers".into(),
            snapshot_id: snapshot.clone(),
            message: String::new(),
        }],
        source_id: "creature-context-parsers".into(),
        confidence: 1.0,
        observed_at: "2026-08-07T00:00:00Z".into(),
        fresh_until: None,
        required: false,
        snapshot_id: snapshot.clone(),
    }
}

/// A shape for socket matching. Matching keys on the name (spec §6.4), so the
/// hash spans all three fields to keep distinct shapes distinct in the IDX.
fn socket_shape(qualified_name: &str, signature: &str) -> SocketShape {
    let version = "1";
    let hash = blake3::hash(format!("{qualified_name}|{signature}|{version}").as_bytes())
        .to_hex()
        .to_string();
    SocketShape {
        qualified_name: qualified_name.to_string(),
        structural_signature: signature.to_string(),
        version: version.to_string(),
        hash,
    }
}

/// The `provides` socket for an exported declaration: the shape it exposes. The
/// name is the declaration's own; the signature is its construct, which is what
/// Tree-sitter can see without a type checker.
fn provides_socket(
    entity: EntityId,
    symbol: &crate::adapter::ParsedSymbol,
    snapshot: &SnapshotId,
) -> AtlasSocket {
    AtlasSocket {
        id: SocketId(deterministic_uuid(&format!(
            "provides/{entity}/{}",
            symbol.name
        ))),
        entity_id: entity,
        direction: SocketDirection::Provides,
        shape: socket_shape(&symbol.name, &construct_label(&symbol.construct)),
        optional: false,
        resolution: SocketResolution::Unresolved,
        source_id: "creature-context-parsers".into(),
        confidence: 1.0,
        observed_at: "2026-08-07T00:00:00Z".into(),
        snapshot_id: snapshot.clone(),
    }
}

/// The `requires` socket for an intra-repo import: the shape a file needs. An
/// import does not reveal its target's signature, so the shape carries only the
/// name (the load-bearing field for matching, spec §6.4). Not optional — an
/// unmet intra-repo import is a real integration finding, so it must be able to
/// darken the axis.
fn requires_socket(file: EntityId, import: &ParsedImport, snapshot: &SnapshotId) -> AtlasSocket {
    AtlasSocket {
        id: SocketId(deterministic_uuid(&format!(
            "requires/{file}/{}/{}",
            import.path, import.start_line
        ))),
        entity_id: file,
        direction: SocketDirection::Requires,
        shape: socket_shape(&import.path, ""),
        optional: false,
        resolution: SocketResolution::Unresolved,
        source_id: "creature-context-parsers".into(),
        confidence: 1.0,
        observed_at: "2026-08-07T00:00:00Z".into(),
        snapshot_id: snapshot.clone(),
    }
}
