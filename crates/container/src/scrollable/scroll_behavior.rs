use attribute::position::Vec2d;
use constructor::Constructor;
use crate::scrollable::Float;

#[derive(Constructor)]
pub struct ScrollBehavior {
    pub max_scroll: Vec2d,
    pub min_scroll: Vec2d,
    pub velocity: Vec2d,
    pub scroll_offset: Vec2d,
    #[constructor(default = true)]
    pub bouncy: bool,
    #[constructor(default = 0.6)]
    pub bouncy_resistance: Float,
    #[constructor(default = 0.15)]
    pub bouncy_recovery: Float,
    #[constructor(default = 0.95)]
    pub friction: Float,
}

impl Default for ScrollBehavior {
    fn default() -> Self {
        #[cfg(target_os = "ios")]
        let defaults = (0.6, 0.15, 0.98);
        #[cfg(not(target_os = "ios"))]
        let defaults = (0.6, 0.15, 0.95);

        Self {
            max_scroll: Vec2d { x: Float::MAX, y: Float::MAX },
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
