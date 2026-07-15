pub mod control;
pub mod primitives;
pub mod widgets;

// Core primitives
pub use primitives::AnimInstant;
pub use primitives::Animatable;
pub use primitives::Curve;
pub use primitives::{AnimatableExt, Tween};

// Animation orchestration
pub use control::{AnimationController, AnimationStatus, StatusListener};
pub use control::{Keyframe, KeyframeAnimation};
pub use control::{ParallelAnimation, SequentialAnimation, StaggeredAnimation};

// Widget layer
pub use widgets::AnimatedBuilder;
pub use widgets::AnimatedSwitcher;
pub use widgets::ImplicitAnimatedBuilder;
pub use widgets::{Animated, AnimationEffect};
pub use widgets::{FadeTransition, RotationTransition, ScaleTransition, SlideTransition};
pub use widgets::{MorphTransition, Rgba};
