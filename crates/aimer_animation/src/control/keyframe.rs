use crate::primitives::animatable::Animatable;
use crate::primitives::curve::Curve;

/// A single keyframe in a keyframe animation.
///
/// Defines a target value and the easing curve to use when interpolating
/// from the previous keyframe to this one.
#[derive(Debug, Clone)]
pub struct Keyframe<T: Animatable + Clone> {
    pub value: T,
    pub curve: Curve,
}

impl<T: Animatable + Clone> Keyframe<T> {
    pub fn new(value: T, curve: Curve) -> Self {
        Self { value, curve }
    }

    pub fn linear(value: T) -> Self {
        Self {
            value,
            curve: Curve::Linear,
        }
    }
}

/// A multi-step animation defined by keyframes at specific fractions.
///
/// Given a progress `t` (0.0–1.0), `KeyframeAnimation` finds the two bounding
/// keyframes, applies the target keyframe's curve to the local `t`, and lerps
/// between the two values.
///
/// # Example
/// ```rust
/// use self::aimer_animation::KeyframeAnimation;
///
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
    /// Panics if `frames` is empty. Frames are sorted by fraction
    /// automatically.
    pub fn new(mut frames: Vec<(f32, Keyframe<T>)>) -> Self {
        assert!(
            !frames.is_empty(),
            "KeyframeAnimation requires at least one keyframe"
        );
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
    /// - If `t` is before the first keyframe, returns the first keyframe's
    ///   value.
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

        // A NaN progress value used to miss every interval and fall through
        // to the last keyframe. Preserve that behavior before the binary
        // search, whose lower-bound index would otherwise be zero.
        if t.is_nan() {
            return self.frames.last().unwrap().1.value.clone();
        }

        // Find the first keyframe at or after `t`; the preceding entry is the
        // lower bound. Using `<` rather than `<=` preserves the old linear
        // scan's choice of the first interval when fractions are duplicated.
        let upper_index = self.frames.partition_point(|frame| frame.0 < t);
        let (f0, ref kf0) = self.frames[upper_index - 1];
        let (f1, ref kf1) = self.frames[upper_index];
        let range = f1 - f0;
        let local_t = if range > 0.0 { (t - f0) / range } else { 0.0 };
        let curved_t = kf1.curve.transform(local_t);
        kf0.value.lerp(&kf1.value, curved_t)
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
    use std::hint::black_box;
    use std::time::Instant;

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
        let anim = KeyframeAnimation::from_values(&[(0.0, 0.0f32), (0.5, 100.0), (1.0, 0.0)]);
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
    fn test_duplicate_fractions_keep_the_first_interval() {
        let anim = KeyframeAnimation::with_curves(&[
            (0.0, 0.0f32, Curve::Linear),
            (0.5, 10.0, Curve::Linear),
            (0.5, 20.0, Curve::Linear),
            (1.0, 30.0, Curve::Linear),
        ]);

        assert_eq!(anim.at(0.5), 10.0);
        assert!((anim.at(0.75) - 25.0).abs() < 1e-9);
    }

    #[test]
    fn test_nan_progress_falls_back_to_the_last_keyframe() {
        let anim = KeyframeAnimation::from_values(&[(0.0, 10.0f32), (1.0, 100.0)]);

        assert_eq!(anim.at(f32::NAN), 100.0);
    }

    #[test]
    fn test_tuple_keyframes() {
        let anim =
            KeyframeAnimation::from_values(&[(0.0, (0.0f32, 0.0f32)), (1.0, (100.0, 200.0))]);
        let r = anim.at(0.5);
        assert!((r.0 - 50.0).abs() < 1e-9);
        assert!((r.1 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_len() {
        let anim = KeyframeAnimation::from_values(&[(0.0, 0.0f32), (0.5, 50.0), (1.0, 100.0)]);
        assert_eq!(anim.len(), 3);
    }

    #[test]
    #[ignore = "manual numeric-kernel profile"]
    fn profile_keyframe_lookup() {
        const MEASURED: usize = 4_096;
        const WARMUP: usize = 512;
        const ROUNDS: usize = 7;

        for count in [2, 8, 32, 256, 2_048] {
            let values: Vec<_> = (0..count)
                .map(|index| {
                    let fraction = index as f32 / (count - 1) as f32;
                    (fraction, index as f32 * 3.0)
                })
                .collect();
            let animation = KeyframeAnimation::from_values(&values);
            let mut samples = Vec::with_capacity(ROUNDS);
            let mut checksum = 0.0;

            for _ in 0..ROUNDS {
                for index in 0..WARMUP {
                    let t = ((index * 37) % 997) as f32 / 996.0;
                    checksum = black_box(checksum + animation.at(t));
                }

                let start = Instant::now();
                for index in 0..MEASURED {
                    let t = ((index * 37) % 997) as f32 / 996.0;
                    checksum = black_box(checksum + animation.at(t));
                }
                samples.push(start.elapsed().as_secs_f64() * 1e6 / MEASURED as f64);
            }

            samples.sort_by(f64::total_cmp);
            let p50 = samples[ROUNDS / 2];
            let p95 = samples[(ROUNDS * 95).div_ceil(100) - 1];
            println!("{count} keyframes: p50 {p50:.3} us, p95 {p95:.3} us");
            assert!(checksum.is_finite());
        }
    }
}
