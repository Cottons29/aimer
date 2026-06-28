use aimer_attribute::position::Vec2d;
use aimer_macro::Constructor;

#[derive(Constructor)]
pub struct ScrollBehavior {
    pub max_scroll: Vec2d,
    pub min_scroll: Vec2d,
    pub velocity: Vec2d,
    pub scroll_offset: Vec2d,
    #[constructor(default = true)]
    pub bouncy: bool,
    #[constructor(default = 0.35)]
    pub bouncy_resistance: f32,
    #[constructor(default = 0.38)]
    pub bouncy_recovery: f32,
    /// Per-60 Hz-frame velocity retention during a fling.
    ///
    /// Applied as `friction.powf(frame_ratio)` each momentum frame where
    /// `frame_ratio = dt / FRAME_REF_120`, matching UIScrollView's discrete
    /// per-frame deceleration model exactly.
    ///
    /// `0.9998` per 60 fps frame = UIScrollView.DecelerationRate.normal.
    /// At 120 fps this is applied as `0.9998^(1/2)` per frame.
    /// Velocity at 1 s: 89 %, 2 s: 79 %, 5 s: 55 %, 10 s: 30 %.
    #[constructor(default = ScrollBehavior::DEFAULT_FRICTION)]
    pub friction: f32,
}

impl ScrollBehavior {
    const DEFAULT_FRICTION: f32 = 0.988;
}



impl Default for ScrollBehavior {
    fn default() -> Self {
        // friction 0.9998 per 60 fps = UIScrollView.DecelerationRate.normal.
        // Applied as friction.powf(frame_ratio) each frame (discrete, like
        // UIScrollView), NOT as a continuous exponential.
        // Velocity at 1 s: 89 %, 5 s: 55 %, 10 s: 30 %.
        let defaults = (0.75, 0.045, ScrollBehavior::DEFAULT_FRICTION);

        Self {
            max_scroll: Vec2d { x: f32::MAX, y: f32::MAX },
            min_scroll: Vec2d { x: 0.0, y: 0.0 },
            velocity: Vec2d { x: 0.0, y: 0.0 },
            scroll_offset: Vec2d { x: 0.0, y: 0.0 },
            bouncy: true,
            bouncy_resistance: defaults.0,
            bouncy_recovery: defaults.1,
            friction: defaults.2,
        }
    }
}


impl ScrollBehavior {
    pub fn no_bounce() -> Self {
        Self {
            bouncy: false,
            ..Default::default()
        }
    }
}

#[derive(Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
}
