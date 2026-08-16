//! A tiny, dependency-free, deterministic PNG writer. 8-bit RGBA, filter 0 on
//! every scanline, zlib with STORED (uncompressed) deflate blocks, no `tIME`
//! chunk, no timestamps. The same pixels always produce the same bytes.

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &x in bytes {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// zlib stream (RFC 1950) wrapping the raw bytes in STORED deflate blocks.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    if raw.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let mut i = 0;
        while i < raw.len() {
            let end = usize::min(i + 65535, raw.len());
            let chunk = &raw[i..end];
            let is_last = end == raw.len();
            out.push(if is_last { 1 } else { 0 }); // BFINAL bit, BTYPE 00 (stored)
            let len = chunk.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(chunk);
            i = end;
        }
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Encode an 8-bit RGBA image to PNG bytes. `rgba.len()` must equal
/// `width * height * 4`. Deterministic: no timestamps, fixed filter/compression.
pub fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width as usize * height as usize * 4, "rgba size");
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity(height as usize * (1 + row));
    for y in 0..height as usize {
        raw.push(0); // filter type 0 (none)
        raw.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }
    let mut out: Vec<u8> = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // depth 8, RGBA, deflate, filter 0, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reconstruct the raw (filter+scanline) bytes from a STORED-deflate zlib
    // stream — no decoder dependency; we know the exact format we emit.
    fn inflate_stored(zlib: &[u8]) -> Vec<u8> {
        let mut raw = Vec::new();
        let mut i = 2; // skip 2-byte zlib header
        loop {
            let is_last = zlib[i] & 1 == 1;
            i += 1;
            let len = u16::from_le_bytes([zlib[i], zlib[i + 1]]) as usize;
            i += 4; // LEN + NLEN
            raw.extend_from_slice(&zlib[i..i + len]);
            i += len;
            if is_last {
                break;
            }
        }
        raw
    }

    // Pull a chunk's data out of a PNG by 4-byte type tag.
    fn chunk_data<'a>(png: &'a [u8], kind: &[u8; 4]) -> &'a [u8] {
        let mut i = 8; // skip signature
        loop {
            let len = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
            let tag = &png[i + 4..i + 8];
            let data = &png[i + 8..i + 8 + len];
            if tag == kind {
                return data;
            }
            i += 12 + len; // len + tag + data + crc
        }
    }

    #[test]
    fn signature_and_ihdr_dims() {
        let rgba = vec![
            0xd0, 0x3b, 0x3b, 0xff, 0x0c, 0xa3, 0x0c, 0xff, // row 0
            0xfa, 0xb2, 0x19, 0xff, 0x6b, 0x6b, 0x6b, 0xff, // row 1
        ];
        let png = encode_rgba_png(2, 2, &rgba);
        assert_eq!(&png[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        let ihdr = chunk_data(&png, b"IHDR");
        assert_eq!(&ihdr[0..4], &2u32.to_be_bytes());
        assert_eq!(&ihdr[4..8], &2u32.to_be_bytes());
        assert_eq!(ihdr[8], 8); // bit depth
        assert_eq!(ihdr[9], 6); // color type RGBA
    }

    #[test]
    fn pixels_round_trip_through_stored_idat() {
        let rgba = vec![
            0xd0, 0x3b, 0x3b, 0xff, 0x0c, 0xa3, 0x0c, 0xff, 0xfa, 0xb2, 0x19, 0xff, 0x6b, 0x6b,
            0x6b, 0xff,
        ];
        let png = encode_rgba_png(2, 2, &rgba);
        let raw = inflate_stored(chunk_data(&png, b"IDAT"));
        // Expect: [0, r0(8 bytes), 0, r1(8 bytes)]
        let expect = vec![
            0u8, 0xd0, 0x3b, 0x3b, 0xff, 0x0c, 0xa3, 0x0c, 0xff, 0, 0xfa, 0xb2, 0x19, 0xff, 0x6b,
            0x6b, 0x6b, 0xff,
        ];
        assert_eq!(raw, expect);
    }

    #[test]
    fn deterministic() {
        let rgba = vec![0u8; 4 * 4 * 4];
        assert_eq!(encode_rgba_png(4, 4, &rgba), encode_rgba_png(4, 4, &rgba));
    }
}
