//! Galaxy layout: circle-pack the atlas tree so folders become bubbles, then
//! rasterize every leaf as a filled disc colored by its Green status. Pure,
//! deterministic (sqrt-only, no trig, no RNG): same nodes → same pixels.

use crate::texture::color;
use crate::texture::pack::{Circle, enclose, pack_siblings};
use crate::texture::png;
use creature_context_types::{GreenCode, ScopeScale};
use std::collections::BTreeMap;

/// Every leaf gets the same radius; folder radii grow from packing.
const LEAF_R: f64 = 1.0;

/// A placed leaf: absolute center + its health.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leaf {
    pub x: f64,
    pub y: f64,
    pub code: GreenCode,
}

/// A placed circle in the galaxy: any node (folder or leaf) with its absolute
/// center, packed radius, tree depth and health. Folders are the "dishes" (drawn
/// as rings); leaves are the "cells" (drawn as filled discs). Emitted in DFS
/// order — parent before its children — so painting in order draws outer rings
/// before inner ones with no sort required.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Disc {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub depth: u32,
    pub code: GreenCode,
    pub is_leaf: bool,
}

struct Node {
    code: GreenCode,
    children: Vec<usize>,
    radius: f64,
    offsets: Vec<(f64, f64)>,
}

/// Circle-pack the atlas tree and return every leaf's absolute position + code.
/// Thin filter over [`galaxy_discs`] for callers that only want the cells.
pub fn galaxy_layout(nodes: Vec<(String, Option<String>, ScopeScale, GreenCode)>) -> Vec<Leaf> {
    galaxy_discs(nodes)
        .into_iter()
        .filter(|d| d.is_leaf)
        .map(|d| Leaf {
            x: d.x,
            y: d.y,
            code: d.code,
        })
        .collect()
}

/// Circle-pack the atlas tree and return **every** node as a placed [`Disc`] —
/// folders and leaves alike — in DFS (parent-before-child) order. Input:
/// `(id, parent_id, scale, code)` rows (order irrelevant — children are sorted by
/// id for determinism). `scale` is unused by the layout (structure comes from
/// `parent_id`). Pure and deterministic: same nodes → same discs.
pub fn galaxy_discs(nodes: Vec<(String, Option<String>, ScopeScale, GreenCode)>) -> Vec<Disc> {
    if nodes.is_empty() {
        return vec![];
    }
    let index: BTreeMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.0.clone(), i))
        .collect();
    let mut tree: Vec<Node> = nodes
        .iter()
        .map(|n| Node {
            code: n.3,
            children: Vec::new(),
            radius: LEAF_R,
            offsets: Vec::new(),
        })
        .collect();
    let mut roots: Vec<usize> = Vec::new();
    let mut ordered: Vec<usize> = (0..nodes.len()).collect();
    ordered.sort_by(|&a, &b| nodes[a].0.cmp(&nodes[b].0));
    for &i in &ordered {
        match &nodes[i].1 {
            Some(pid) => match index.get(pid) {
                Some(&p) => tree[p].children.push(i),
                None => roots.push(i), // dangling parent → treat as root
            },
            None => roots.push(i),
        }
    }
    roots.sort_by(|&a, &b| nodes[a].0.cmp(&nodes[b].0));

    fn up(node: usize, tree: &mut Vec<Node>) -> f64 {
        let children = tree[node].children.clone();
        if children.is_empty() {
            tree[node].radius = LEAF_R;
            tree[node].offsets = Vec::new();
            return LEAF_R;
        }
        let mut radii = Vec::with_capacity(children.len());
        for &ch in &children {
            radii.push(up(ch, tree));
        }
        let centers = pack_siblings(&radii);
        let circles: Vec<Circle> = centers
            .iter()
            .zip(&radii)
            .map(|(&(x, y), &r)| Circle { x, y, r })
            .collect();
        let e = enclose(&circles);
        let offsets: Vec<(f64, f64)> = centers.iter().map(|&(x, y)| (x - e.x, y - e.y)).collect();
        tree[node].radius = e.r;
        tree[node].offsets = offsets;
        e.r
    }
    for &r in &roots {
        up(r, &mut tree);
    }

    let root_radii: Vec<f64> = roots.iter().map(|&r| tree[r].radius).collect();
    let root_centers = pack_siblings(&root_radii);
    let root_circles: Vec<Circle> = root_centers
        .iter()
        .zip(&root_radii)
        .map(|(&(x, y), &r)| Circle { x, y, r })
        .collect();
    let root_enc = enclose(&root_circles);
    let root_abs: Vec<(f64, f64)> = root_centers
        .iter()
        .map(|&(x, y)| (x - root_enc.x, y - root_enc.y))
        .collect();

    fn down(node: usize, cx: f64, cy: f64, depth: u32, tree: &[Node], out: &mut Vec<Disc>) {
        let n = &tree[node];
        if n.children.is_empty() {
            out.push(Disc {
                x: cx,
                y: cy,
                r: LEAF_R,
                depth,
                code: n.code,
                is_leaf: true,
            });
            return;
        }
        // Emit the folder's own enclosing circle before descending, so `out` is in
        // parent-before-child order and outer rings paint before inner ones.
        out.push(Disc {
            x: cx,
            y: cy,
            r: n.radius,
            depth,
            code: n.code,
            is_leaf: false,
        });
        for (child, &(ox, oy)) in n.children.iter().zip(&n.offsets) {
            down(*child, cx + ox, cy + oy, depth + 1, tree, out);
        }
    }
    let mut discs = Vec::new();
    for (&r, &(ox, oy)) in roots.iter().zip(&root_abs) {
        down(r, ox, oy, 0, &tree, &mut discs);
    }
    discs
}

