pub mod curve;
pub mod time;
pub mod controller;
pub mod animated;

pub use curve::Curve;
pub use time::AnimInstant;
pub use controller::{AnimationController, AnimationStatus};
pub use animated::{Animated, AnimationEffect};
