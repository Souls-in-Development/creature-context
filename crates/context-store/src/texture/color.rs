//! Green status → RGBA. Fixed hex from the spec; alpha 255 for a status,
//! fully transparent for an unused cell.

use creature_context_types::GreenCode;

/// A cell with no entity: fully transparent.
pub const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

/// The color for a Green status. Fixed and exact — determinism depends on it.
pub const fn rgba(code: GreenCode) -> [u8; 4] {
    match code {
        GreenCode::Green => [0x0c, 0xa3, 0x0c, 0xff],
        GreenCode::Yellow => [0xfa, 0xb2, 0x19, 0xff],
        GreenCode::Red => [0xd0, 0x3b, 0x3b, 0xff],
        GreenCode::Unknown => [0x6b, 0x6b, 0x6b, 0xff],
    }
}

/// A muted, dark version of a status color for a folder's ring — the "dish" the
/// leaf "cells" sit in. Same hue, roughly half brightness, full alpha, so a
/// folder's boundary reads as structure behind the solid leaf discs rather than
/// competing with them. Integer-only (const, no overflow: max is 127+10).
pub const fn ring(code: GreenCode) -> [u8; 4] {
    let [r, g, b, _] = rgba(code);
    [r / 2 + 10, g / 2 + 10, b / 2 + 10, 0xff]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_is_muted_but_same_alpha() {
        // Dimmer than the solid cell, still opaque, and hue-preserving (a red
        // folder's ring stays red-dominant).
        let solid = rgba(GreenCode::Red);
        let dish = ring(GreenCode::Red);
        assert_eq!(dish[3], 0xff);
        assert!(dish[0] < solid[0], "ring is darker than the cell");
        assert!(dish[0] > dish[1] && dish[0] > dish[2], "red stays dominant");
    }

    #[test]
    fn each_code_maps_to_its_hex() {
        assert_eq!(rgba(GreenCode::Green), [0x0c, 0xa3, 0x0c, 0xff]);
        assert_eq!(rgba(GreenCode::Yellow), [0xfa, 0xb2, 0x19, 0xff]);
        assert_eq!(rgba(GreenCode::Red), [0xd0, 0x3b, 0x3b, 0xff]);
        assert_eq!(rgba(GreenCode::Unknown), [0x6b, 0x6b, 0x6b, 0xff]);
        assert_eq!(TRANSPARENT, [0, 0, 0, 0]);
    }
}
