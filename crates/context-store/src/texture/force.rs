//! Galaxy layout as a tree-based radial spiral — each top-level directory is an
//! arm. A force-directed sim was tried first, but it needs a rich dependency
//! graph to pull modules into arms, and the atlas's is sparse (data corpora have
//! no edges at all), so disconnected directories just drifted. The tree is always
//! complete, so the arms are drawn from it: every directory (a System) gets an
//! evenly-spaced angle, and its subtree fills a spiral wedge — radius by the
//! node's index within the arm, a twist that winds the wedge, a jitter for width.
//!
//! **Determinism.** The tool's invariant is byte-identical output on every OS,
//! and this honours it: no RNG, no clocks, fixed traversal order, and only
//! `+ − × ÷ sqrt`. Angles never call runtime trig — they resolve through a fixed
//! unit-circle lookup built once by repeated rotation with constant `cos`/`sin`
//! literals. Same nodes → same pixels.

use crate::texture::color;
use crate::texture::png;
use creature_context_types::{EntityKind, GreenCode, ScopeScale};
use std::collections::{BTreeMap, BTreeSet};

/// Data and documentation extensions — the same lists the `modules` overview uses
/// to tell support files from code, kept here so context-store need not depend on
/// context-core. A file with one of these extensions is support, not source.
const SUPPORT_EXTS: &[&str] = &[
    "jsonl", "ndjson", "csv", "tsv", "parquet", "arrow", "npy", "db", "sqlite", "sqlite3", "dat",
    "bin", "json", "yaml", "yml", "toml", "png", "jpg", "jpeg", "gif", "svg", "webp", "md", "rst",
    "adoc", "mdx",
];
/// Documentation extensions — the readable layer describing the code, a distinct
/// role from bulk data (drawn as a dim haze, not dust).
const DOC_EXTS: &[&str] = &["md", "rst", "adoc", "mdx", "txt"];

/// Whether an entity is documentation (by extension). Docs are support (non-code)
/// but their own role: readable, meaningful, drawn distinctly from data dust.
pub fn entity_is_docs(kind: EntityKind, path: Option<&str>) -> bool {
    matches!(kind, EntityKind::File | EntityKind::Resource)
        && matches!(ext_of(path.unwrap_or("")), Some(e) if DOC_EXTS.contains(&e))
}
/// Path fragments that mark agent/tooling *meta* — files that describe or tool the
/// project rather than being the product (agent instructions, editor/CI config).
/// These are excluded from the galaxy entirely: they aren't Rosetta, and drawing
/// their hundreds of files in the code arm would drown the product it's meant to
/// map. A path segment match (leading + trailing `/`) so only the directory, not a
/// same-named symbol, counts.
const META_DIR_MARKERS: &[&str] = &["/.agents/", "/.claude/", "/.github/", "/.cursor/"];
/// Root-level meta files (matched by basename) that live outside a meta directory.
const META_FILE_NAMES: &[&str] = &["agents.md", "claude.md", "copilot-instructions.md"];

/// Whether an entity is agent/tooling meta — excluded from the render. Matched by
/// path so every descendant of a meta directory (a `.py` deep in `.agents/`) is
/// caught, plus a handful of root-level meta files by name. Path-only, kind-
/// agnostic, and deterministic.
pub fn entity_is_meta(path: Option<&str>) -> bool {
    let Some(p) = path else { return false };
    let lower = p.to_ascii_lowercase();
    // Normalise to always have a leading slash so a top-level `.agents/...` matches
    // the `/.agents/` segment marker without a special leading case.
    let framed = format!("/{}", lower.trim_start_matches('/'));
    if META_DIR_MARKERS.iter().any(|m| framed.contains(m)) {
        return true;
    }
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    META_FILE_NAMES.contains(&base)
}

/// Path fragments that mark vendored / build-output / library code — desaturated
/// even when the files themselves parse as source, so a bundled dependency reads
/// as a dim cloud rather than a bright core arm.
const VENDOR_MARKERS: &[&str] = &[
    "node_modules",
    "vendor/",
    "third_party",
    "/target/",
    ".build",
    "deriveddata",
    ".noindex",
    "checkouts",
    "/deps/",
    "/dist/",
];

fn ext_of(path: &str) -> Option<&str> {
    path.rsplit('/').next()?.rsplit_once('.').map(|(_, e)| e)
}

/// Deterministic integer hash → [0,1). No RNG; same bytes on every platform.
fn hash01(x: i64, y: i64) -> f64 {
    let mut h = (x.wrapping_mul(374_761_393).wrapping_add(y.wrapping_mul(668_265_263))) as u64;
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xffff) as f64 / 65535.0
}

/// Value noise (smooth-interpolated hash lattice) → ~[0,1]. The building block for
/// organic clouds: modulating a nebula's brightness by fractal value noise breaks
/// the perfect disc into wisps and lumps, so it reads as gas, not a circle.
fn value_noise(fx: f64, fy: f64) -> f64 {
    let (x0, y0) = (fx.floor(), fy.floor());
    let (ix, iy) = (x0 as i64, y0 as i64);
    let (tx, ty) = (fx - x0, fy - y0);
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let n00 = hash01(ix, iy);
    let n10 = hash01(ix + 1, iy);
    let n01 = hash01(ix, iy + 1);
    let n11 = hash01(ix + 1, iy + 1);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sy
}

/// Two octaves of value noise, centred near 1.0, as a cloud density multiplier.
fn cloud_density(px: f64, py: f64) -> f32 {
    let n = 0.6 * value_noise(px / 34.0, py / 34.0) + 0.4 * value_noise(px / 13.0, py / 13.0);
    (n * 1.7) as f32
}

