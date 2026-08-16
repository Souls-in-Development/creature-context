//! `map`: render the current snapshot's green-pixels texture to ATLAS.png, and
//! append it as a per-snapshot frame. Reads the store with bounded per-entity
//! readers; the image is a deterministic function of the atlas. `--galaxy`
//! circle-packs the atlas tree; the default is the Hilbert square.

use creature_context_core::project::{ProjectPaths, atomic_write};
use creature_context_store::{AtlasRepository, texture};
use std::error::Error;
use std::path::Path;

/// Fixed galaxy canvas resolution (square).
const GALAXY_CANVAS: u32 = 1024;

/// Sanitize a snapshot id into a filesystem-safe frame filename stem.
fn frame_stem(snapshot_id: &str) -> String {
    snapshot_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

pub fn run_map(root: &Path, galaxy: bool, axis: Option<&str>) -> Result<(), Box<dyn Error>> {
    let paths = ProjectPaths::new(root);
    let repository = AtlasRepository::open(&paths.database)?;

    let (png, count, layout, dims) = if galaxy {
        // Agent/tooling meta (`.agents/`, `.claude/`, `.github/`, …) is not the
        // product — exclude it from the galaxy so its file count can't drown the
        // map. Drop those nodes, then keep only edges whose both ends survive.
        let excluded = repository.meta_entity_ids()?;
        let nodes: Vec<_> = repository
            .entity_tree_nodes_for_axis(axis)?
            .into_iter()
            .filter(|(id, ..)| !excluded.contains(id))
            .collect();
        let edges: Vec<_> = repository
            .entity_edges()?
            .into_iter()
            .filter(|(s, t)| !excluded.contains(s) && !excluded.contains(t))
            .collect();
        let support = repository.support_entity_ids()?;
        let docs = repository.doc_entity_ids()?;
        let ages = repository.entity_ages(&paths.root)?;
        let count = nodes.len();
        let png = texture::force::galaxy_png(&nodes, &edges, &support, &docs, &ages, GALAXY_CANVAS);
        (
            png,
            count,
            "galaxy",
            format!("{GALAXY_CANVAS}x{GALAXY_CANVAS}"),
        )
    } else {
        let rows = repository.entity_green_codes()?;
        let count = rows.len();
        let codes = texture::order_codes(rows);
        let (rgba, side) = texture::render_square(&codes);
        let png = texture::png::encode_rgba_png(side, side, &rgba);
        (png, count, "square", format!("{side}x{side}"))
    };

    let atlas_png = paths.root.join("ATLAS.png");
    atomic_write(&atlas_png, &png)?;

    // Append-only frame for this snapshot. Bytes are deterministic per snapshot,
    // so re-running is a no-op if the frame already exists.
    if let Some(snapshot) = repository.current_snapshot_id()? {
        let frames = paths.creature.join("atlas-frames");
        std::fs::create_dir_all(&frames)?;
        let frame = frames.join(format!("{}.png", frame_stem(&snapshot.0)));
        if !frame.exists() {
            atomic_write(&frame, &png)?;
        }
    }

    println!(
        "map: {count} entities, {layout}, {dims}, {}",
        atlas_png.display()
    );
    Ok(())
}
