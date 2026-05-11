/// Easing curves for animations.
///
/// Each variant maps a linear progress `t ∈ [0.0, 1.0]` to a curved value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Cubic bezier defined by two control points (x1, y1, x2, y2).
    CubicBezier(f32, f32, f32, f32),
    /// Decelerate curve (1 - (1-t)^2).
    Decelerate,
    /// Bounce at the end.
    BounceOut,
}

impl Curve {
    /// Transform a linear progress value `t` (0.0–1.0) through this curve.
    pub fn transform(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Curve::Linear => t,
            Curve::EaseIn => t * t * t,
            Curve::EaseOut => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
            Curve::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let inv = -2.0 * t + 2.0;
                    1.0 - inv * inv * inv / 2.0
                }
            }
            Curve::CubicBezier(x1, y1, x2, y2) => {
                cubic_bezier_y_for_x(t, *x1, *y1, *x2, *y2)
            }
            Curve::Decelerate => {
                let inv = 1.0 - t;
                1.0 - inv * inv
            }
            Curve::BounceOut => bounce_out(t),
        }
    }
}
#[allow(clippy::derivable_impls)]
impl Default for Curve {
    fn default() -> Self {
        Curve::Linear
    }
}

fn bounce_out(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984375
    }
}

/// Approximate cubic bezier: find y for a given x using Newton's method.
fn cubic_bezier_y_for_x(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Find t such that bezier_x(t) ≈ x, then return bezier_y(t).
    let mut t = x; // initial guess
    for _ in 0..8 {
        let bx = bezier(t, x1, x2);
        let dx = bezier_derivative(t, x1, x2);
        if dx.abs() < 1e-12 {
            break;
        }
        t -= (bx - x) / dx;
        t = t.clamp(0.0, 1.0);
    }
    bezier(t, y1, y2)
}

/// Evaluate cubic bezier at parameter t with control points p1, p2 (p0=0, p3=1).
fn bezier(t: f32, p1: f32, p2: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * t * p1 + 3.0 * inv * t * t * p2 + t * t * t
}

/// Derivative of the cubic bezier.
fn bezier_derivative(t: f32, p1: f32, p2: f32) -> f32 {
    let inv = 1.0 - t;
    3.0 * inv * inv * p1 + 6.0 * inv * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear() {
        assert!((Curve::Linear.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((Curve::Linear.transform(0.5) - 0.5).abs() < 1e-9);
        assert!((Curve::Linear.transform(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_ease_in_boundaries() {
        assert!((Curve::EaseIn.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((Curve::EaseIn.transform(1.0) - 1.0).abs() < 1e-9);
        // ease-in should be slower at start
        assert!(Curve::EaseIn.transform(0.5) < 0.5);
    }

    #[test]
    fn test_ease_out_boundaries() {
        assert!((Curve::EaseOut.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((Curve::EaseOut.transform(1.0) - 1.0).abs() < 1e-9);
        // ease-out should be faster at start
        assert!(Curve::EaseOut.transform(0.5) > 0.5);
    }

    #[test]
    fn test_ease_in_out_boundaries() {
        assert!((Curve::EaseInOut.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((Curve::EaseInOut.transform(1.0) - 1.0).abs() < 1e-9);
        assert!((Curve::EaseInOut.transform(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_clamp() {
        assert!((Curve::Linear.transform(-0.5) - 0.0).abs() < 1e-9);
        assert!((Curve::Linear.transform(1.5) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_bounce_out_boundaries() {
        assert!((Curve::BounceOut.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((Curve::BounceOut.transform(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_decelerate() {
        assert!((Curve::Decelerate.transform(0.0) - 0.0).abs() < 1e-9);
        assert!((Curve::Decelerate.transform(1.0) - 1.0).abs() < 1e-9);
        assert!(Curve::Decelerate.transform(0.5) > 0.5);
    }
}