/// Whether a leaf entity is *support* (vendored/library, data, or docs) rather
/// than core code. Folders inherit support-ness from the share of their
/// descendant leaves that are support, so a library or docs context is drawn as a
/// dim, desaturated nebula instead of a saturated one. Mirrors the `modules`
/// role rules (data/doc extensions), plus vendored path markers.
pub fn entity_is_support(kind: EntityKind, path: Option<&str>) -> bool {
    if let Some(p) = path {
        let lower = p.to_ascii_lowercase();
        if VENDOR_MARKERS.iter().any(|m| lower.contains(m)) {
            return true;
        }
    }
    match kind {
        EntityKind::Function | EntityKind::Type | EntityKind::Component | EntityKind::Test => false,
        EntityKind::Resource => true,
        EntityKind::File => matches!(ext_of(path.unwrap_or("")), Some(e) if SUPPORT_EXTS.contains(&e)),
        _ => false,
    }
}

/// Golden-angle rotation `(cos, sin)` literals — used by the data-corpus dust
/// scatter (a phyllotaxis fill) so it needs no runtime trig.
const GA_COS: f64 = -0.737_368_878_078_319_7;
const GA_SIN: f64 = 0.675_490_294_261_523_8;

/// Tree-based radial arms. A fixed unit-circle lookup of `CIRCLE_N` samples is
/// built by repeated rotation (constants below are cos/sin of one 1/CIRCLE_N
/// turn), so an arbitrary angle resolves to a point with NO runtime trig —
/// keeping the byte-identical determinism invariant. `RADIAL_STEP` is the radius
/// added per tree depth; `ARM_TWIST` rotates each depth a little so the wedges
/// wind into spiral arms.
const CIRCLE_N: usize = 2048;
const CIRCLE_COS: f64 = 0.999_995_293_809_576_1;
const CIRCLE_SIN: f64 = 0.003_067_956_762_965_976;
const RADIAL_STEP: f64 = 100.0;
/// How many times the single arm winds from core to rim, and how thick the band
/// is relative to its radius (it widens outward, like a real spiral arm).
const ARM_TURNS: f64 = 1.5;
const ARM_WIDTH: f64 = 0.45;

/// Context-nebula thresholds: a grouping must hold at least this many files to be
/// a context worth drawing; radius is this multiple of the members' RMS spread,
/// with a floor so a tight cluster still reads as a cloud.
const MIN_NEBULA_MASS: f64 = 3.0;
const NEBULA_SPREAD: f64 = 1.7;
const NEBULA_FLOOR: f64 = 6.0;
/// A nebula never spans more than this fraction of the galaxy's radius, so a
/// context whose members stretch down a long arm can't wash the whole frame.
const NEBULA_MAX_FRAC: f64 = 0.22;
/// Above this many nodes the O(n²) sim is too slow and one-dot-per-file is
/// unreadable anyway, so the layout collapses to a coarser zoom level (folders as
/// stars) until it fits. Sized so a mid-size repo still plots every file, but a
/// large workspace renders at folder granularity in seconds.
const MAX_PLOT_NODES: usize = 6000;
/// Opaque "space" the galaxy sits on — nebulae only glow against a dark ground.
const SPACE: [u8; 4] = [8, 10, 16, 255];
/// Peak brightness a single nebula adds to the background, before overlaps stack.
const NEBULA_GAIN: f64 = 0.42;

/// A placed star: absolute center, mass (descendant leaf count, for the core
/// glow), health, and whether it is a leaf (a bright cell) or a folder (a dim
/// gravity well drawn faintly behind). Draw size is derived from `mass` in pixels
/// at render time, not from the layout's own units — the galaxy spreads over a
/// large area, so a layout-unit radius would rasterize to a single pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    pub x: f64,
    pub y: f64,
    pub mass: f64,
    pub code: GreenCode,
    pub is_leaf: bool,
    /// Mostly support (data/docs). Laid out (still shapes the physics) but its
    /// leaf-stars are not drawn — the context is shown as dust or haze instead.
    pub is_support: bool,
    /// Mostly documentation — the readable layer, drawn as a dim haze.
    pub is_docs: bool,
}

/// A context grouping drawn as a soft glowing cloud: center, radius (the spatial
/// spread of its member files), and a hue that identifies *which* context it is —
/// not its health. Different context → different colour; no context grouping →
/// no nebula, so empty space stays empty. Stars carry health on top; nebulae
/// carry identity underneath.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nebula {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub color: [u8; 3],
    /// A support (non-code) context — data or docs — not a solid code nebula.
    pub is_support: bool,
    /// A documentation context — drawn as a dim haze; data (is_support && !is_docs)
    /// is drawn as an asteroid-belt dust field.
    pub is_docs: bool,
}

/// The laid-out galaxy: bright health-colored stars (files) over identity-colored
/// context nebulae (groupings).
#[derive(Clone, Debug, PartialEq)]
pub struct Galaxy {
    pub stars: Vec<Star>,
    pub nebulae: Vec<Nebula>,
}

/// Leaf stars only — a thin wrapper over [`galaxy`] for callers (and tests) that
/// want just the file cells, not the context nebulae.
pub fn galaxy_layout(
    nodes: &[(String, Option<String>, ScopeScale, GreenCode)],
    edges: &[(String, String)],
) -> Vec<Star> {
    galaxy_with_support(nodes, edges, &BTreeSet::new(), &BTreeSet::new(), &BTreeMap::new()).stars
}

/// [`galaxy`] with no support classification — every context is drawn saturated.
pub fn galaxy(
    nodes: &[(String, Option<String>, ScopeScale, GreenCode)],
    edges: &[(String, String)],
) -> Galaxy {
    galaxy_with_support(nodes, edges, &BTreeSet::new(), &BTreeSet::new(), &BTreeMap::new())
}

