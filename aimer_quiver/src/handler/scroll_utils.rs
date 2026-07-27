use aimer_attribute::Vec2d;
use aimer_utils::AnimInstant as Instant;
use winit::dpi::PhysicalPosition;


pub struct MomentumScroller {
    velocity: Vec2d,
    pixels_per_line: f32,
    friction: f32,
    min_velocity: f32,
    max_velocity: f32,
    last_tick: Instant,
}

impl MomentumScroller {
    pub fn new() -> Self {
        Self {
            velocity: Vec2d::ZERO,
            pixels_per_line: 40.0,
            friction: 0.90,
            min_velocity: 0.5,
            max_velocity: 60.0,
            last_tick: Instant::now(),
        }
    }

    /// Call this whenever a real LineDelta event arrives.
    /// Adds to velocity rather than replacing it, so a burst of
    /// events accelerates the scroll instead of resetting it.
    pub fn on_line_delta(&mut self, delta: Vec2d) {
        self.velocity = self.velocity + delta.scale(self.pixels_per_line);
        let mag = self.velocity.magnitude();
        if mag > self.max_velocity {
            self.velocity = self.velocity.scale(self.max_velocity / mag);
        }
    }

    /// Call this every frame / timer tick to get the next synthetic
    /// PixelDelta to dispatch. Returns None once momentum has settled.
    pub fn tick(&mut self) -> Option<PhysicalPosition<f64>> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64();
        self.last_tick = now;

        if self.velocity.magnitude() < self.min_velocity {
            self.velocity = Vec2d::ZERO;
            return None;
        }

        let step = self.velocity;

        // frame-rate independent decay (friction is "per 1/60s"), applied uniformly
        let decay = (self.friction as f64).powf(dt * 60.0) as f32;
        self.velocity = self.velocity.scale(decay);

        Some(PhysicalPosition::new(step.x as f64, step.y as f64))
    }
}
