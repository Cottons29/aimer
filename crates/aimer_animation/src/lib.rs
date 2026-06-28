pub mod animatable;
pub mod animated;
pub mod animated_builder;
pub mod animated_switcher;
pub mod controller;
pub mod curve;
pub mod group;
pub mod implicit_animation;
pub mod keyframe;
pub mod morph_transition;
pub mod time;
pub mod transition;
pub mod tween;

// Core primitives
pub use animatable::Animatable;
pub use controller::{AnimationController, AnimationStatus, StatusListener};
pub use curve::Curve;
pub use time::AnimInstant;
pub use tween::{AnimatableExt, Tween};

// Keyframe animation
pub use keyframe::{Keyframe, KeyframeAnimation};

// Animation orchestration
pub use group::{ParallelAnimation, SequentialAnimation, StaggeredAnimation};

// Widget layer
pub use animated::{Animated, AnimationEffect};
pub use animated_builder::AnimatedBuilder;
pub use animated_switcher::AnimatedSwitcher;
pub use implicit_animation::ImplicitAnimatedBuilder;
pub use morph_transition::{MorphTransition, Rgba};
pub use transition::{FadeTransition, RotationTransition, ScaleTransition, SlideTransition};
