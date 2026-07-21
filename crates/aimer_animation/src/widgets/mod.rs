pub mod animated;
pub mod animated_builder;
pub mod animated_switcher;
pub mod implicit_animation;
pub mod morph_transition;
pub mod transition;

pub use animated::{Animated, AnimationEffect};
pub use animated_builder::AnimatedBuilder;
pub use animated_switcher::AnimatedSwitcher;
pub use implicit_animation::ImplicitAnimatedBuilder;
pub use morph_transition::{MorphTransition, Rgba};
pub use transition::{FadeTransition, RotationTransition, ScaleTransition, SlideTransition};

#[cfg(test)]
pub(crate) mod test_frame_requester {
    use std::cell::Cell;

    thread_local! {
        static REQUESTS: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn install() {
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| {
            aimer_events::window::set_redraw_requester(|| {
                REQUESTS.with(|requests| requests.set(requests.get() + 1));
            });
        });
    }

    pub(crate) fn reset() {
        REQUESTS.with(|requests| requests.set(0));
    }

    pub(crate) fn count() -> usize {
        REQUESTS.with(Cell::get)
    }
}