/// Run the simulation and return the laid-out [`Galaxy`]: leaf files as
/// health-colored stars, and each context grouping as an identity-colored
/// [`Nebula`]. `nodes` are `(id, parent_id, scale, code)` rows (order irrelevant —
/// sorted internally for determinism); `edges` are `(source_id, target_id)`
/// dependency pairs (containment is derived from `parent_id`, so it need not
/// appear here); `support` is the set of node ids that are vendored/library, data
/// or docs — their context nebulae are drawn desaturated so support clusters read
/// as dim clouds, not bright core arms.
pub fn galaxy_with_support(
    nodes: &[(String, Option<String>, ScopeScale, GreenCode)],
    edges: &[(String, String)],
    support: &BTreeSet<String>,
    docs: &BTreeSet<String>,
    ages: &BTreeMap<String, i64>,
) -> Galaxy {
    let n = nodes.len();
    if n == 0 {
        return Galaxy {
            stars: vec![],
            nebulae: vec![],
        };
    }

    // Fixed body order: sort by (scale rank, id). Deterministic and stable.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        nodes[a]
            .2
            .rank()
            .cmp(&nodes[b].2.rank())
            .then_with(|| nodes[a].0.cmp(&nodes[b].0))
    });
    let index: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, &orig)| (nodes[orig].0.as_str(), i))
        .collect();
    // id by sorted position, so a plotted node can look up its age.
    let sorted_id: Vec<&str> = order.iter().map(|&o| nodes[o].0.as_str()).collect();

    let scale: Vec<ScopeScale> = order.iter().map(|&o| nodes[o].2).collect();
    let code_full: Vec<GreenCode> = order.iter().map(|&o| nodes[o].3).collect();
    let is_support_leaf: Vec<bool> = order.iter().map(|&o| support.contains(&nodes[o].0)).collect();
    let is_docs_leaf: Vec<bool> = order.iter().map(|&o| docs.contains(&nodes[o].0)).collect();
    let parent_of: Vec<Option<usize>> = order
        .iter()
        .map(|&o| nodes[o].1.as_deref().and_then(|p| index.get(p).copied()))
        .collect();

    // Full-tree children, then mass (descendant leaf count) and support-leaf count,
    // computed over EVERY node before any zoom collapse — so a folder keeps its
    // true file count and data share even when its files are not plotted.
    let mut children_full: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, p) in parent_of.iter().enumerate() {
        if let Some(p) = p {
            children_full[*p].push(i);
        }
    }
    let (mass_full, sup_full) = subtree_counts(&children_full, &is_support_leaf);
    let (_, docs_full) = subtree_counts(&children_full, &is_docs_leaf);

    // Scale-adaptive zoom. An O(n²) sim cannot plot tens of thousands of file
    // Moons, and this picture is to be *seen*, not read — so for a large atlas
    // collapse to the coarsest zoom level whose node count fits the budget: files
    // fold into their folders (folder mass = file count), leaving folders as stars
    // and systems as nebulae. Deeper zoom is the same renderer aimed at a subtree.
    // A small atlas (n ≤ budget) keeps every node, byte-identical to before.
    let cutoff = plot_cutoff_rank(&scale, n);
    let keep: Vec<bool> = (0..n).map(|i| scale[i].rank() <= cutoff).collect();
    let kept_ancestor: Vec<Option<usize>> = (0..n)
        .map(|i| {
            let mut cur = Some(i);
            while let Some(c) = cur {
                if keep[c] {
                    return Some(c);
                }
                cur = parent_of[c];
            }
            None
        })
        .collect();

    // Local index space over the kept (plotted) nodes, in the same sorted order.
    let plotted: Vec<usize> = (0..n).filter(|&i| keep[i]).collect();
    let local: BTreeMap<usize, usize> =
        plotted.iter().enumerate().map(|(l, &g)| (g, l)).collect();
    let n = plotted.len();

    let code: Vec<GreenCode> = plotted.iter().map(|&g| code_full[g]).collect();
    let mass: Vec<f64> = plotted.iter().map(|&g| mass_full[g]).collect();
    let support_frac: Vec<f64> = plotted
        .iter()
        .map(|&g| if mass_full[g] > 0.0 { sup_full[g] / mass_full[g] } else { 0.0 })
        .collect();
    let docs_frac: Vec<f64> = plotted
        .iter()
        .map(|&g| if mass_full[g] > 0.0 { docs_full[g] / mass_full[g] } else { 0.0 })
        .collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (l, &g) in plotted.iter().enumerate() {
        if let Some(gp) = parent_of[g] {
            if let Some(&lp) = local.get(&gp) {
                children[lp].push(l);
            }
        }
    }
    let is_leaf: Vec<bool> = children.iter().map(|c| c.is_empty()).collect();

    // Dependency degree per node — how many other nodes connect to it. Used to
    // centre the frame on the connectivity hub, not the biggest pile of files.
    let mut degree = vec![0.0f64; n];
    for (src, dst) in edges {
        let (Some(&gs), Some(&gd)) = (index.get(src.as_str()), index.get(dst.as_str())) else {
            continue;
        };
        let a = kept_ancestor[gs].and_then(|k| local.get(&k).copied());
        let b = kept_ancestor[gd].and_then(|k| local.get(&k).copied());
        if let (Some(a), Some(b)) = (a, b) {
            if a != b {
                degree[a] += 1.0;
                degree[b] += 1.0;
            }
        }
    }

    // Tree-based arms via a radial layout. Each top-level directory (a child of
    // the repo root) owns an angular wedge sized to its file count, and its subtree
    // fans out within that wedge — so every directory is its own arm, complete and
    // present whether or not it has dependency edges (the force layout had nothing
    // to arm the disconnected corpora out along). Depth sets the radius, a per-depth
    // twist winds the wedges into spirals, and it is deterministic: angles resolve
    // through a fixed unit-circle table built by repeated rotation, no runtime trig.
    let circle: Vec<(f64, f64)> = {
        let mut v = Vec::with_capacity(CIRCLE_N);
        let (mut cx, mut cy) = (1.0f64, 0.0f64);
        for _ in 0..CIRCLE_N {
            v.push((cx, cy));
            let (nx, ny) = (cx * CIRCLE_COS - cy * CIRCLE_SIN, cx * CIRCLE_SIN + cy * CIRCLE_COS);
            cx = nx;
            cy = ny;
        }
        v
    };
    let sample = |frac: f64| -> (f64, f64) {
        let i = (frac * CIRCLE_N as f64).round() as i64;
        circle[i.rem_euclid(CIRCLE_N as i64) as usize]
    };

    // ONE arm. A single project is one arm of a larger galaxy, not a whole
    // galaxy — so lay everything along one winding spiral curve, and give it a
    // perpendicular thickness that grows outward so it reads as a Milky-Way-like
    // band of clouds and dust, not a thin line. Displacement along the arm is
    // *age*: the oldest code sits at the core, the newest at the fingertips —
    // reading the arm outward is reading the codebase's history (just like a real
    // galaxy: old stars in the bulge, young stars in the arms). Ages come from the
    // caller (file creation dates); a node without one falls back to tree order.
    let arm_order: Vec<usize> = {
        // Tree order as the stable fallback / tie-break.
        let mut dfs: Vec<usize> = Vec::with_capacity(n);
        let roots: Vec<usize> = (0..n)
            .filter(|&l| match parent_of[plotted[l]] {
                Some(gp) => !local.contains_key(&gp),
                None => true,
            })
            .collect();
        let mut stack: Vec<usize> = roots.into_iter().rev().collect();
        while let Some(node) = stack.pop() {
            dfs.push(node);
            for &c in children[node].iter().rev() {
                stack.push(c);
            }
        }
        let dfs_rank: Vec<usize> = {
            let mut r = vec![0usize; n];
            for (rank, &node) in dfs.iter().enumerate() {
                r[node] = rank;
            }
            r
        };
        let age_of = |l: usize| ages.get(sorted_id[plotted[l]]).copied();
        let mut ord: Vec<usize> = (0..n).collect();
        ord.sort_by(|&a, &b| {
            // Oldest first (smallest timestamp → core). No age → i64::MAX → rim.
            age_of(a)
                .unwrap_or(i64::MAX)
                .cmp(&age_of(b).unwrap_or(i64::MAX))
                .then(dfs_rank[a].cmp(&dfs_rank[b]))
        });
        ord
    };
    let (mut x, mut y) = (vec![0.0f64; n], vec![0.0f64; n]);
    let total = n.max(1) as f64;
    let span = RADIAL_STEP * total.sqrt();
    for (rank, &node) in arm_order.iter().enumerate() {
        let t = (rank as f64 + 0.5) / total; // 0 at core → 1 at the fingertips
        let radius = span * t.sqrt(); // √ so density is even along the arm
        let (sx, sy) = sample(ARM_TURNS * t); // winds ARM_TURNS times core→rim
        let (px, py) = sample(ARM_TURNS * t + 0.25); // perpendicular (quarter turn)
        let jitter = ((rank as f64) * 0.618_033_988_75).fract() - 0.5;
        let width = ARM_WIDTH * radius; // the band widens toward the rim
        x[node] = sx * radius + px * jitter * width;
        y[node] = sy * radius + py * jitter * width;
    }

    // Centre on the plain centroid. The root sits at the origin and the arms
    // radiate from it, so the galaxy is roughly symmetric already; a plain centroid
    // keeps it framed without a big arm dragging the frame off to one side (which a
    // degree-weighted centre did). `degree` now only exists for potential reuse.
    let _ = &degree;
    let cx = (0..n).map(|i| x[i]).sum::<f64>() / n as f64;
    let cy = (0..n).map(|i| y[i]).sum::<f64>() / n as f64;
    for i in 0..n {
        x[i] -= cx;
        y[i] -= cy;
    }

    let stars = (0..n)
        .map(|i| Star {
            x: x[i],
            y: y[i],
            mass: mass[i],
            code: code[i],
            is_leaf: is_leaf[i],
            is_support: support_frac[i] > 0.5,
            is_docs: docs_frac[i] > 0.5,
        })
        .collect();

    // The galaxy's size for the nebula threshold is its leaf count (files), not
    // the sum of every node's mass — otherwise the root slips under "half".
    let total_leaves = is_leaf.iter().filter(|&&l| l).count() as f64;
    let nebulae = context_nebulae(
        &children,
        &is_leaf,
        &support_frac,
        &docs_frac,
        &x,
        &y,
        total_leaves,
    );
    Galaxy { stars, nebulae }
}

