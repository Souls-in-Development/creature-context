//! Deterministic circle packing — the D3 front-chain algorithm, sqrt-only (no
//! trig, no RNG), so `pack_siblings` gives bit-for-bit identical output on every
//! platform. Places sibling circles tangent and non-overlapping in O(n).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub r: f64,
}

/// Position `circles[c]` tangent to `circles[p]` and `circles[q]`. Mirrors D3's
/// `place(b, a, c)` with `p` as D3's `b` and `q` as D3's `a`. Pure algebra (law
/// of cosines) + one `sqrt` — no trig, so the chosen tangency point is identical
/// on every platform.
fn place(circles: &mut [Circle], p: usize, q: usize, c: usize) {
    let (bx, by, br) = (circles[p].x, circles[p].y, circles[p].r);
    let (ax, ay, ar) = (circles[q].x, circles[q].y, circles[q].r);
    let cr = circles[c].r;
    let dx = bx - ax;
    let dy = by - ay;
    let d2 = dx * dx + dy * dy;
    if d2 > 0.0 {
        let a2 = (ar + cr) * (ar + cr);
        let b2 = (br + cr) * (br + cr);
        if a2 > b2 {
            let x = (d2 + b2 - a2) / (2.0 * d2);
            let y = (b2 / d2 - x * x).max(0.0).sqrt();
            circles[c].x = bx - x * dx - y * dy;
            circles[c].y = by - x * dy + y * dx;
        } else {
            let x = (d2 + a2 - b2) / (2.0 * d2);
            let y = (a2 / d2 - x * x).max(0.0).sqrt();
            circles[c].x = ax + x * dx - y * dy;
            circles[c].y = ay + x * dy + y * dx;
        }
    } else {
        circles[c].x = ax + cr;
        circles[c].y = ay;
    }
}

fn intersects(circles: &[Circle], a: usize, b: usize) -> bool {
    let dr = circles[a].r + circles[b].r - 1e-6;
    let dx = circles[b].x - circles[a].x;
    let dy = circles[b].y - circles[a].y;
    dr > 0.0 && dr * dr > dx * dx + dy * dy
}

fn score(circles: &[Circle], node: usize, next: usize) -> f64 {
    let a = circles[node];
    let b = circles[next];
    let ab = a.r + b.r;
    let dx = (a.x * b.r + b.x * a.r) / ab;
    let dy = (a.y * b.r + b.y * a.r) / ab;
    dx * dx + dy * dy
}

/// Pack `radii.len()` circles tangent and non-overlapping, returning each
/// circle's center `(x, y)` in the packing's own frame (index order matches
/// input). O(n) front chain; deterministic.
pub fn pack_siblings(radii: &[f64]) -> Vec<(f64, f64)> {
    let n = radii.len();
    let mut c: Vec<Circle> = radii
        .iter()
        .map(|&r| Circle { x: 0.0, y: 0.0, r })
        .collect();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![(0.0, 0.0)];
    }
    // First two circles tangent along the x axis.
    c[0].x = -c[1].r;
    c[0].y = 0.0;
    c[1].x = c[0].r;
    c[1].y = 0.0;
    if n == 2 {
        return c.iter().map(|z| (z.x, z.y)).collect();
    }
    // Third circle tangent to circles 1 and 0.
    place(&mut c, 1, 0, 2);

    let mut next = vec![0usize; n];
    let mut prev = vec![0usize; n];
    let (mut a, mut b) = (0usize, 1usize);
    let cc = 2usize;
    next[a] = b;
    prev[cc] = b;
    next[b] = cc;
    prev[a] = cc;
    next[cc] = a;
    prev[b] = a;

    let mut i = 3usize;
    'pack: while i < n {
        place(&mut c, a, b, i); // circle i tangent to a and b
        let mut j = next[b];
        let mut k = prev[a];
        let mut sj = c[b].r;
        let mut sk = c[a].r;
        loop {
            if sj <= sk {
                if intersects(&c, j, i) {
                    b = j;
                    next[a] = b;
                    prev[b] = a;
                    continue 'pack; // retry circle i with the new (a, b)
                }
                sj += c[j].r;
                j = next[j];
            } else {
                if intersects(&c, k, i) {
                    a = k;
                    next[a] = b;
                    prev[b] = a;
                    continue 'pack;
                }
                sk += c[k].r;
                k = prev[k];
            }
            if j == next[k] {
                break;
            }
        }
        // Insert circle i between a and b.
        prev[i] = a;
        next[i] = b;
        next[a] = i;
        prev[b] = i;
        b = i;
        // New closest pair to the centroid becomes the next (a, b).
        let mut aa = score(&c, a, next[a]);
        let mut node = next[i];
        while node != b {
            let ca = score(&c, node, next[node]);
            if ca < aa {
                a = node;
                aa = ca;
            }
            node = next[node];
        }
        b = next[a];
        i += 1;
    }
    c.iter().map(|z| (z.x, z.y)).collect()
}

/// The **minimal** enclosing circle of `circles` — Welzl's algorithm, the second
/// half of D3's circle pack (`packEnclose`). `pack_siblings` places the siblings;
/// `enclose` wraps them as tightly as possible so a parent bubble hugs its
/// children. The old version used the centroid as the center, which both
/// off-centered the parent and inflated its radius — the slack that made the
/// galaxy look mostly empty with its contents shoved to one side.
///
/// Deterministic: processed in fixed input order (D3 shuffles for expected linear
/// time; correctness is order-independent, and sibling counts are small, so the
/// fixed-order O(n²) path is both exact and reproducible — no RNG). Pure
/// arithmetic + sqrt, no trig.
pub fn enclose(circles: &[Circle]) -> Circle {
    let mut e: Option<Circle> = None;
    let n = circles.len();
    let mut i = 0;
    while i < n {
        let ci = circles[i];
        if e.map_or(true, |c| !encloses(c, ci)) {
            e = Some(ci); // basis 1: the circle itself
            let mut j = 0;
            while j < i {
                let cj = circles[j];
                if !encloses(e.unwrap(), cj) {
                    e = Some(enclose2(ci, cj));
                    let mut k = 0;
                    while k < j {
                        let ck = circles[k];
                        if !encloses(e.unwrap(), ck) {
                            e = Some(enclose3(ci, cj, ck));
                        }
                        k += 1;
                    }
                }
                j += 1;
            }
        }
        i += 1;
    }
    e.unwrap_or(Circle { x: 0.0, y: 0.0, r: 0.0 })
}