/// Rasterize the galaxy onto a `canvas`×`canvas` RGBA square. Two passes over the
/// placed discs, both hard-edged squared-distance tests (no anti-aliasing, no
/// trig — exactly reproducible):
///
/// 1. **Folders as rings** — each folder's enclosing circle drawn as a muted
///    "dish" outline, in DFS order so outer rings sit under inner ones. This is
///    the fix for the old "petri dish" look: the layout always packed folders
///    into nested circles, but only the leaf cells were ever drawn, so the dishes
///    holding them were invisible.
/// 2. **Leaves as filled discs** — the solid status "cells", painted last so they
///    sit on top of the rings.
///
/// The bounding box spans every disc, so the outermost folder ring frames the
/// image.
pub fn render_galaxy(discs: &[Disc], canvas: u32) -> (Vec<u8>, u32) {
    let side = canvas.max(1);
    let mut rgba = vec![0u8; side as usize * side as usize * 4];
    if discs.is_empty() {
        return (rgba, side);
    }
    let (mut minx, mut miny) = (f64::INFINITY, f64::INFINITY);
    let (mut maxx, mut maxy) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for d in discs {
        minx = minx.min(d.x - d.r);
        miny = miny.min(d.y - d.r);
        maxx = maxx.max(d.x + d.r);
        maxy = maxy.max(d.y + d.r);
    }
    let margin = 0.02 * (side as f64);
    let span = (maxx - minx).max(maxy - miny).max(1e-9);
    let scale = (side as f64 - 2.0 * margin) / span;

    // Pass 1: folder rings (dishes). Ring width scales with the folder so the root
    // reads as a bold frame and deep planets as fine outlines, bounded so nested
    // rings stay visible and no ring vanishes below a pixel.
    for d in discs.iter().filter(|d| !d.is_leaf) {
        let cx = margin + (d.x - minx) * scale;
        let cy = margin + (d.y - miny) * scale;
        let r_px = (d.r * scale).max(1.0);
        let w = (r_px * 0.04).clamp(1.0, 3.0);
        let inner = (r_px - w).max(0.0);
        let (r2, inner2) = (r_px * r_px, inner * inner);
        stamp(&mut rgba, side, cx, cy, r_px, color::ring(d.code), |dist2| {
            dist2 <= r2 && dist2 >= inner2
        });
    }

    // Pass 2: leaf cells, filled, on top.
    let leaf_px = (LEAF_R * scale).max(0.5);
    let leaf_px2 = leaf_px * leaf_px;
    for d in discs.iter().filter(|d| d.is_leaf) {
        let cx = margin + (d.x - minx) * scale;
        let cy = margin + (d.y - miny) * scale;
        stamp(&mut rgba, side, cx, cy, leaf_px, color::rgba(d.code), |dist2| {
            dist2 <= leaf_px2
        });
    }
    (rgba, side)
}

