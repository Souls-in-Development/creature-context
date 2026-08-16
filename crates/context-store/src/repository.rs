use creature_context_types::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use thiserror::Error;

const MIGRATION: &str = include_str!("../migrations/0004_multiscale_atlas.sql");

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Uuid(#[from] uuid::Error),
    #[error("database has no current Atlas snapshot")]
    Empty,
    #[error("stored root entity is missing")]
    MissingRoot,
}

pub struct AtlasRepository {
    connection: Connection,
}

impl AtlasRepository {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        // Write-Ahead Logging so a reader never blocks on the resident daemon's
        // writes. In the default rollback-journal mode a `run` daemon rewriting
        // the snapshot holds an exclusive lock, and any agent doing `status` /
        // `modules` / `orbit` mid-reindex hits "database is locked" — a hard error
        // that makes agents give up on the tool and fall back to grep. WAL lets
        // readers see the last committed snapshot while the writer works. NORMAL
        // synchronous is WAL's safe companion (no corruption risk; only the most
        // recent commit is at risk on power loss), and a busy timeout absorbs the
        // brief writer-writer and checkpoint contention WAL does not remove, so a
        // second writer waits rather than erroring. Best-effort: a filesystem that
        // cannot honour WAL (rare, e.g. some network mounts) leaves the connection
        // usable in its prior mode rather than failing the open.
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        let _ = connection.pragma_update(None, "synchronous", "NORMAL");
        let _ = connection.busy_timeout(std::time::Duration::from_secs(5));
        connection.execute_batch(MIGRATION)?;
        Ok(Self { connection })
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(MIGRATION)?;
        Ok(Self { connection })
    }

    pub fn replace_snapshot(&mut self, snapshot: &AtlasSnapshot) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM atlas_edges", [])?;
        transaction.execute("DELETE FROM atlas_entities", [])?;
        for entity in &snapshot.entities {
            transaction.execute(
                "INSERT INTO atlas_entities (id, scale, kind, canonical_name, parent_id, payload_json, snapshot_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![entity.id.to_string(), serde_json::to_value(entity.scale)?.as_str(), serde_json::to_value(entity.kind)?.as_str(), entity.canonical_name, entity.parent_id.map(|id| id.to_string()), serde_json::to_string(entity)?, snapshot.id.0],
            )?;
        }
        for edge in &snapshot.edges {
            transaction.execute(
                "INSERT INTO atlas_edges (id, source_id, target_id, relationship_kind, relationship_plane, required, payload_json, snapshot_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![edge.id.to_string(), edge.source_entity_id.to_string(), edge.target_entity_id.to_string(), serde_json::to_value(edge.kind)?.as_str(), serde_json::to_value(edge.plane)?.as_str(), edge.required, serde_json::to_string(edge)?, snapshot.id.0],
            )?;
        }
        transaction.execute("INSERT INTO metadata (key, value) VALUES ('current_snapshot', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&snapshot.id.0])?;
        transaction.execute("INSERT INTO metadata (key, value) VALUES ('root_id', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [snapshot.entities.first().map(|e| e.id).unwrap_or_else(|| EntityId(uuid::Uuid::nil())).to_string()])?;
        transaction.commit()?;
        Ok(())
    }

    /// Write one folder's entities and edges into the store, stamped with the
    /// snapshot they belong to. `INSERT OR REPLACE` keyed on id, so re-indexing a
    /// folder overwrites its own rows without touching others, and the shared
    /// ancestors (Universe, Galaxy) that every folder carries converge on one row
    /// rather than conflicting. This is the layered writer: the store is filled a
    /// folder at a time and never emptied mid-scan, so nothing whole is ever held.
    pub fn upsert_subtree(
        &mut self,
        entities: &[AtlasEntity],
        edges: &[AtlasEdge],
        snapshot_id: &SnapshotId,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        for entity in entities {
            transaction.execute(
                "INSERT OR REPLACE INTO atlas_entities (id, scale, kind, canonical_name, parent_id, payload_json, snapshot_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![entity.id.to_string(), serde_json::to_value(entity.scale)?.as_str(), serde_json::to_value(entity.kind)?.as_str(), entity.canonical_name, entity.parent_id.map(|id| id.to_string()), serde_json::to_string(entity)?, snapshot_id.0],
            )?;
        }
        for edge in edges {
            transaction.execute(
                "INSERT OR REPLACE INTO atlas_edges (id, source_id, target_id, relationship_kind, relationship_plane, required, payload_json, snapshot_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![edge.id.to_string(), edge.source_entity_id.to_string(), edge.target_entity_id.to_string(), serde_json::to_value(edge.kind)?.as_str(), serde_json::to_value(edge.plane)?.as_str(), edge.required, serde_json::to_string(edge)?, snapshot_id.0],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Delete every row not stamped with `snapshot_id`, then record the snapshot as
    /// current and `root` as its Universe entity. Run once after all folders are
    /// written: it removes the rows of folders (or files) that vanished since the
    /// previous scan, so the store ends holding exactly the new snapshot without a
    /// whole-database delete up front.
    pub fn finalize_snapshot(
        &mut self,
        snapshot_id: &SnapshotId,
        root: EntityId,
    ) -> Result<(), StoreError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM atlas_edges WHERE snapshot_id <> ?1",
            [&snapshot_id.0],
        )?;
        transaction.execute(
            "DELETE FROM atlas_entities WHERE snapshot_id <> ?1",
            [&snapshot_id.0],
        )?;
        transaction.execute(
            "INSERT INTO metadata (key, value) VALUES ('current_snapshot', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [&snapshot_id.0],
        )?;
        transaction.execute(
            "INSERT INTO metadata (key, value) VALUES ('root_id', ?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [root.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Load one planet's descendants — its files (direct children) and their
    /// symbols (grandchildren) — plus the edges sourced from the planet or any of
    /// those files, all stamped with `snapshot_id`. This is the bounded input the
    /// cross-folder socket stitch re-evaluates: one planet subtree at a time, never
    /// the whole tree. The planet entity itself is NOT returned (the caller already
    /// holds it); the returned entities are files + symbols only.
    pub fn load_planet_subtree(
        &self,
        planet_id: EntityId,
        snapshot_id: &SnapshotId,
    ) -> Result<(Vec<AtlasEntity>, Vec<AtlasEdge>), StoreError> {
        // Files: direct children of the planet.
        let mut file_statement = self.connection.prepare(
            "SELECT payload_json FROM atlas_entities WHERE parent_id = ?1 AND snapshot_id = ?2 ORDER BY id",
        )?;
        let files: Vec<AtlasEntity> = file_statement
            .query_map(params![planet_id.to_string(), snapshot_id.0], |row| {
                row.get::<_, String>(0)
            })?
            .map(|value| Ok(serde_json::from_str(&value?)?))
            .collect::<Result<Vec<AtlasEntity>, StoreError>>()?;

        // Symbols: children of any of those files.
        let mut entities = files.clone();
        for file in &files {
            let mut symbol_statement = self.connection.prepare(
                "SELECT payload_json FROM atlas_entities WHERE parent_id = ?1 AND snapshot_id = ?2 ORDER BY id",
            )?;
            let symbols = symbol_statement
                .query_map(params![file.id.to_string(), snapshot_id.0], |row| {
                    row.get::<_, String>(0)
                })?
                .map(|value| Ok(serde_json::from_str(&value?)?))
                .collect::<Result<Vec<AtlasEntity>, StoreError>>()?;
            entities.extend(symbols);
        }

        // Edges sourced from the planet or any of its files.
        let mut sources: Vec<String> = vec![planet_id.to_string()];
        sources.extend(files.iter().map(|f| f.id.to_string()));
        let mut edges: Vec<AtlasEdge> = Vec::new();
        for source in &sources {
            let mut edge_statement = self.connection.prepare(
                "SELECT payload_json FROM atlas_edges WHERE source_id = ?1 AND snapshot_id = ?2 ORDER BY id",
            )?;
            let mut some = edge_statement
                .query_map(params![source, snapshot_id.0], |row| row.get::<_, String>(0))?
                .map(|value| Ok(serde_json::from_str(&value?)?))
                .collect::<Result<Vec<AtlasEdge>, StoreError>>()?;
            edges.append(&mut some);
        }

        Ok((entities, edges))
    }

    /// The id of the snapshot currently recorded as live, or `None` when the store
    /// holds no snapshot yet. A single metadata read — the layered scan needs the
    /// previous snapshot's id to reconcile identity per directory without loading
    /// the whole prior snapshot into memory.
    pub fn current_snapshot_id(&self) -> Result<Option<SnapshotId>, StoreError> {
        let id: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id.map(SnapshotId))
    }

    /// Light per-entity `(scale, id-string, overall GreenCode)` for the current
    /// snapshot — the input to the texture projection. Reads only three small
    /// columns per row (never the heavy `AtlasEntity`), so a caller can sort by
    /// `(scale.rank(), id)` — the idx encode order — without materializing the
    /// snapshot. `green` = null (unevaluated) reads back as `Unknown`.
    pub fn entity_green_codes(&self) -> Result<Vec<(ScopeScale, String, GreenCode)>, StoreError> {
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(vec![]);
        };
        let mut statement = self.connection.prepare(
            "SELECT scale, id, json_extract(payload_json,'$.green.overall') \
             FROM atlas_entities WHERE snapshot_id = ?1",
        )?;
        let rows = statement.query_map(params![snapshot], |row| {
            let scale: String = row.get(0)?;
            let id: String = row.get(1)?;
            let code: Option<String> = row.get(2)?;
            Ok((scale, id, code))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (scale, id, code) = row?;
            let scale: ScopeScale = serde_json::from_value(serde_json::Value::String(scale))?;
            let code = match code.as_deref() {
                Some("green") => GreenCode::Green,
                Some("yellow") => GreenCode::Yellow,
                Some("red") => GreenCode::Red,
                _ => GreenCode::Unknown,
            };
            out.push((scale, id, code));
        }
        Ok(out)
    }

    /// Like `entity_green_codes`, but also returns each entity's `parent_id`
    /// (SQL NULL → `None`), so a caller can rebuild the tree for the galaxy
    /// layout. Light columns only — never the heavy `AtlasEntity`.
    pub fn entity_tree_nodes(
        &self,
    ) -> Result<Vec<(String, Option<String>, ScopeScale, GreenCode)>, StoreError> {
        self.entity_tree_nodes_for_axis(None)
    }

    /// As [`Self::entity_tree_nodes`], but coloured by a single Green axis instead
    /// of `overall`. `axis` is a snake_case axis name (`integration`,
    /// `verification`, …); `None` uses `overall`. Lets the galaxy show, say, the
    /// compiler's verdict (the Integration axis) directly. Only the fixed axis
    /// keys are accepted, so the interpolated JSON path is not user-controlled.
    pub fn entity_tree_nodes_for_axis(
        &self,
        axis: Option<&str>,
    ) -> Result<Vec<(String, Option<String>, ScopeScale, GreenCode)>, StoreError> {
        const AXES: &[&str] = &[
            "content",
            "structure",
            "integration",
            "verification",
            "freshness",
            "coherence",
        ];
        let path = match axis {
            Some(a) if AXES.contains(&a) => format!("$.green.axes.{a}.code"),
            _ => "$.green.overall".to_string(),
        };
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(vec![]);
        };
        let sql = format!(
            "SELECT id, parent_id, scale, json_extract(payload_json,'{path}') \
             FROM atlas_entities WHERE snapshot_id = ?1"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params![snapshot], |row| {
            let id: String = row.get(0)?;
            let parent: Option<String> = row.get(1)?;
            let scale: String = row.get(2)?;
            let code: Option<String> = row.get(3)?;
            Ok((id, parent, scale, code))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, parent, scale, code) = row?;
            let scale: ScopeScale = serde_json::from_value(serde_json::Value::String(scale))?;
            let code = match code.as_deref() {
                Some("green") => GreenCode::Green,
                Some("yellow") => GreenCode::Yellow,
                Some("red") => GreenCode::Red,
                _ => GreenCode::Unknown,
            };
            out.push((id, parent, scale, code));
        }
        Ok(out)
    }

    /// The dependency edges of the current snapshot as `(source_id, target_id)`
    /// pairs — the relationships that pull related code together in the galaxy
    /// force layout. Containment (`Contains`) is excluded: it is already carried
    /// by each entity's `parent_id`, so the layout derives it there and this
    /// returns only the cross-tree dependency graph (imports, calls, and the
    /// like). Rows come back ordered so the layout that consumes them stays
    /// deterministic.
    pub fn entity_edges(&self) -> Result<Vec<(String, String)>, StoreError> {
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(vec![]);
        };
        let mut statement = self.connection.prepare(
            "SELECT source_id, target_id FROM atlas_edges \
             WHERE snapshot_id = ?1 AND relationship_kind != 'contains' \
             ORDER BY source_id, target_id",
        )?;
        let rows = statement.query_map(params![snapshot], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// The ids of entities that are *support* — data or docs — for the current
    /// snapshot, so the galaxy renderer draws them as dust rather than code.
    /// Two signals: extension/path (data/doc extensions — mirrors the `modules`
    /// overview), and — the stronger one — **no parsed code symbols**: a File that
    /// produced no Function/Type/Component/Test child is a data or binary file
    /// whatever its extension, which is how a corpus of unrecognised extensions
    /// (0 symbols) gets classified as data instead of masquerading as code.
    pub fn support_entity_ids(&self) -> Result<std::collections::BTreeSet<String>, StoreError> {
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(Default::default());
        };
        let mut statement = self.connection.prepare(
            "SELECT id, parent_id, kind, json_extract(payload_json,'$.relative_path') \
             FROM atlas_entities WHERE snapshot_id = ?1",
        )?;
        let rows: Vec<(String, Option<String>, EntityKind, Option<String>)> = statement
            .query_map(params![snapshot], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .map(|r| {
                let (id, parent, kind, path) = r?;
                let kind: EntityKind =
                    serde_json::from_value(serde_json::Value::String(kind))?;
                Ok((id, parent, kind, path))
            })
            .collect::<Result<_, StoreError>>()?;

        // Which entities have at least one parsed code-symbol child — the mark of
        // an actual source file.
        let mut has_code_child: std::collections::BTreeSet<&str> = Default::default();
        for (_, parent, kind, _) in &rows {
            if matches!(
                kind,
                EntityKind::Function
                    | EntityKind::Type
                    | EntityKind::Component
                    | EntityKind::Test
            ) {
                if let Some(p) = parent {
                    has_code_child.insert(p.as_str());
                }
            }
        }

        let mut out = std::collections::BTreeSet::new();
        for (id, _, kind, path) in &rows {
            let by_ext = crate::texture::force::entity_is_support(*kind, path.as_deref());
            let no_symbols = matches!(kind, EntityKind::File | EntityKind::Resource)
                && !has_code_child.contains(id.as_str());
            if by_ext || no_symbols {
                out.insert(id.clone());
            }
        }
        Ok(out)
    }

    /// The ids of documentation entities (by extension) for the current snapshot —
    /// the readable layer, a distinct role from bulk data, drawn as a dim haze.
    pub fn doc_entity_ids(&self) -> Result<std::collections::BTreeSet<String>, StoreError> {
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(Default::default());
        };
        let mut statement = self.connection.prepare(
            "SELECT id, kind, json_extract(payload_json,'$.relative_path') \
             FROM atlas_entities WHERE snapshot_id = ?1",
        )?;
        let rows = statement.query_map(params![snapshot], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut out = std::collections::BTreeSet::new();
        for row in rows {
            let (id, kind, path) = row?;
            let kind: EntityKind = serde_json::from_value(serde_json::Value::String(kind))?;
            if crate::texture::force::entity_is_docs(kind, path.as_deref()) {
                out.insert(id);
            }
        }
        Ok(out)
    }

    /// Ids of agent/tooling *meta* entities (`.agents/`, `.claude/`, `.github/`,
    /// `.cursor/`, and root meta files) — excluded from the galaxy render. Matched
    /// by path, so every descendant of a meta directory is included. Mirrors
    /// `entity_is_meta`; the caller drops these nodes and any edge touching them.
    pub fn meta_entity_ids(&self) -> Result<std::collections::BTreeSet<String>, StoreError> {
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(Default::default());
        };
        let mut statement = self.connection.prepare(
            "SELECT id, json_extract(payload_json,'$.relative_path') \
             FROM atlas_entities WHERE snapshot_id = ?1",
        )?;
        let rows = statement.query_map(params![snapshot], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut out = std::collections::BTreeSet::new();
        for row in rows {
            let (id, path) = row?;
            if crate::texture::force::entity_is_meta(path.as_deref()) {
                out.insert(id);
            }
        }
        Ok(out)
    }

    /// Each entity's age as a Unix timestamp, read straight from the filesystem
    /// (the "Date Created" the Finder shows — `birthtime` on macOS, falling back to
    /// modified time). Keyed by entity id, for the galaxy's arm ordering (oldest at
    /// the core, newest at the fingertips). Entities with no path, or files that
    /// cannot be stat'd, are simply omitted — the layout falls back to tree order
    /// for those. `root` is the project directory the relative paths hang off.
    pub fn entity_ages(
        &self,
        root: &std::path::Path,
    ) -> Result<std::collections::BTreeMap<String, i64>, StoreError> {
        let snapshot: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(Default::default());
        };
        let mut statement = self.connection.prepare(
            "SELECT id, json_extract(payload_json,'$.relative_path') \
             FROM atlas_entities WHERE snapshot_id = ?1",
        )?;
        let rows = statement.query_map(params![snapshot], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut out = std::collections::BTreeMap::new();
        for row in rows {
            let (id, path) = row?;
            let Some(path) = path else { continue };
            let Ok(meta) = std::fs::metadata(root.join(&path)) else {
                continue;
            };
            let time = meta.created().or_else(|_| meta.modified());
            if let Ok(t) = time {
                if let Ok(dur) = t.duration_since(std::time::UNIX_EPOCH) {
                    out.insert(id, dur.as_secs() as i64);
                }
            }
        }
        Ok(out)
    }

    /// The number of entities and edges currently stored. Cheap COUNT queries, so
    /// a caller can report totals without materializing the whole snapshot.
    pub fn counts(&self) -> Result<(usize, usize), StoreError> {
        let entities: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM atlas_entities", [], |row| row.get(0))?;
        let edges: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM atlas_edges", [], |row| row.get(0))?;
        Ok((entities as usize, edges as usize))
    }

    pub fn load_snapshot(&self) -> Result<AtlasSnapshot, StoreError> {
        let snapshot_id: String = self
            .connection
            .query_row(
                "SELECT value FROM metadata WHERE key='current_snapshot'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::Empty)?;

        let mut entity_statement = self.connection.prepare(
            "SELECT payload_json FROM atlas_entities ORDER BY scale, canonical_name, id",
        )?;
        let entities = entity_statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|value| Ok(serde_json::from_str(&value?)?))
            .collect::<Result<Vec<AtlasEntity>, StoreError>>()?;
        let mut edge_statement = self.connection.prepare("SELECT payload_json FROM atlas_edges ORDER BY relationship_kind, source_id, target_id, id")?;
        let edges = edge_statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|value| Ok(serde_json::from_str(&value?)?))
            .collect::<Result<Vec<AtlasEdge>, StoreError>>()?;
        Ok(AtlasSnapshot {
            id: SnapshotId(snapshot_id),
            timestamp: "2026-08-03T00:00:00Z".to_string(),
            entities,
            edges,
            records: vec![],
            conflicts: vec![],
            sources: vec![],
        })
    }
}
