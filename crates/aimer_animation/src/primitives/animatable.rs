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
}
