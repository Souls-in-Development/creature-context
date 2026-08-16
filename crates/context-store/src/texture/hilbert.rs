//! Hilbert space-filling curve: map a 1D index to a 2D cell so that
//! consecutive indices land in nearby cells. `side` must be a power of two.

/// Map distance `d` along the Hilbert curve of a `side`×`side` grid to `(x, y)`.
/// `side` must be a power of two; `d` must be in `[0, side*side)`.
pub fn d2xy(side: u32, d: u32) -> (u32, u32) {
    let (mut x, mut y) = (0u32, 0u32);
    let mut t = d;
    let mut s = 1u32;
    while s < side {
        let rx = 1 & (t / 2);
        let ry = 1 & (t ^ rx);
        if ry == 0 {
            if rx == 1 {
                x = s - 1 - x;
                y = s - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        x += s * rx;
        y += s * ry;
        t /= 4;
        s *= 2;
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values_for_side_2() {
        assert_eq!(d2xy(2, 0), (0, 0));
        assert_eq!(d2xy(2, 1), (0, 1));
        assert_eq!(d2xy(2, 2), (1, 1));
        assert_eq!(d2xy(2, 3), (1, 0));
    }

    #[test]
    fn bijection_for_side_4() {
        let side = 4u32;
        let mut seen = std::collections::BTreeSet::new();
        for d in 0..side * side {
            let (x, y) = d2xy(side, d);
            assert!(x < side && y < side, "cell in bounds");
            assert!(seen.insert((x, y)), "cell {:?} used twice", (x, y));
        }
        assert_eq!(seen.len(), (side * side) as usize);
    }
}