/// Paint the pixels of a `reach`-radius box around `(cx, cy)` for which the
/// squared distance to the center satisfies `hit`, in `color`. Shared by the ring
/// and disc passes so both use one clipped, hard-edged rasterizer.
fn stamp(
    rgba: &mut [u8],
    side: u32,
    cx: f64,
    cy: f64,
    reach: f64,
    color: [u8; 4],
    hit: impl Fn(f64) -> bool,
) {
    let x0 = (cx - reach).floor().max(0.0) as i64;
    let x1 = (cx + reach).floor().min(side as f64 - 1.0) as i64;
    let y0 = (cy - reach).floor().max(0.0) as i64;
    let y1 = (cy + reach).floor().min(side as f64 - 1.0) as i64;
    let mut py = y0;
    while py <= y1 {
        let mut px = x0;
        while px <= x1 {
            let dx = px as f64 + 0.5 - cx;
            let dy = py as f64 + 0.5 - cy;
            if hit(dx * dx + dy * dy) {
                let p = (py as usize * side as usize + px as usize) * 4;
                rgba[p..p + 4].copy_from_slice(&color);
            }
            px += 1;
        }
        py += 1;
    }
}

/// Layout + rasterize + PNG encode. Pure function of the nodes and canvas size.
pub fn galaxy_png(
    nodes: Vec<(String, Option<String>, ScopeScale, GreenCode)>,
    canvas: u32,
) -> Vec<u8> {
    let discs = galaxy_discs(nodes);
    let (rgba, side) = render_galaxy(&discs, canvas);
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

    fn tiny_tree() -> Vec<(String, Option<String>, ScopeScale, GreenCode)> {
        // root -> {fa -> {l1, l2}, fb -> {l3}}
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
    fn leaves_are_the_childless_nodes() {
        let leaves = galaxy_layout(tiny_tree());
        assert_eq!(leaves.len(), 3, "l1, l2, l3 are the leaves");
        let mut codes: Vec<GreenCode> = leaves.iter().map(|l| l.code).collect();
        codes.sort();
        assert_eq!(codes, vec![GreenCode::Red, GreenCode::Yellow, GreenCode::Green]);
    }

    #[test]
    fn leaf_centers_are_distinct() {
        let leaves = galaxy_layout(tiny_tree());
        for a in 0..leaves.len() {
            for b in (a + 1)..leaves.len() {
                let same = (leaves[a].x - leaves[b].x).abs() < 1e-9
                    && (leaves[a].y - leaves[b].y).abs() < 1e-9;
                assert!(!same, "two leaves coincide");
            }
        }
    }

    #[test]
    fn layout_is_deterministic() {
        assert_eq!(galaxy_layout(tiny_tree()), galaxy_layout(tiny_tree()));
    }

    #[test]
    fn render_is_deterministic_and_draws_color() {
        let a = galaxy_png(tiny_tree(), 128);
        let b = galaxy_png(tiny_tree(), 128);
        assert_eq!(a, b, "same nodes -> same PNG");
        let (rgba, _) = render_galaxy(&galaxy_discs(tiny_tree()), 128);
        let green = color::rgba(GreenCode::Green);
        assert!(rgba.chunks(4).any(|c| c == green), "green leaf drawn");
    }

    #[test]
    fn discs_include_folders_and_leaves() {
        let discs = galaxy_discs(tiny_tree());
        let folders = discs.iter().filter(|d| !d.is_leaf).count();
        let leaves = discs.iter().filter(|d| d.is_leaf).count();
        assert_eq!(leaves, 3, "l1, l2, l3");
        assert_eq!(folders, 3, "root, fa, fb");
        // Parent before child: the root folder is emitted first, and folders
        // enclose their children (larger radius).
        assert!(!discs[0].is_leaf && discs[0].depth == 0, "root first");
        // Folders enclose their children, so never smaller than a leaf. A folder
        // wrapping a single leaf is exactly leaf-sized; a multi-child folder (the
        // root here, with two subfolders) is strictly larger.
        assert!(
            discs.iter().filter(|d| !d.is_leaf).all(|f| f.r >= LEAF_R),
            "every folder is at least leaf-sized"
        );
        assert!(discs[0].r > LEAF_R, "the root folder encloses more than one cell");
    }

    #[test]
    fn folder_dishes_are_drawn_not_just_cells() {
        // The petri-dish fix: a folder's muted ring color must appear in the
        // output, not only the bright leaf cells. Before, only leaves were drawn.
        let (rgba, _) = render_galaxy(&galaxy_discs(tiny_tree()), 128);
        let dish = color::ring(GreenCode::Unknown); // root/fa/fb are Unknown folders
        assert!(
            rgba.chunks(4).any(|c| c == dish),
            "expected a folder ring (dish) in the raster, found none"
        );
    }
}
