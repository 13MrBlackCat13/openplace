use std::collections::HashMap;
use std::sync::OnceLock;

/// 64-color palette ported 1:1 from src/utils/colors.ts.
/// Index 0 is transparent (alpha 0 on the canvas); 1..=31 are free,
/// 32..=63 are paid (unlocked via the extraColorsBitmap).
pub const PALETTE: [(u8, u8, u8); 64] = [
    (0, 0, 0), // 0 — transparent
    (0, 0, 0),
    (60, 60, 60),
    (120, 120, 120),
    (210, 210, 210),
    (255, 255, 255),
    (96, 0, 24),
    (237, 28, 36),
    (255, 127, 39),
    (246, 170, 9),
    (249, 221, 59),
    (255, 250, 188),
    (14, 185, 104),
    (19, 230, 123),
    (135, 255, 94),
    (12, 129, 110),
    (16, 174, 166),
    (19, 225, 190),
    (40, 80, 158),
    (64, 147, 228),
    (96, 247, 242),
    (107, 80, 246),
    (153, 177, 251),
    (120, 12, 153),
    (170, 56, 185),
    (224, 159, 249),
    (203, 0, 122),
    (236, 31, 128),
    (243, 141, 169),
    (104, 70, 52),
    (149, 104, 42),
    (248, 178, 119),
    (170, 170, 170),
    (165, 14, 30),
    (250, 128, 114),
    (228, 92, 26),
    (214, 181, 148),
    (156, 132, 49),
    (197, 173, 49),
    (232, 212, 95),
    (74, 107, 58),
    (90, 148, 74),
    (132, 197, 115),
    (15, 121, 159),
    (187, 250, 242),
    (125, 199, 255),
    (77, 49, 184),
    (74, 66, 132),
    (122, 113, 196),
    (181, 174, 241),
    (219, 164, 99),
    (209, 128, 81),
    (255, 197, 165),
    (155, 82, 73),
    (209, 128, 120),
    (250, 182, 164),
    (123, 99, 82),
    (156, 132, 107),
    (51, 57, 65),
    (109, 117, 141),
    (179, 185, 209),
    (109, 100, 63),
    (148, 140, 107),
    (205, 197, 158),
];

pub const TILE_SIZE: i32 = 1000;
pub const MAX_COLOR_ID: u8 = 63;

pub fn is_paid_color(color_id: u8) -> bool {
    color_id >= 32
}

/// JS: checkColorUnlocked — ids < 32 are always unlocked, otherwise bit
/// `1 << (id - 32)` must be set in extraColorsBitmap.
pub fn check_color_unlocked(color_id: u8, extra_colors_bitmap: i32) -> bool {
    if (color_id as i32) < 32 {
        return true;
    }
    (extra_colors_bitmap >> (color_id - 32)) & 1 == 1
}

/// Flat palette bytes (RGB triplets) for indexed PNG encoding.
pub fn palette_bytes() -> Vec<u8> {
    let mut out = Vec::with_capacity(64 * 3);
    for (r, g, b) in PALETTE {
        out.extend_from_slice(&[r, g, b]);
    }
    out
}

/// tRNS chunk for indexed PNG: index 0 fully transparent, the rest opaque.
pub fn palette_trns() -> Vec<u8> {
    let mut out = vec![0u8];
    out.resize(64, 255);
    out
}

fn reverse_palette() -> &'static HashMap<(u8, u8, u8), u8> {
    static REVERSE: OnceLock<HashMap<(u8, u8, u8), u8>> = OnceLock::new();
    REVERSE.get_or_init(|| {
        let mut m = HashMap::with_capacity(64);
        for (i, rgb) in PALETTE.iter().enumerate().skip(1) {
            m.insert(*rgb, i as u8);
        }
        m
    })
}

/// Map an RGBA pixel (from a decoded tile PNG) back to a palette color id.
/// Alpha 0 → 0 (transparent). Unknown colors are snapped to the nearest
/// palette entry (legacy blobs produced by sharp may contain dithered pixels).
pub fn rgba_to_color_id(r: u8, g: u8, b: u8, a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    if let Some(id) = reverse_palette().get(&(r, g, b)) {
        return *id;
    }
    let mut best = 0u8;
    let mut best_dist = u32::MAX;
    for (i, (pr, pg, pb)) in PALETTE.iter().enumerate().skip(1) {
        let dr = r as i32 - *pr as i32;
        let dg = g as i32 - *pg as i32;
        let db = b as i32 - *pb as i32;
        let d = (dr * dr + dg * dg + db * db) as u32;
        if d < best_dist {
            best_dist = d;
            best = i as u8;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_colors_always_unlocked() {
        assert!(check_color_unlocked(0, 0));
        assert!(check_color_unlocked(31, 0));
    }

    #[test]
    fn paid_colors_require_bitmap_bit() {
        assert!(!check_color_unlocked(32, 0));
        assert!(check_color_unlocked(32, 1));
        assert!(check_color_unlocked(63, 1 << 31));
    }

    #[test]
    fn palette_has_64_entries() {
        assert_eq!(PALETTE.len(), 64);
        assert_eq!(palette_bytes().len(), 192);
        assert_eq!(palette_trns()[0], 0);
        assert_eq!(palette_trns()[63], 255);
    }

    #[test]
    fn rgba_mapping() {
        assert_eq!(rgba_to_color_id(0, 0, 0, 0), 0);
        assert_eq!(rgba_to_color_id(237, 28, 36, 255), 7);
        assert_eq!(rgba_to_color_id(205, 197, 158, 255), 63);
    }
}
