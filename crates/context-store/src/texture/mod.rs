//! Deterministic "green pixels" texture projection: the atlas rendered as an
//! image, every entity a pixel colored by its Green status. Same snapshot →
//! byte-identical PNG. The layout function (`render_square`) is isolated so a
//! later plan can swap in the galaxy (circle-pack) layout without touching the
//! color map, PNG writer, or IO.

pub mod color;
pub mod force;
pub mod galaxy;
pub mod hilbert;
pub mod pack;
pub mod png;

use creature_context_types::{AtlasSnapshot, GreenCode, ScopeScale};

/// Render `ATLAS.png` bytes directly from a snapshot in memory (no store round
/// trip) — what the resident daemon calls each re-index so the live map stays
/// fresh. `galaxy` selects the circle-packed layout; else the Hilbert square.
pub fn render_snapshot_png(snapshot: &AtlasSnapshot, galaxy: bool) -> Vec<u8> {
    let code = |entity: &creature_context_types::AtlasEntity| {
        entity
            .green
            .as_ref()
            .map(|green| green.overall)
            .unwrap_or(GreenCode::Unknown)
    };
    if galaxy {
        // Agent/tooling meta (`.agents/`, `.claude/`, `.github/`, …) is excluded
        // from the galaxy: it isn't the product, and its file count would drown
        // the map. Drop those entities up front, then keep only edges whose both
        // ends survive.
        let excluded: std::collections::BTreeSet<String> = snapshot
            .entities
            .iter()
            .filter(|e| force::entity_is_meta(e.relative_path.as_deref()))
            .map(|e| e.id.to_string())
            .collect();
        let nodes: Vec<_> = snapshot
            .entities
            .iter()
            .filter(|e| !excluded.contains(&e.id.to_string()))
            .map(|e| {
                (
                    e.id.to_string(),
                    e.parent_id.map(|p| p.to_string()),
                    e.scale,
                    code(e),
                )
            })
            .collect();
        // Dependency edges (everything but containment, which parent_id carries)
        // become springs so connected modules cluster into arms.
        let edges: Vec<(String, String)> = snapshot
            .edges
            .iter()
            .filter(|e| e.kind != creature_context_types::RelationshipKind::Contains)
            .map(|e| (e.source_entity_id.to_string(), e.target_entity_id.to_string()))
            .filter(|(s, t)| !excluded.contains(s) && !excluded.contains(t))
            .collect();
        // Support entities → data dust (not code nebulae). Two signals, matching
        // AtlasRepository::support_entity_ids: data/doc extension or vendored path,
        // and — the stronger one — a File with no parsed code-symbol child (a data
        // or binary file whatever its extension).
        use creature_context_types::EntityKind;
        let has_code_child: std::collections::BTreeSet<_> = snapshot
            .entities
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntityKind::Function
                        | EntityKind::Type
                        | EntityKind::Component
                        | EntityKind::Test
                )
            })
            .filter_map(|e| e.parent_id)
            .collect();
        let support: std::collections::BTreeSet<String> = snapshot
            .entities
            .iter()
            .filter(|e| {
                force::entity_is_support(e.kind, e.relative_path.as_deref())
                    || (matches!(e.kind, EntityKind::File | EntityKind::Resource)
                        && !has_code_child.contains(&e.id))
            })
            .map(|e| e.id.to_string())
            .collect();
        // Docs (readable layer) → dim haze; a distinct role from data dust.
        let docs: std::collections::BTreeSet<String> = snapshot
            .entities
            .iter()
            .filter(|e| force::entity_is_docs(e.kind, e.relative_path.as_deref()))
            .map(|e| e.id.to_string())
            .collect();
        // The live daemon render has no project root to stat file dates against,
        // so it uses tree order for the arm (empty ages). The `map` CLI, which has
        // the root, orders the arm by file creation date.
        force::galaxy_png(&nodes, &edges, &support, &docs, &std::collections::BTreeMap::new(), 1024)
    } else {
        let rows = snapshot
            .entities
            .iter()
            .map(|e| (e.scale, e.id.to_string(), code(e)))
            .collect();
        square_png(&order_codes(rows))
    }
}