/// One nebula per context grouping — every internal node that is a real grouping:
/// big enough to be a context (≥ `MIN_MASS` files) but not the all-encompassing
/// root (≤ half the galaxy), so empty space and the whole-repo node get no cloud.
/// Center = the centroid of the grouping's files; radius = their spread (RMS
/// distance, floored); colour = a distinct identity hue from the grouping's order
/// (golden-angle spacing so neighbours never share a hue), *not* its health.
fn context_nebulae(
    children: &[Vec<usize>],
    is_leaf: &[bool],
    support_frac: &[f64],
    docs_frac: &[f64],
    x: &[f64],
    y: &[f64],
    total_leaves: f64,
) -> Vec<Nebula> {
    let n = children.len();
    // Per node: plotted-leaf centroid accumulators, Σ(x²+y²), and plotted-leaf
    // count — one post-order pass. Counts are of *plotted* leaves (which may be
    // folders after a zoom collapse), so centroid and threshold stay consistent;
    // support share is precomputed over the full file tree and passed in.
    let (mut sx, mut sy, mut ssq) = (vec![0.0f64; n], vec![0.0f64; n], vec![0.0f64; n]);
    let mut cnt = vec![0.0f64; n];
    let roots: Vec<usize> = (0..n)
        .filter(|&i| !children.iter().any(|c| c.contains(&i)))
        .collect();
    for &root in &roots {
        let mut stack = vec![(root, false)];
        while let Some((node, visited)) = stack.pop() {
            if visited {
                if is_leaf[node] {
                    sx[node] = x[node];
                    sy[node] = y[node];
                    ssq[node] = x[node] * x[node] + y[node] * y[node];
                    cnt[node] = 1.0;
                } else {
                    for &c in &children[node] {
                        sx[node] += sx[c];
                        sy[node] += sy[c];
                        ssq[node] += ssq[c];
                        cnt[node] += cnt[c];
                    }
                }
            } else {
                stack.push((node, true));
                for &c in &children[node] {
                    stack.push((c, false));
                }
            }
        }
    }

    // Cap nebula radius to a fraction of the galaxy's own radius, so a context
    // stretched down a long arm can't balloon over the whole frame.
    let max_extent = (0..n)
        .map(|i| (x[i] * x[i] + y[i] * y[i]).sqrt())
        .fold(0.0f64, f64::max)
        .max(1.0);
    let nebula_cap = NEBULA_MAX_FRAC * max_extent;

    // Grouping nodes in deterministic order → identity hues. Threshold on plotted
    // leaves so it works the same at any zoom level.
    let groups: Vec<usize> = (0..n)
        .filter(|&i| !is_leaf[i] && cnt[i] >= MIN_NEBULA_MASS && cnt[i] <= 0.5 * total_leaves)
        .collect();
    groups
        .iter()
        .enumerate()
        .map(|(rank, &g)| {
            let count = cnt[g];
            let ncx = sx[g] / count;
            let ncy = sy[g] / count;
            let var = (ssq[g] / count - (ncx * ncx + ncy * ncy)).max(0.0);
            let r = (NEBULA_SPREAD * var.sqrt() + NEBULA_FLOOR).min(nebula_cap);
            // Golden-angle hue: consecutive groupings land far apart on the wheel.
            let hue = ((rank as f64) * 0.618_033_988_75).fract() * 360.0;
            // Role decides both colour and how it's drawn later: code = bright
            // saturated nebula; docs = dim readable haze; data = desaturated dust.
            let is_docs = docs_frac[g] > 0.5;
            let is_support = support_frac[g] > 0.5;
            let (sat, val) = if is_support { (0.12, 0.6) } else { (0.55, 0.9) };
            Nebula {
                x: ncx,
                y: ncy,
                r,
                color: hsv_to_rgb(hue, sat, val),
                is_support,
                is_docs,
            }
        })
        .collect()
}

