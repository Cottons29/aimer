use aimer_widget::base::Color;

/// Trait for types that can be linearly interpolated.
///
/// Implement this on any type you want to animate between two values.
/// The framework provides implementations for common numeric types and tuples.
pub trait Animatable {
    /// Linearly interpolate from `self` to `other` by factor `t` (0.0–1.0).
    fn lerp(&self, other: &Self, t: f32) -> Self;
}

impl Animatable for f32 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl Animatable for f64 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t as f64;
        self + (other - self) * t
    }
}

impl Animatable for i32 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        (*self as f32 + (*other - *self) as f32 * t).round() as i32
    }
}

impl Animatable for i64 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        (*self as f64 + (*other - *self) as f64 * t as f64).round() as i64
    }
}

impl Animatable for u8 {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        (*self as f32 + (*other as f32 - *self as f32) * t).round() as u8
    }
}
impl Animatable for Color {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        let rgba= self.to_rgba();
        let rgba2= other.to_rgba();

        let red = rgba.0.lerp(&rgba2.0, t);
        let green = rgba.1.lerp(&rgba2.1, t);
        let blue = rgba.2.lerp(&rgba2.2, t);
        let alpha = rgba.3.lerp(&rgba2.3, t);

        Color::Rgba(red, green, blue, alpha)
    }
}

/// 2D point / offset interpolation.
impl Animatable for (f32, f32) {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        (self.0.lerp(&other.0, t), self.1.lerp(&other.1, t))
    }
}

/// RGBA color component interpolation (each component 0.0–1.0).
impl Animatable for (f32, f32, f32, f32) {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        (
            self.0.lerp(&other.0, t),
            self.1.lerp(&other.1, t),
            self.2.lerp(&other.2, t),
            self.3.lerp(&other.3, t),
        )
    }
}

/// 3D vector interpolation.
impl Animatable for (f32, f32, f32) {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        (
            self.0.lerp(&other.0, t),
            self.1.lerp(&other.1, t),
            self.2.lerp(&other.2, t),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::hint::black_box;
    use std::time::Instant;

    use super::*;

    #[test]
    fn test_f32_lerp() {
        assert!((0.0f32.lerp(&10.0, 0.0) - 0.0).abs() < 1e-9);
        assert!((0.0f32.lerp(&10.0, 0.5) - 5.0).abs() < 1e-9);
        assert!((0.0f32.lerp(&10.0, 1.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_f64_lerp() {
        assert!((0.0f64.lerp(&10.0, 0.5) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_i32_lerp_rounds() {
        assert_eq!(0i32.lerp(&10, 0.5), 5);
        assert_eq!(0i32.lerp(&10, 0.3), 3);
    }

    #[test]
    fn test_tuple2_lerp() {
        let a = (0.0, 0.0);
        let b = (10.0, 20.0);
        let r = a.lerp(&b, 0.5);
        assert!((r.0 - 5.0).abs() < 1e-9);
        assert!((r.1 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_tuple4_lerp() {
        let a = (0.0, 0.0, 0.0, 1.0);
        let b = (1.0, 1.0, 1.0, 0.0);
        let r = a.lerp(&b, 0.5);
        assert!((r.0 - 0.5).abs() < 1e-9);
        assert!((r.3 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_u8_lerp() {
        assert_eq!(0u8.lerp(&255, 0.5), 128);
    }

    #[test]
    #[ignore = "manual numeric-kernel profile"]
    fn profile_animatable_interpolation() {
        const MEASURED: usize = 4_096;
        const WARMUP: usize = 512;
        const ROUNDS: usize = 7;

        let mut samples = Vec::with_capacity(ROUNDS);
        let mut checksum = 0.0;
        for _ in 0..ROUNDS {
            let begin = 0.0f32;
            let end = 100.0f32;
            for index in 0..WARMUP {
                let t = index as f32 / WARMUP as f32;
                checksum = black_box(checksum + begin.lerp(&end, t));
            }

            let start = Instant::now();
            for index in 0..MEASURED {
                let t = index as f32 / MEASURED as f32;
                checksum = black_box(checksum + begin.lerp(&end, t));
            }
            samples.push(start.elapsed().as_secs_f64() * 1e6 / MEASURED as f64);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "f32: p50 {:.3} us, p95 {:.3} us",
            samples[ROUNDS / 2],
            samples[(ROUNDS * 95).div_ceil(100) - 1]
        );

        let mut tuple_samples = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let begin = (0.0f32, 10.0, 20.0, 1.0);
            let end = (100.0f32, 110.0, 120.0, 0.0);
            for index in 0..WARMUP {
                let t = index as f32 / WARMUP as f32;
                let value = begin.lerp(&end, t);
                checksum = black_box(checksum + value.0 + value.1 + value.2 + value.3);
            }

            let start = Instant::now();
            for index in 0..MEASURED {
                let t = index as f32 / MEASURED as f32;
                let value = begin.lerp(&end, t);
                checksum = black_box(checksum + value.0 + value.1 + value.2 + value.3);
            }
            tuple_samples.push(start.elapsed().as_secs_f64() * 1e6 / MEASURED as f64);
        }
        tuple_samples.sort_by(f64::total_cmp);
        println!(
            "tuple4: p50 {:.3} us, p95 {:.3} us",
            tuple_samples[ROUNDS / 2],
            tuple_samples[(ROUNDS * 95).div_ceil(100) - 1]
        );
        assert!(checksum.is_finite());
    }
}