/// Sort light `(scale, id, code)` rows into idx order — `(scale.rank(), id)` —
/// and drop everything but the codes. This is the exact order
/// `encode_atlas_idx` emits entities in, so pixel `d` is entity `d`.
pub fn order_codes(mut rows: Vec<(ScopeScale, String, GreenCode)>) -> Vec<GreenCode> {
    rows.sort_by(|a, b| a.0.rank().cmp(&b.0.rank()).then_with(|| a.1.cmp(&b.1)));
    rows.into_iter().map(|(_, _, code)| code).collect()
}

/// Smallest power-of-two `side` such that `side*side >= n`. Integer-only (no
/// float `sqrt`) so it is bit-for-bit identical on every platform.
fn square_side(n: usize) -> u32 {
    if n <= 1 {
        return 1;
    }
    let mut s: u64 = 0;
    while s * s < n as u64 {
        s += 1;
    }
    (s as u32).next_power_of_two()
}

/// Lay `codes` on a Hilbert curve of the smallest fitting power-of-two square,
/// returning `(rgba, side)`. Cells past the last entity are transparent.
pub fn render_square(codes: &[GreenCode]) -> (Vec<u8>, u32) {
    let side = square_side(codes.len());
    let mut rgba = vec![0u8; side as usize * side as usize * 4]; // transparent
    for (d, &code) in codes.iter().enumerate() {
        let (x, y) = hilbert::d2xy(side, d as u32);
        let p = (y as usize * side as usize + x as usize) * 4;
        rgba[p..p + 4].copy_from_slice(&color::rgba(code));
    }
    (rgba, side)
}

/// The full square projection: layout + PNG encode. Pure function of `codes`.
pub fn square_png(codes: &[GreenCode]) -> Vec<u8> {
    let (rgba, side) = render_square(codes);
    png::encode_rgba_png(side, side, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_is_power_of_two_and_fits() {
        assert_eq!(square_side(0), 1);
        assert_eq!(square_side(1), 1);
        assert_eq!(square_side(2), 2);
        assert_eq!(square_side(4), 2);
        assert_eq!(square_side(5), 4);
        assert_eq!(square_side(16), 4);
        assert_eq!(square_side(17), 8);
        // realistic: 188034 entities -> 512
        assert_eq!(square_side(188_034), 512);
    }

    #[test]
    fn pixel_lands_at_its_hilbert_index_with_its_color() {
        // 4 entities -> side 2. Index 2 -> (1,1) -> Green.
        let codes = vec![
            GreenCode::Unknown,
            GreenCode::Red,
            GreenCode::Green,
            GreenCode::Yellow,
        ];
        let (rgba, side) = render_square(&codes);
        assert_eq!(side, 2);
        let (x, y) = hilbert::d2xy(2, 2);
        let p = (y as usize * side as usize + x as usize) * 4;
        assert_eq!(&rgba[p..p + 4], &color::rgba(GreenCode::Green));
    }

    #[test]
    fn tail_cells_are_transparent() {
        let codes = vec![GreenCode::Green]; // 1 entity, side 1, no tail
        let (rgba, side) = render_square(&codes);
        assert_eq!(side, 1);
        assert_eq!(&rgba[0..4], &color::rgba(GreenCode::Green));

        let codes3 = vec![GreenCode::Green, GreenCode::Green, GreenCode::Green]; // side 2, 1 tail cell
        let (rgba3, _) = render_square(&codes3);
        let transparent = rgba3.chunks(4).filter(|c| *c == color::TRANSPARENT).count();
        assert_eq!(transparent, 1);
    }

    #[test]
    fn deterministic_png() {
        let codes = vec![GreenCode::Green, GreenCode::Red, GreenCode::Unknown];
        assert_eq!(square_png(&codes), square_png(&codes));
    }
}