/// HSV→RGB, the standard piecewise-linear conversion — arithmetic only, no trig,
/// so a hue maps to the same bytes on every platform. `h` in [0,360), `s`/`v` in
/// [0,1]. Used to give each context grouping a distinct identity colour.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hp as i64 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |t: f64| (((t + m) * 255.0) + 0.5).clamp(0.0, 255.0) as u8;
    [to(r1), to(g1), to(b1)]
}

/// Draw radius in pixels for a star of the given mass. A fixed base so leaves are
/// always visible, plus a `sqrt(mass)` glow so heavier folders read as brighter
/// cores, capped so the root never becomes one covering blob. Leaves are the
/// bright cells; folders draw a touch smaller as dim wells behind them.
fn star_radius_px(mass: f64, is_leaf: bool) -> f64 {
    // Small, star-like points — a fine dusting, not fat blobs.
    let base = if is_leaf { 1.3 } else { 1.1 };
    (base + 0.28 * mass.sqrt()).min(if is_leaf { 2.0 } else { 3.5 })
}

/// Per-node descendant-leaf count (`mass`) and support-leaf count, over the child
/// forest, in one iterative post-order pass (a deep tree cannot overflow the
/// stack). A childless node is a leaf: mass 1, support 1 if it is a support leaf.
/// An internal node sums its children.
fn subtree_counts(children: &[Vec<usize>], is_support_leaf: &[bool]) -> (Vec<f64>, Vec<f64>) {
    let n = children.len();
    let mut mass = vec![0.0f64; n];
    let mut sup = vec![0.0f64; n];
    let roots: Vec<usize> = (0..n)
        .filter(|&i| !children.iter().any(|c| c.contains(&i)))
        .collect();
    for &root in &roots {
        let mut stack = vec![(root, false)];
        while let Some((node, visited)) = stack.pop() {
            if visited {
                if children[node].is_empty() {
                    mass[node] = 1.0;
                    sup[node] = if is_support_leaf[node] { 1.0 } else { 0.0 };
                } else {
                    for &c in &children[node] {
                        mass[node] += mass[c];
                        sup[node] += sup[c];
                    }
                }
            } else {
                stack.push((node, true));
                for &c in &children[node] {
                    stack.push((c, false));
                }
            }
        }
    }
    (mass, sup)
}

