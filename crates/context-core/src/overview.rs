//! Tree-ordered, role-tagged module map — the readable overview an AI reads to
//! model "what is this repo." It walks the atlas hierarchy in tree order (not a
//! file-count leaderboard) and tags each folder module with a **role** — code,
//! data, docs — so a corpus or an export is findable and in its structural place,
//! but never weighted like source. Computed at read-time from entity kinds and
//! paths; it does not change the scan or its determinism.
//!
//! The signal that a module is *code* is the presence of parsed **symbols**
//! (functions, types): the parser only emits them for source it understands, so a
//! corpus of `.jsonl` has none. Data extensions with no symbols read as `data`.

use creature_context_types::{AtlasEntity, AtlasSnapshot, EntityId, EntityKind, ScopeScale};
use std::collections::BTreeMap;

/// What a folder module mostly holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleRole {
    Code,
    Data,
    Docs,
}

impl ModuleRole {
    pub fn label(self) -> &'static str {
        match self {
            ModuleRole::Code => "code",
            ModuleRole::Data => "data",
            ModuleRole::Docs => "docs",
        }
    }
}

/// One line of the module map.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleLine {
    pub depth: usize,
    pub name: String,
    pub path: String,
    pub role: ModuleRole,
    /// Descendant file entities (files + resources + tests).
    pub files: usize,
    /// Descendant parsed symbols (functions, types, components).
    pub symbols: usize,
}

const DATA_EXTS: &[&str] = &[
    "jsonl", "ndjson", "csv", "tsv", "parquet", "arrow", "npy", "db", "sqlite", "sqlite3", "dat",
    "bin", "json", "yaml", "yml", "toml", "png", "jpg", "jpeg", "gif", "svg", "webp",
];
const DOC_EXTS: &[&str] = &["md", "rst", "adoc", "mdx"];

fn ext_of(path: &str) -> Option<&str> {
    path.rsplit('/').next()?.rsplit_once('.').map(|(_, e)| e)
}

/// Category counts a single entity contributes: `(code, data, docs)`.
fn categorize(kind: EntityKind, path: &str) -> (u64, u64, u64) {
    match kind {
        // Parsed symbols are the strongest code signal.
        EntityKind::Function | EntityKind::Type | EntityKind::Component | EntityKind::Test => {
            (1, 0, 0)
        }
        EntityKind::Resource => match ext_of(path) {
            Some(e) if DOC_EXTS.contains(&e) => (0, 0, 1),
            _ => (0, 1, 0),
        },
        EntityKind::File => match ext_of(path) {
            Some(e) if DOC_EXTS.contains(&e) => (0, 0, 1),
            Some(e) if DATA_EXTS.contains(&e) => (0, 1, 0),
            _ => (1, 0, 0), // a source file with a code-ish extension
        },
        // Container scales carry no leaf signal of their own.
        _ => (0, 0, 0),
    }
}

/// Pick a module's role from its aggregated `(code, data, docs)` signal. Code wins
/// ties: when a module holds real source, that is what an agent should navigate.
pub fn role_from_signal(code: u64, data: u64, docs: u64) -> ModuleRole {
    if code >= data && code >= docs {
        ModuleRole::Code
    } else if data >= docs {
        ModuleRole::Data
    } else {
        ModuleRole::Docs
    }
}

/// Is this a folder-level module (a node the overview lists)?
fn is_module_scale(scale: ScopeScale) -> bool {
    matches!(scale, ScopeScale::Galaxy | ScopeScale::System | ScopeScale::Planet)
}

struct Agg {
    code: u64,
    data: u64,
    docs: u64,
    files: u64,
    symbols: u64,
}

