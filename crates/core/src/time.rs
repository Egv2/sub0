//! Float seconds from external tools are converted to `u32` ms here and nowhere else.

/// Negative inputs clamp to 0; huge ones saturate at `u32::MAX`.
pub fn secs_to_ms(secs: f64) -> u32 {
    (secs.max(0.0) * 1000.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_to_nearest_ms() {
        assert_eq!(secs_to_ms(1.2345), 1235);
        assert_eq!(secs_to_ms(1.2344), 1234);
        assert_eq!(secs_to_ms(0.0), 0);
    }

    #[test]
    fn clamps_negative_and_saturates() {
        assert_eq!(secs_to_ms(-1.5), 0);
        assert_eq!(secs_to_ms(1.0e12), u32::MAX);
    }
}