/// The finest scale rank to plot without exceeding [`MAX_PLOT_NODES`]. Ranks run
/// Universe(0) → Moon(4); returns the largest rank whose cumulative node count
/// still fits, so a small atlas keeps every node (Moon) and a large one collapses
/// files into folders. Always keeps at least the coarsest levels present.
fn plot_cutoff_rank(scale: &[ScopeScale], n: usize) -> u8 {
    if n <= MAX_PLOT_NODES {
        return u8::MAX; // keep everything
    }
    let max_rank = scale.iter().map(|s| s.rank()).max().unwrap_or(0);
    let mut best = 0u8;
    for r in 0..=max_rank {
        let count = scale.iter().filter(|s| s.rank() <= r).count();
        if count <= MAX_PLOT_NODES {
            best = r;
        } else {
            break;
        }
    }
    best
}

/// Rasterize the galaxy onto a `canvas`×`canvas` RGBA square, three composited
/// layers on opaque "space":
///
/// 1. **Context nebulae** — each grouping a soft radial glow (quadratic falloff),
///    its identity colour *added* into a float accumulator so overlapping and
///    nested groupings brighten the shared region. Accumulating before clamping
///    makes the result independent of nebula draw order — deterministic.
/// 2. **Space + glow composited** to opaque RGBA.
/// 3. **Stars** — leaf files as bright health-coloured discs on top.
///
/// All squared-distance tests and arithmetic; no anti-aliasing, no trig.
pub fn render_galaxy(galaxy: &Galaxy, canvas: u32) -> (Vec<u8>, u32) {
    let side = canvas.max(1);
    let px_count = side as usize * side as usize;
    let mut rgba = vec![0u8; px_count * 4];
    let stars = &galaxy.stars;
    if stars.is_empty() {
        for p in rgba.chunks_mut(4) {
            p.copy_from_slice(&SPACE);
        }
        return (rgba, side);
    }
    // Frame on where the connected mass actually is. Isolated corpora drift into
    // the void under repulsion, and a plain min/max box then explodes and shoves
    // the core into a corner. Percentile bounds of the star centres ignore the few
    // far outliers — they simply clip at the edge — so the dense core fills the
    // frame.
    let mut xs: Vec<f64> = stars.iter().map(|s| s.x).collect();
    let mut ys: Vec<f64> = stars.iter().map(|s| s.y).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let pct = |v: &[f64], f: f64| v[(((v.len() - 1) as f64) * f).round() as usize];
    let (minx, maxx) = (pct(&xs, 0.02), pct(&xs, 0.98));
    let (miny, maxy) = (pct(&ys, 0.02), pct(&ys, 0.98));
    let margin = 0.06 * (side as f64);
    let span = (maxx - minx).max(maxy - miny).max(1e-9);
    let scale = (side as f64 - 2.0 * margin) / span;
    let map = |x: f64, y: f64| (margin + (x - minx) * scale, margin + (y - miny) * scale);

    // Layer 1: accumulate nebula glow (order-independent → deterministic). A code
    // context is a solid glowing cloud; a project *data* context (a corpus like a
    // db) is a scattered dust/asteroid field so it reads as loose matter, not a
    // dominating blob — but it is still shown, because it is part of the project.
    // (External program bulk — venv / node_modules / __pycache__ — is excluded
    // earlier, at the scan, so it never reaches here.)
    let mut glow = vec![0.0f32; px_count * 3];
    for neb in &galaxy.nebulae {
        let (cx, cy) = map(neb.x, neb.y);
        let rp = neb.r * scale;
        if rp < 0.5 {
            continue;
        }
        let (cr, cg, cb) = (neb.color[0] as f32, neb.color[1] as f32, neb.color[2] as f32);
        let mut add = |ax: f64, ay: f64, t: f32| {
            let ix = ax.floor();
            let iy = ay.floor();
            if ix < 0.0 || iy < 0.0 || ix >= side as f64 || iy >= side as f64 {
                return;
            }
            let q = (iy as usize * side as usize + ix as usize) * 3;
            glow[q] += cr * t;
            glow[q + 1] += cg * t;
            glow[q + 2] += cb * t;
        };
        if !neb.is_support {
            // Code: discrete, structured — drawn as an asteroid belt of dim specks
            // on a phyllotaxis spiral (deterministic — golden-angle constant-matrix
            // rotation, no runtime trig), the individual units, not a diffuse mass.
            let n = ((rp * rp / 22.0) as usize).clamp(24, 4000);
            let (mut dx, mut dy) = (1.0f64, 0.0f64);
            for k in 0..n {
                let rr = rp * (((k as f64) + 0.5) / n as f64).sqrt();
                // Gravity well: the specks concentrate toward the context's centre
                // — bright and dense at the core, thinning to the rim — so the dust
                // reads as matter falling into a well, not a flat disc.
                let well = (1.0 - 0.65 * rr / rp) as f32;
                let (sx, sy) = (cx + dx * rr, cy + dy * rr);
                add(sx, sy, 0.6 * well);
                add(sx + 1.0, sy, 0.35 * well);
                add(sx, sy + 1.0, 0.35 * well);
                let (nx, ny) = (dx * GA_COS - dy * GA_SIN, dx * GA_SIN + dy * GA_COS);
                dx = nx;
                dy = ny;
            }
        } else {
            // Data (and docs): a diffuse bulk mass → an organic gas cloud. Data at
            // full brightness; docs as a dim haze — the readable layer, present but
            // not competing.
            let gain = if neb.is_docs {
                NEBULA_GAIN as f32 * 0.3
            } else {
                NEBULA_GAIN as f32
            };
            let rp2 = rp * rp;
            let x0 = (cx - rp).floor().max(0.0) as i64;
            let x1 = (cx + rp).floor().min(side as f64 - 1.0) as i64;
            let y0 = (cy - rp).floor().max(0.0) as i64;
            let y1 = (cy + rp).floor().min(side as f64 - 1.0) as i64;
            let mut py = y0;
            while py <= y1 {
                let mut px = x0;
                while px <= x1 {
                    let dx = px as f64 + 0.5 - cx;
                    let dy = py as f64 + 0.5 - cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 <= rp2 {
                        // Organic cloud: the smooth falloff × fractal value noise,
                        // so the nebula breaks into wisps and lumps instead of a
                        // perfect disc.
                        let falloff = (1.0 - d2 / rp2) as f32;
                        let density = cloud_density(px as f64, py as f64);
                        add(px as f64, py as f64, falloff * gain * density);
                    }
                    px += 1;
                }
                py += 1;
            }
        }
    }

    // Layer 2: space + glow → opaque pixels.
    for i in 0..px_count {
        let g = i * 3;
        let p = i * 4;
        rgba[p] = (SPACE[0] as f32 + glow[g]).min(255.0) as u8;
        rgba[p + 1] = (SPACE[1] as f32 + glow[g + 1]).min(255.0) as u8;
        rgba[p + 2] = (SPACE[2] as f32 + glow[g + 2]).min(255.0) as u8;
        rgba[p + 3] = 255;
    }

    // Layer 3: code leaf stars, health-coloured, on top. Support leaves (data,
    // vendored, docs) are laid out but not drawn.
    for s in stars.iter().filter(|s| s.is_leaf && !s.is_support) {
        let (cx, cy) = map(s.x, s.y);
        let rp = star_radius_px(s.mass, s.is_leaf);
        let rp2 = rp * rp;
        let col = color::rgba(s.code);
        let x0 = (cx - rp).floor().max(0.0) as i64;
        let x1 = (cx + rp).floor().min(side as f64 - 1.0) as i64;
        let y0 = (cy - rp).floor().max(0.0) as i64;
        let y1 = (cy + rp).floor().min(side as f64 - 1.0) as i64;
        let mut py = y0;
        while py <= y1 {
            let mut px = x0;
            while px <= x1 {
                let dx = px as f64 + 0.5 - cx;
                let dy = py as f64 + 0.5 - cy;
                if dx * dx + dy * dy <= rp2 {
                    let p = (py as usize * side as usize + px as usize) * 4;
                    rgba[p..p + 4].copy_from_slice(&col);
                }
                px += 1;
            }
            py += 1;
        }
    }
    (rgba, side)
}