/// Build the tree-ordered, role-tagged module map. Folder modules (Galaxy →
/// System → Planet) are emitted in depth-first hierarchy order; each carries the
/// role and counts aggregated from all its descendants.
pub fn module_overview(snapshot: &AtlasSnapshot) -> Vec<ModuleLine> {
    // children by parent, in stable path/name order for deterministic tree order.
    let mut children: BTreeMap<EntityId, Vec<&AtlasEntity>> = BTreeMap::new();
    let mut root: Option<&AtlasEntity> = None;
    for entity in &snapshot.entities {
        match entity.parent_id {
            Some(parent) => children.entry(parent).or_default().push(entity),
            None => {
                if root.is_none() || entity.scale.rank() < root.unwrap().scale.rank() {
                    root = Some(entity);
                }
            }
        }
    }
    for kids in children.values_mut() {
        kids.sort_by(|a, b| {
            let ap = a.relative_path.as_deref().unwrap_or(&a.canonical_name);
            let bp = b.relative_path.as_deref().unwrap_or(&b.canonical_name);
            ap.cmp(bp).then(a.id.cmp(&b.id))
        });
    }

    // Bottom-up aggregation, memoized, so each subtree is summed once.
    let mut agg: BTreeMap<EntityId, Agg> = BTreeMap::new();
    fn aggregate<'a>(
        node: &'a AtlasEntity,
        children: &BTreeMap<EntityId, Vec<&'a AtlasEntity>>,
        memo: &mut BTreeMap<EntityId, Agg>,
    ) -> (u64, u64, u64, u64, u64) {
        let path = node.relative_path.as_deref().unwrap_or("");
        let (mut code, mut data, mut docs) = categorize(node.kind, path);
        let mut files = matches!(
            node.kind,
            EntityKind::File | EntityKind::Resource | EntityKind::Test
        ) as u64;
        let mut symbols = matches!(
            node.kind,
            EntityKind::Function | EntityKind::Type | EntityKind::Component
        ) as u64;
        if let Some(kids) = children.get(&node.id) {
            for child in kids {
                let (c, d, o, f, s) = aggregate(child, children, memo);
                code += c;
                data += d;
                docs += o;
                files += f;
                symbols += s;
            }
        }
        memo.insert(
            node.id,
            Agg {
                code,
                data,
                docs,
                files,
                symbols,
            },
        );
        (code, data, docs, files, symbols)
    }
    let Some(root) = root else {
        return vec![];
    };
    aggregate(root, &children, &mut agg);

    // Depth-first emit of the folder modules.
    let mut out = Vec::new();
    fn walk<'a>(
        node: &'a AtlasEntity,
        depth: usize,
        children: &BTreeMap<EntityId, Vec<&'a AtlasEntity>>,
        agg: &BTreeMap<EntityId, Agg>,
        out: &mut Vec<ModuleLine>,
    ) {
        if is_module_scale(node.scale) {
            let a = &agg[&node.id];
            out.push(ModuleLine {
                depth,
                name: node.canonical_name.clone(),
                path: node.relative_path.clone().unwrap_or_default(),
                role: role_from_signal(a.code, a.data, a.docs),
                files: a.files as usize,
                symbols: a.symbols as usize,
            });
        }
        let next_depth = if is_module_scale(node.scale) {
            depth + 1
        } else {
            depth
        };
        if let Some(kids) = children.get(&node.id) {
            for child in kids {
                if is_module_scale(child.scale) {
                    walk(child, next_depth, children, agg, out);
                }
            }
        }
    }
    walk(root, 0, &children, &agg, &mut out);
    out
}

/// A module is worth showing on the map when it holds real code, sits at the top
/// of the tree (structural landmark), or is a sizeable data/docs store. This keeps
/// the map a summary — the whole point — instead of every subdirectory.
pub fn is_notable(line: &ModuleLine, min_symbols: usize, min_files: usize) -> bool {
    line.depth <= 1 || line.symbols >= min_symbols || line.files >= min_files
}

/// Render the module map as tree-ordered text: full path, role, and counts as
/// trailing annotations (never the sort key). Only notable modules are shown.
pub fn render_overview(lines: &[ModuleLine]) -> String {
    const MIN_SYMBOLS: usize = 40;
    const MIN_FILES: usize = 150;
    let mut out = String::new();
    for line in lines {
        if !is_notable(line, MIN_SYMBOLS, MIN_FILES) {
            continue;
        }
        let path = if line.path.is_empty() { "." } else { &line.path };
        out.push_str(&format!(
            "{path}  [{role}]  {sym} symbols / {files} files\n",
            role = line.role.label(),
            sym = line.symbols,
            files = line.files,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_wins_ties_and_symbols_signal_code() {
        assert_eq!(role_from_signal(5, 5, 0), ModuleRole::Code);
        assert_eq!(role_from_signal(0, 10, 0), ModuleRole::Data);
        assert_eq!(role_from_signal(0, 0, 3), ModuleRole::Docs);
        assert_eq!(role_from_signal(0, 2, 3), ModuleRole::Docs);
    }

    #[test]
    fn categorize_by_kind_and_extension() {
        assert_eq!(categorize(EntityKind::Function, "a/f.rs"), (1, 0, 0));
        assert_eq!(categorize(EntityKind::File, "a/b.rs"), (1, 0, 0));
        assert_eq!(categorize(EntityKind::File, "data/x.jsonl"), (0, 1, 0));
        assert_eq!(categorize(EntityKind::File, "docs/readme.md"), (0, 0, 1));
        assert_eq!(categorize(EntityKind::Resource, "a/pic.png"), (0, 1, 0));
        assert_eq!(categorize(EntityKind::Resource, "a/notes.md"), (0, 0, 1));
    }
}
