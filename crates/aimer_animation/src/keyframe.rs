use crate::animatable::Animatable;
use crate::curve::Curve;
use aimer_macro::Constructor;

/// A single keyframe in a keyframe animation.
///
/// Defines a target value and the easing curve to use when interpolating
/// from the previous keyframe to this one.
#[derive(Debug, Clone, Constructor)]
pub struct Keyframe<T: Animatable + Clone> {
    pub value: T,
    pub curve: Curve,
}

impl<T: Animatable + Clone> Keyframe<T> {
    pub fn new(value: T, curve: Curve) -> Self {
        Self { value, curve }
    }

    pub fn linear(value: T) -> Self {
        Self { value, curve: Curve::Linear }
    }
}

/// A multi-step animation defined by keyframes at specific fractions.
///
/// Given a progress `t` (0.0–1.0), `KeyframeAnimation` finds the two bounding
/// keyframes, applies the target keyframe's curve to the local `t`, and lerps
/// between the two values.
///
/// # Example
/// ```ignore
/// let anim = KeyframeAnimation::from_values(&[
///     (0.0, 0.0f32),
///     (0.5, 100.0),   // peak at halfway
///     (1.0, 0.0),     // back to start
/// ]);
/// let value = anim.at(0.75); // interpolated between 100.0 and 0.0
/// ```
#[derive(Debug, Clone)]
pub struct KeyframeAnimation<T: Animatable + Clone> {
    /// Sorted by fraction (ascending). Each entry is (fraction, keyframe).
    frames: Vec<(f32, Keyframe<T>)>,
}

impl<T: Animatable + Clone> KeyframeAnimation<T> {
    /// Create a keyframe animation from a list of (fraction, keyframe) pairs.
    ///
    /// Panics if `frames` is empty. Frames are sorted by fraction automatically.
    pub fn new(mut frames: Vec<(f32, Keyframe<T>)>) -> Self {
        assert!(!frames.is_empty(), "KeyframeAnimation requires at least one keyframe");
        frames.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Self { frames }
    }

    /// Create a keyframe animation from (fraction, value) pairs using linear
    /// interpolation between each pair.
    pub fn from_values(values: &[(f32, T)]) -> Self {
        let frames = values
            .iter()
            .map(|(f, v)| (*f, Keyframe::linear(v.clone())))
            .collect();
        Self::new(frames)
    }

    /// Create a keyframe animation from (fraction, value, curve) triples.
    pub fn with_curves(entries: &[(f32, T, Curve)]) -> Self {
        let frames = entries
            .iter()
            .map(|(f, v, c)| (*f, Keyframe::new(v.clone(), *c)))
            .collect();
        Self::new(frames)
    }

    /// Evaluate the animation at progress `t` (0.0–1.0).
    ///
    /// - If `t` is before the first keyframe, returns the first keyframe's value.
    /// - If `t` is after the last keyframe, returns the last keyframe's value.
    /// - Otherwise, interpolates between the two bounding keyframes.
    pub fn at(&self, t: f32) -> T {
        let t = t.clamp(0.0, 1.0);

        // Before first keyframe
        if t <= self.frames[0].0 {
            return self.frames[0].1.value.clone();
        }

        // After last keyframe
        if t >= self.frames.last().unwrap().0 {
            return self.frames.last().unwrap().1.value.clone();
        }

        // Find bounding keyframes
        for i in 0..self.frames.len() - 1 {
            let (f0, ref kf0) = self.frames[i];
            let (f1, ref kf1) = self.frames[i + 1];

            if t >= f0 && t <= f1 {
                let range = f1 - f0;
                let local_t = if range > 0.0 { (t - f0) / range } else { 0.0 };
                let curved_t = kf1.curve.transform(local_t);
                return kf0.value.lerp(&kf1.value, curved_t);
            }
        }

        // Fallback (should not reach here)
        self.frames.last().unwrap().1.value.clone()
    }

    /// Returns the number of keyframes.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Returns `true` if there are no keyframes.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// Make `KeyframeAnimation` itself `Animatable` so it can be used in tweens.
impl<T: Animatable + Clone> Animatable for KeyframeAnimation<T> {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        // Interpolate between two keyframe animations by evaluating both at t
        // and creating a simple two-keyframe animation from the results.
        // This is a pragmatic approach — for most use cases, use `.at(t)` directly.
        let val_a = self.at(t);
        let val_b = other.at(t);
        KeyframeAnimation::from_values(&[(0.0, val_a), (1.0, val_b)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_keyframes() {
        let anim = KeyframeAnimation::from_values(&[(0.0, 0.0f32), (1.0, 100.0)]);
        assert!((anim.at(0.0) - 0.0).abs() < 1e-9);
        assert!((anim.at(0.5) - 50.0).abs() < 1e-9);
        assert!((anim.at(1.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_three_keyframes() {
        let anim = KeyframeAnimation::from_values(&[
            (0.0, 0.0f32),
            (0.5, 100.0),
            (1.0, 0.0),
        ]);
        assert!((anim.at(0.0) - 0.0).abs() < 1e-9);
        assert!((anim.at(0.25) - 50.0).abs() < 1e-9);
        assert!((anim.at(0.5) - 100.0).abs() < 1e-9);
        assert!((anim.at(0.75) - 50.0).abs() < 1e-9);
        assert!((anim.at(1.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_clamped_before_first() {
        let anim = KeyframeAnimation::from_values(&[(0.2, 10.0f32), (1.0, 100.0)]);
        assert!((anim.at(0.0) - 10.0).abs() < 1e-9);
        assert!((anim.at(0.1) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_clamped_after_last() {
        let anim = KeyframeAnimation::from_values(&[(0.0, 10.0f32), (0.8, 100.0)]);
        assert!((anim.at(0.9) - 100.0).abs() < 1e-9);
        assert!((anim.at(1.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_with_curves() {
        let anim = KeyframeAnimation::with_curves(&[
            (0.0, 0.0f32, Curve::Linear),
            (0.5, 100.0, Curve::EaseIn),
            (1.0, 0.0, Curve::Linear),
        ]);
        // At 0.25, we're between 0.0 and 100.0 with EaseIn curve
        let val = anim.at(0.25);
        // EaseIn(t=0.5) = 0.5^3 = 0.125, so lerp(0, 100, 0.125) = 12.5
        assert!((val - 12.5).abs() < 1.0);
    }

    #[test]
    fn test_tuple_keyframes() {
        let anim = KeyframeAnimation::from_values(&[
            (0.0, (0.0f32, 0.0f32)),
            (1.0, (100.0, 200.0)),
        ]);
        let r = anim.at(0.5);
        assert!((r.0 - 50.0).abs() < 1e-9);
        assert!((r.1 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_len() {
        let anim = KeyframeAnimation::from_values(&[(0.0, 0.0f32), (0.5, 50.0), (1.0, 100.0)]);
        assert_eq!(anim.len(), 3);
    }
}
