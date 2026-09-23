/// Level formula (src/services/pixel.ts transaction version):
/// level = (pixelsPainted / LEVEL_BASE_PIXEL)^LEVEL_EXPONENT + 1
pub fn calculate_level(pixels_painted: i64, base: f64, exponent: f64) -> f64 {
    (pixels_painted.max(0) as f64 / base).powf(exponent) + 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_formula_matches_reference() {
        // (4 / 30)^0.65 + 1 ≈ 1.2699 (verified against the JS implementation)
        let lvl = calculate_level(4, 30.0, 0.65);
        assert!((lvl - 1.2699045331720487).abs() < 1e-9);
    }

    #[test]
    fn zero_pixels_is_level_one() {
        assert_eq!(calculate_level(0, 30.0, 0.65), 1.0);
    }
}
