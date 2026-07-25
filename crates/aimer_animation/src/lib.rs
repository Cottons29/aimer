pub mod control;
mod local_cell;
pub mod primitives;
pub mod widgets;

// Core primitives
// Animation orchestration
pub use control::{
    AnimationController, AnimationStatus, Keyframe, KeyframeAnimation, ParallelAnimation,
    SequentialAnimation, StaggeredAnimation, StatusListener,
};
pub use primitives::{AnimInstant, Animatable, AnimatableExt, Curve, Tween};
// Widget layer
pub use widgets::AnimatedBuilder;
pub use widgets::{
    Animated, AnimatedSwitcher, AnimationEffect, FadeTransition, ImplicitAnimatedBuilder,
    MorphTransition, Rgba, RotationTransition, ScaleTransition, SlideTransition,
};