/// Whether circle `a` (weakly) contains circle `b`. Mirrors D3's `enclosesWeak`
/// epsilon so boundary circles count as enclosed and Welzl terminates.
fn encloses(a: Circle, b: Circle) -> bool {
    let dr = a.r - b.r + a.r.max(b.r).max(1.0) * 1e-9;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dr > 0.0 && dr * dr > dx * dx + dy * dy
}

/// Smallest circle internally tangent to both `a` and `b` (D3 `encloseBasis2`).
fn enclose2(a: Circle, b: Circle) -> Circle {
    let (x1, y1, r1) = (a.x, a.y, a.r);
    let (x2, y2, r2) = (b.x, b.y, b.r);
    let x21 = x2 - x1;
    let y21 = y2 - y1;
    let r21 = r2 - r1;
    let l = (x21 * x21 + y21 * y21).sqrt();
    if l == 0.0 {
        // Coincident centers (shouldn't occur among packed siblings) — the larger
        // circle already encloses the smaller; return it rather than divide by 0.
        return if r1 >= r2 { a } else { b };
    }
    Circle {
        x: (x1 + x2 + x21 / l * r21) / 2.0,
        y: (y1 + y2 + y21 / l * r21) / 2.0,
        r: (l + r1 + r2) / 2.0,
    }
}

/// Smallest circle internally tangent to all three (D3 `encloseBasis3`).
fn enclose3(a: Circle, b: Circle, c: Circle) -> Circle {
    let (x1, y1, r1) = (a.x, a.y, a.r);
    let (x2, y2, r2) = (b.x, b.y, b.r);
    let (x3, y3, r3) = (c.x, c.y, c.r);
    let a2 = x1 - x2;
    let a3 = x1 - x3;
    let b2 = y1 - y2;
    let b3 = y1 - y3;
    let c2 = r2 - r1;
    let c3 = r3 - r1;
    let d1 = x1 * x1 + y1 * y1 - r1 * r1;
    let d2 = d1 - x2 * x2 - y2 * y2 + r2 * r2;
    let d3 = d1 - x3 * x3 - y3 * y3 + r3 * r3;
    let ab = a3 * b2 - a2 * b3;
    if ab == 0.0 {
        // Collinear centers — degenerate; fall back to the pair that spans widest.
        return enclose2(a, b);
    }
    let xa = (b2 * d3 - b3 * d2) / (ab * 2.0) - x1;
    let xb = (b3 * c2 - b2 * c3) / ab;
    let ya = (a3 * d2 - a2 * d3) / (ab * 2.0) - y1;
    let yb = (a2 * c3 - a3 * c2) / ab;
    let aa = xb * xb + yb * yb - 1.0;
    let bb = 2.0 * (r1 + xa * xb + ya * yb);
    let cc = xa * xa + ya * ya - r1 * r1;
    let r = if aa != 0.0 {
        -(bb + (bb * bb - 4.0 * aa * cc).sqrt()) / (2.0 * aa)
    } else {
        -cc / bb
    };
    Circle {
        x: x1 + xa + xb * r,
        y: y1 + ya + yb * r,
        r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circles_from(radii: &[f64], centers: &[(f64, f64)]) -> Vec<Circle> {
        radii
            .iter()
            .zip(centers)
            .map(|(&r, &(x, y))| Circle { x, y, r })
            .collect()
    }

    #[test]
    fn two_equal_circles_are_tangent() {
        let centers = pack_siblings(&[1.0, 1.0]);
        let dx = centers[1].0 - centers[0].0;
        let dy = centers[1].1 - centers[0].1;
        assert!(((dx * dx + dy * dy).sqrt() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn no_pair_overlaps() {
        let radii: Vec<f64> = (1..=40).map(|i| 1.0 + (i % 5) as f64 * 0.3).collect();
        let centers = pack_siblings(&radii);
        for a in 0..radii.len() {
            for b in (a + 1)..radii.len() {
                let dx = centers[b].0 - centers[a].0;
                let dy = centers[b].1 - centers[a].1;
                let d = (dx * dx + dy * dy).sqrt();
                assert!(
                    d >= radii[a] + radii[b] - 1e-6,
                    "circles {a} and {b} overlap: d={d}, r+r={}",
                    radii[a] + radii[b]
                );
            }
        }
    }

    #[test]
    fn deterministic() {
        let radii: Vec<f64> = (1..=30).map(|i| 1.0 + (i % 7) as f64 * 0.4).collect();
        assert_eq!(pack_siblings(&radii), pack_siblings(&radii));
    }

    #[test]
    fn enclose_contains_every_circle() {
        let radii = vec![1.0, 1.5, 0.8, 1.2, 2.0, 0.6];
        let centers = pack_siblings(&radii);
        let circles = circles_from(&radii, &centers);
        let e = enclose(&circles);
        for c in &circles {
            let d = ((c.x - e.x).powi(2) + (c.y - e.y).powi(2)).sqrt();
            assert!(d + c.r <= e.r + 1e-9, "circle escapes the enclosing circle");
        }
    }
}