/// Layout + rasterize + PNG encode. Pure function of the nodes, edges, support
/// set, and canvas.
pub fn galaxy_png(
    nodes: &[(String, Option<String>, ScopeScale, GreenCode)],
    edges: &[(String, String)],
    support: &BTreeSet<String>,
    docs: &BTreeSet<String>,
    ages: &BTreeMap<String, i64>,
    canvas: u32,
) -> Vec<u8> {
    let g = galaxy_with_support(nodes, edges, support, docs, ages);
    let (rgba, side) = render_galaxy(&g, canvas);
    png::encode_rgba_png(side, side, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(
        id: &str,
        parent: Option<&str>,
        code: GreenCode,
    ) -> (String, Option<String>, ScopeScale, GreenCode) {
        (
            id.to_string(),
            parent.map(|p| p.to_string()),
            ScopeScale::Moon,
            code,
        )
    }

    fn tree() -> Vec<(String, Option<String>, ScopeScale, GreenCode)> {
        vec![
            node("root", None, GreenCode::Unknown),
            node("fa", Some("root"), GreenCode::Unknown),
            node("fb", Some("root"), GreenCode::Unknown),
            node("l1", Some("fa"), GreenCode::Green),
            node("l2", Some("fa"), GreenCode::Red),
            node("l3", Some("fb"), GreenCode::Yellow),
        ]
    }

    #[test]
    fn layout_places_every_node() {
        let stars = galaxy_layout(&tree(), &[]);
        assert_eq!(stars.len(), 6);
        assert_eq!(stars.iter().filter(|s| s.is_leaf).count(), 3);
    }

    fn two_systems() -> Vec<(String, Option<String>, ScopeScale, GreenCode)> {
        // root -> {sysA -> a1..a4, sysB -> b1..b4}. 8 leaves total; each system
        // holds 4 (= half the galaxy), so both qualify as context nebulae while
        // the 8-mass root is excluded as the all-encompassing node.
        let mut v = vec![
            node("root", None, GreenCode::Unknown),
            node("sysA", Some("root"), GreenCode::Unknown),
            node("sysB", Some("root"), GreenCode::Unknown),
        ];
        for i in 0..4 {
            v.push(node(&format!("a{i}"), Some("sysA"), GreenCode::Green));
            v.push(node(&format!("b{i}"), Some("sysB"), GreenCode::Red));
        }
        v
    }

    #[test]
    fn simulation_is_deterministic() {
        assert_eq!(galaxy(&tree(), &[]), galaxy(&tree(), &[]));
        let empty = BTreeSet::new();
        assert_eq!(
            galaxy_png(&tree(), &[], &empty, &empty, &BTreeMap::new(), 128),
            galaxy_png(&tree(), &[], &empty, &empty, &BTreeMap::new(), 128)
        );
        // Nebulae are part of the deterministic output too.
        assert_eq!(galaxy(&two_systems(), &[]), galaxy(&two_systems(), &[]));
    }

    #[test]
    fn context_groupings_become_distinct_nebulae() {
        let g = galaxy(&two_systems(), &[]);
        assert_eq!(g.nebulae.len(), 2, "sysA and sysB are the context groupings");
        // Different context → different colour (identity, not health).
        assert_ne!(
            g.nebulae[0].color, g.nebulae[1].color,
            "each context gets a distinct hue"
        );
        assert!(g.nebulae.iter().all(|nb| nb.r > 0.0), "nebulae have extent");
    }

    #[test]
    fn no_grouping_no_nebula() {
        // The tiny tree's folders are all too small or the whole galaxy — no
        // context grouping qualifies, so there is no nebula colour anywhere.
        assert!(galaxy(&tree(), &[]).nebulae.is_empty());
    }

    #[test]
    fn support_contexts_are_desaturated() {
        // Mark all of sysB's files support; its nebula must be greyer (lower
        // saturation) than sysA's, while both keep a hue (identity is preserved).
        let mut support = BTreeSet::new();
        for i in 0..4 {
            support.insert(format!("b{i}"));
        }
        let g = galaxy_with_support(&two_systems(), &[], &support, &BTreeSet::new(), &BTreeMap::new());
        assert_eq!(g.nebulae.len(), 2);
        let sat = |c: [u8; 3]| {
            let max = c.iter().copied().max().unwrap() as f64;
            let min = c.iter().copied().min().unwrap() as f64;
            if max == 0.0 { 0.0 } else { (max - min) / max }
        };
        // Nebulae order follows grouping order (sysA then sysB by id).
        let (a_sat, b_sat) = (sat(g.nebulae[0].color), sat(g.nebulae[1].color));
        assert!(
            b_sat < a_sat,
            "the support context (sysB) should be desaturated: {b_sat} !< {a_sat}"
        );
    }

    #[test]
    fn support_classification_matches_roles() {
        use creature_context_types::EntityKind;
        assert!(entity_is_support(EntityKind::File, Some("README.md")));
        assert!(entity_is_support(EntityKind::File, Some("data/corpus.jsonl")));
        assert!(entity_is_support(EntityKind::File, Some("app/node_modules/x/index.js")));
        assert!(entity_is_support(EntityKind::Resource, Some("assets/logo.png")));
        assert!(!entity_is_support(EntityKind::Function, Some("src/lib.rs")));
        assert!(!entity_is_support(EntityKind::File, Some("src/main.rs")));
    }

    #[test]
    fn meta_classification_excludes_tooling() {
        // Descendants of a meta directory, at any depth and any extension.
        assert!(entity_is_meta(Some(".agents/plugins/x/scanner.py")));
        assert!(entity_is_meta(Some("Core/.claude/rules/foo.md")));
        assert!(entity_is_meta(Some(".github/workflows/ci.yml")));
        assert!(entity_is_meta(Some(".cursor/rules/creature-context.mdc")));
        // Root-level meta files by basename, case-insensitive.
        assert!(entity_is_meta(Some("AGENTS.md")));
        assert!(entity_is_meta(Some("sub/CLAUDE.md")));
        // Product code and data are NOT meta.
        assert!(!entity_is_meta(Some("src/main.rs")));
        assert!(!entity_is_meta(Some("Core/native/Sources/lib.swift")));
        assert!(!entity_is_meta(Some("docs/guide.md")));
        assert!(!entity_is_meta(None));
        // A same-named symbol that is not a path segment must not match.
        assert!(!entity_is_meta(Some("src/agents.rs")));
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), [0, 0, 255]);
    }

    #[test]
    fn mass_rolls_up_the_tree() {
        // root=3 leaves, fa=2, fb=1, each leaf=1 → root draws largest.
        let stars = galaxy_layout(&tree(), &[]);
        let root = stars
            .iter()
            .max_by(|a, b| a.mass.partial_cmp(&b.mass).unwrap())
            .unwrap();
        assert!(!root.is_leaf, "the heaviest node is a folder (the root)");
        assert_eq!(root.mass, 3.0, "root holds all three leaves");
    }

    #[test]
    fn edges_do_not_break_determinism() {
        // The layout is tree-based: dependency edges feed only the framing (degree),
        // not node positions. Adding one must keep the output byte-deterministic.
        let e = vec![("l1".to_string(), "l3".to_string())];
        assert_eq!(galaxy_layout(&tree(), &e), galaxy_layout(&tree(), &e));
    }

    fn arm_tree() -> Vec<(String, Option<String>, ScopeScale, GreenCode)> {
        // A galaxy root with two System directories, each holding three files.
        let mut v = vec![
            ("root".into(), None, ScopeScale::Galaxy, GreenCode::Unknown),
            (
                "sysA".into(),
                Some("root".into()),
                ScopeScale::System,
                GreenCode::Unknown,
            ),
            (
                "sysB".into(),
                Some("root".into()),
                ScopeScale::System,
                GreenCode::Unknown,
            ),
        ];
        for i in 0..3 {
            v.push((
                format!("a{i}"),
                Some("sysA".into()),
                ScopeScale::Moon,
                GreenCode::Green,
            ));
            v.push((
                format!("b{i}"),
                Some("sysB".into()),
                ScopeScale::Moon,
                GreenCode::Red,
            ));
        }
        v
    }

    #[test]
    fn directories_become_separate_arms() {
        // Each top-level directory is its own arm, so one directory's files cluster
        // apart from another's. sysA's files (Green) and sysB's (Red) occupy
        // different arms → their centroids are distinct and separated.
        let stars = galaxy_layout(&arm_tree(), &[]);
        let centroid = |code: GreenCode| {
            let group: Vec<&Star> = stars.iter().filter(|s| s.code == code && s.is_leaf).collect();
            let n = group.len().max(1) as f64;
            (
                group.iter().map(|s| s.x).sum::<f64>() / n,
                group.iter().map(|s| s.y).sum::<f64>() / n,
            )
        };
        let (agx, agy) = centroid(GreenCode::Green);
        let (arx, ary) = centroid(GreenCode::Red);
        let sep = ((agx - arx).powi(2) + (agy - ary).powi(2)).sqrt();
        assert!(sep > 1.0, "the two directories sit in different arms (sep={sep})");
    }
}
