//! Reads image dimensions straight from file headers (PNG, JPEG, GIF, WebP).

pub fn dimensions(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() >= 24 && b.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes(b[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(b[20..24].try_into().ok()?);
        return Some((w, h));
    }
    if b.len() >= 10 && (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        let w = u16::from_le_bytes([b[6], b[7]]) as u32;
        let h = u16::from_le_bytes([b[8], b[9]]) as u32;
        return Some((w, h));
    }
    if b.len() >= 30 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return match &b[12..16] {
            b"VP8 " => Some((
                (u16::from_le_bytes([b[26], b[27]]) & 0x3fff) as u32,
                (u16::from_le_bytes([b[28], b[29]]) & 0x3fff) as u32,
            )),
            b"VP8L" => {
                let bits = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
                Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
            }
            b"VP8X" => {
                let w = 1 + (b[24] as u32 | (b[25] as u32) << 8 | (b[26] as u32) << 16);
                let h = 1 + (b[27] as u32 | (b[28] as u32) << 8 | (b[29] as u32) << 16);
                Some((w, h))
            }
            _ => None,
        };
    }
    if b.len() > 4 && b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            // Start-of-frame markers carry the dimensions.
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h));
            }
            if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
                i += 2;
                continue;
            }
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            i += 2 + len;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png() {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend_from_slice(&1200u32.to_be_bytes());
        b.extend_from_slice(&630u32.to_be_bytes());
        assert_eq!(dimensions(&b), Some((1200, 630)));
    }

    #[test]
    fn gif() {
        let mut b = b"GIF89a".to_vec();
        b.extend_from_slice(&[0x2C, 0x01, 0x90, 0x00]); // 300 x 144
        assert_eq!(dimensions(&b), Some((300, 144)));
    }

    #[test]
    fn jpeg() {
        // SOI, APP0 (len 4), SOF0 with 16x8 (height 8, width 16)
        let b = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x08, 0x00, 0x10, 0x03,
            0x00, 0x00,
        ];
        assert_eq!(dimensions(&b), Some((16, 8)));
    }

    #[test]
    fn webp_lossless() {
        let mut b = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0".to_vec();
        b.push(0x2F);
        let bits: u32 = (1200 - 1) | ((630 - 1) << 14);
        b.extend_from_slice(&bits.to_le_bytes());
        b.resize(40, 0); // real files are always longer than the header
        assert_eq!(dimensions(&b), Some((1200, 630)));
    }

    #[test]
    fn unknown() {
        assert_eq!(dimensions(b"<svg></svg>"), None);
    }
}
