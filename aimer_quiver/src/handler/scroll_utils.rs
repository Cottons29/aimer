use aimer_attribute::Vec2d;
use aimer_utils::AnimInstant as Instant;
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

/// Pixels-per-line heuristic for native platforms where LineDelta
/// actually occurs (macOS/Linux/Windows with certain mice). Not used
/// on wasm since winit always reports PixelDelta there.
const LINE_HEIGHT_PX: f64 = 20.0;

pub fn to_pixel_delta(delta: MouseScrollDelta, scale_factor: f64) -> PhysicalPosition<f64> {
    match delta {
        MouseScrollDelta::PixelDelta(pos) => pos,
        MouseScrollDelta::LineDelta(x, y) => PhysicalPosition::new(
            x as f64 * LINE_HEIGHT_PX * scale_factor,
            y as f64 * LINE_HEIGHT_PX * scale_factor,
        ),
    }
}

/// A frame-rate-independent input-distance smoother.
///
/// Despite its historical name, this type does not derive or retain momentum.
/// It stores only undelivered input distance, subtracting every emitted frame
/// step until the exact total has been consumed.
pub struct MomentumScroller {
    remaining: Vec2d,
    pub(crate) pixels_per_line: f32,
    pub(crate) friction: f32,
    min_velocity: f32,
    pub(crate) max_velocity: f32,
    last_tick: Instant,
}

impl MomentumScroller {
    pub fn new() -> Self {
        Self {
            remaining: Vec2d::ZERO,
            pixels_per_line: 40.0,
            friction: 0.65,
            min_velocity: 0.01,
            max_velocity: 60.0,
            last_tick: Instant::now(),
        }
    }

    /// Adds an input delta to the exact distance still to be delivered.
    ///
    /// New input extends the remaining distance. It does not estimate a
    /// velocity, so the smoother cannot manufacture inertial travel after the
    /// device stops reporting deltas.
    pub fn on_line_delta(&mut self, delta: Vec2d) {
        self.remaining = self.remaining + delta.scale(self.pixels_per_line);
    }

    pub fn clear(&mut self) {
        self.remaining = Vec2d::ZERO;
        self.last_tick = Instant::now();
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.remaining.magnitude() > 0.0
    }

    /// Returns the next frame-synchronized portion of the pending input.
    ///
    /// The response is frame-rate independent, bounded by `max_velocity`, and
    /// always removed from `remaining`. The final frame emits the residue
    /// exactly, preserving total input distance without overshoot.
    pub fn tick(&mut self) -> Option<PhysicalPosition<f64>> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;
        self.tick_with_dt(dt)
    }

    fn tick_with_dt(&mut self, dt: f64) -> Option<PhysicalPosition<f64>> {
        let magnitude = self.remaining.magnitude();
        if magnitude == 0.0 {
            return None;
        }

        if magnitude <= self.min_velocity {
            let final_step = self.remaining;
            self.remaining = Vec2d::ZERO;
            return Some(PhysicalPosition::new(
                final_step.x as f64,
                final_step.y as f64,
            ));
        }

        let frame_ratio = dt.clamp(1.0 / 120.0, 1.0 / 30.0) * 60.0;
        let response = 1.0 - (self.friction as f64).powf(frame_ratio);
        let mut step = self.remaining.scale(response as f32);
        let step_magnitude = step.magnitude();
        if step_magnitude > self.max_velocity {
            step = step.scale(self.max_velocity / step_magnitude);
        }
        self.remaining = Vec2d {
            x: self.remaining.x - step.x,
            y: self.remaining.y - step.y,
        };

        Some(PhysicalPosition::new(step.x as f64, step.y as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_preserves_distance_without_adding_momentum() {
        let mut scroller = MomentumScroller::new();
        scroller.pixels_per_line = 1.0;
        scroller.on_line_delta(Vec2d { x: 0.0, y: -8.0 });

        let mut total: f64 = 0.0;
        let mut frames = 0;
        while let Some(delta) = scroller.tick_with_dt(1.0 / 60.0) {
            assert!(delta.y <= 0.0);
            total += delta.y;
            frames += 1;
            assert!(frames < 30);
        }

        assert!(frames > 1);
        assert!((total + 8.0).abs() < 0.0001);
    }

    #[test]
    fn repeated_deltas_extend_the_remaining_distance() {
        let mut scroller = MomentumScroller::new();
        scroller.pixels_per_line = 1.0;
        scroller.on_line_delta(Vec2d { x: 0.0, y: -8.0 });
        let first = scroller.tick_with_dt(1.0 / 60.0).unwrap();
        scroller.on_line_delta(Vec2d { x: 0.0, y: -8.0 });

        let mut total: f64 = first.y;
        while let Some(delta) = scroller.tick_with_dt(1.0 / 60.0) {
            total += delta.y;
        }

        assert!((total + 16.0).abs() < 0.0001);
    }
}
