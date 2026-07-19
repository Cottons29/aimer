use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, State, StateUpdater,
    StatefulElement, StatefulWidget, VisitorElement, Widget,
};
use std::cell::UnsafeCell;
use std::rc::Rc;
use std::time::Duration;

use crate::control::controller::AnimationController;
use crate::local_cell::LocalCell;
use crate::primitives::animatable::Animatable;
use crate::primitives::curve::Curve;
use crate::primitives::time::AnimInstant;
use crate::primitives::tween::Tween;

type ImplicitElementBuilder<T> = dyn Fn(&T, &BuildContext) -> Box<dyn Element>;

/// A widget that automatically animates when its value changes.
///
/// On the first build, the value is used directly (no animation).
/// When the widget is rebuilt with a different value, a tween animation
/// runs from the currently displayed value to the new value over the specified
/// duration. Retargeting an animation therefore remains continuous. Rebuilding
/// with an equal value does not restart the controller.
///
/// # Example
/// ```rust
/// use std::time::Duration;
///
/// use aimer_animation::{Curve, ImplicitAnimatedBuilder};
/// use aimer_widget::ErrorWidget;
///
/// let animated = ImplicitAnimatedBuilder::new(
///     160.0_f32,
///     Duration::from_millis(300),
///     Curve::Linear,
///     |width| ErrorWidget::new(format!("Width: {width:.0}")),
/// );
/// ```
pub struct ImplicitAnimatedBuilder<T: Animatable + Clone + PartialEq + 'static> {
    pub value: T,
    pub duration: Duration,
    pub curve: Curve,
    builder: Rc<ImplicitElementBuilder<T>>,
}

impl<T> ImplicitAnimatedBuilder<T>
where
    T: Animatable + Clone + PartialEq + 'static,
{
    /// Creates an implicit animation for `value`.
    ///
    /// `T` must support interpolation through [`Animatable`]. The builder is
    /// called with the initial value immediately and with interpolated values
    /// during drawing. `duration` and `curve` are adopted on later rebuilds.
    pub fn new<F, W>(value: T, duration: Duration, curve: Curve, builder: F) -> Self
    where
        F: Fn(&T) -> W + 'static,
        W: Widget,
    {
        let builder = Rc::new(move |value: &T, ctx: &BuildContext| builder(value).to_element(ctx));
        Self { value, duration, curve, builder }
    }
}

impl<T> StatefulWidget for ImplicitAnimatedBuilder<T>
where
    T: Animatable + Clone + PartialEq + 'static,
{
    type State = ImplicitAnimatedState<T>;

    fn create_state(&self) -> Self::State {
        ImplicitAnimatedState {
            target: self
                .value
                .clone(),
            current: Rc::new(LocalCell::new(
                self.value
                    .clone(),
            )),
            duration: self.duration,
            curve: self.curve,
            builder: self
                .builder
                .clone(),
            controller: AnimationController::new(self.duration, self.curve),
            tween: Rc::new(LocalCell::new(None)),
            updater: StateUpdater::empty(),
        }
    }
}

impl<T> Widget for ImplicitAnimatedBuilder<T>
where
    T: Animatable + Clone + PartialEq + 'static,
{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, "ImplicitAnimatedBuilder", self.key())
            .0
            .boxed()
    }
}

#[doc(hidden)]
pub struct ImplicitAnimatedState<T: Animatable + Clone + PartialEq + 'static> {
    target: T,
    current: Rc<LocalCell<T>>,
    duration: Duration,
    curve: Curve,
    builder: Rc<ImplicitElementBuilder<T>>,
    controller: AnimationController,
    tween: Rc<LocalCell<Option<Tween<T>>>>,
    updater: StateUpdater<Self>,
}

impl<T> State<ImplicitAnimatedBuilder<T>> for ImplicitAnimatedState<T>
where
    T: Animatable + Clone + PartialEq + 'static,
{
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.duration = new.duration;
        self.curve = new.curve;
        self.builder = new
            .builder
            .clone();
        self.controller
            .set_duration(new.duration);
        self.controller
            .set_curve(new.curve);

        if self.target != new.target {
            let current = self
                .tween
                .with(|tween| {
                    tween
                        .as_ref()
                        .map(|tween| {
                            tween.lerp(
                                self.controller
                                    .value(),
                            )
                        })
                        .unwrap_or_else(|| {
                            self.current
                                .with(Clone::clone)
                        })
                });
            self.current
                .with_mut(|value| *value = current.clone());
            self.tween
                .with_mut(|tween| {
                    *tween = Some(Tween::new(
                        current,
                        new.target
                            .clone(),
                    ))
                });
            self.target = new
                .target
                .clone();
            self.controller
                .reset();
            self.controller
                .forward();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        ImplicitAnimatedFrame {
            current: self
                .current
                .clone(),
            target: self
                .target
                .clone(),
            builder: self
                .builder
                .clone(),
            controller: self
                .controller
                .clone(),
            tween: self
                .tween
                .clone(),
        }
    }
}

struct ImplicitAnimatedFrame<T: Animatable + Clone + 'static> {
    current: Rc<LocalCell<T>>,
    target: T,
    builder: Rc<ImplicitElementBuilder<T>>,
    controller: AnimationController,
    tween: Rc<LocalCell<Option<Tween<T>>>>,
}

impl<T: Animatable + Clone + 'static> Widget for ImplicitAnimatedFrame<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let value = self
            .current
            .with(Clone::clone);
        let child = (self.builder)(&value, ctx);
        Box::new(ImplicitAnimatedElement {
            child: UnsafeCell::new(child),
            current: self
                .current
                .clone(),
            target: self
                .target
                .clone(),
            builder: self
                .builder
                .clone(),
            controller: self
                .controller
                .clone(),
            tween: self
                .tween
                .clone(),
            window: ctx
                .window
                .clone(),
        })
    }
}

struct ImplicitAnimatedElement<T: Animatable + Clone + 'static> {
    child: UnsafeCell<Box<dyn Element>>,
    current: Rc<LocalCell<T>>,
    target: T,
    builder: Rc<ImplicitElementBuilder<T>>,
    controller: AnimationController,
    tween: Rc<LocalCell<Option<Tween<T>>>>,
    window: WindowHandle,
}

unsafe impl<T: Animatable + Clone + 'static> Send for ImplicitAnimatedElement<T> {}
unsafe impl<T: Animatable + Clone + 'static> Sync for ImplicitAnimatedElement<T> {}

impl<T: Animatable + Clone + 'static> Drawable for ImplicitAnimatedElement<T> {
    fn draw(&self, ctx: &BuildContext) {
        let progress = self
            .controller
            .tick(AnimInstant::now());
        let value = self
            .tween
            .with(|tween| {
                tween
                    .as_ref()
                    .map(|tween| tween.lerp(progress))
                    .unwrap_or_else(|| {
                        self.current
                            .with(Clone::clone)
                    })
            });
        self.current
            .with_mut(|current| *current = value.clone());
        unsafe {
            *self
                .child
                .get() = (self.builder)(&value, ctx)
        };
        unsafe {
            &*self
                .child
                .get()
        }
        .draw(ctx);

        if self
            .controller
            .is_animating()
        {
            self.window
                .request_redraw();
        } else {
            self.current
                .with_mut(|current| {
                    *current = self
                        .target
                        .clone()
                });
        }
    }
}

impl<T: Animatable + Clone + 'static> VisitorElement for ImplicitAnimatedElement<T> {
    fn debug_name(&self) -> &'static str {
        "ImplicitAnimatedElement"
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            unsafe {
                &*self
                    .child
                    .get()
            }
            .as_ref(),
        );
    }
}

impl<T: Animatable + Clone + 'static> EventElement for ImplicitAnimatedElement<T> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        unsafe {
            &*self
                .child
                .get()
        }
        .on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            unsafe {
                &*self
                    .child
                    .get()
            }
            .as_ref(),
        );
    }
}

impl<T: Animatable + Clone + 'static> Rebuildable for ImplicitAnimatedElement<T> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        unsafe {
            &*self
                .child
                .get()
        }
        .rebuild_if_dirty(ctx);
    }
}

impl<T: Animatable + Clone + 'static> LayoutElement for ImplicitAnimatedElement<T> {
    fn pos(&self) -> Option<Vec2d> {
        unsafe {
            &*self
                .child
                .get()
        }
        .pos()
    }

    fn size(&self) -> Option<Size> {
        unsafe {
            &*self
                .child
                .get()
        }
        .size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe {
            &*self
                .child
                .get()
        }
        .computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe {
            &*self
                .child
                .get()
        }
        .content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        unsafe {
            &*self
                .child
                .get()
        }
        .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        unsafe {
            &*self
                .child
                .get()
        }
        .invalidate_layout();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            panic!("not needed for state lifecycle tests")
        }
    }

    fn widget(value: f32) -> ImplicitAnimatedBuilder<f32> {
        ImplicitAnimatedBuilder::new(value, Duration::from_millis(100), Curve::Linear, |_| {
            TestWidget
        })
    }

    #[test]
    fn changed_target_starts_from_current_value() {
        let mut state = widget(2.0).create_state();
        let new_state = widget(10.0).create_state();

        state.adopt_config_from(&new_state);

        state
            .tween
            .with(|tween| {
                let tween = tween
                    .as_ref()
                    .unwrap();
                assert_eq!(tween.begin, 2.0);
                assert_eq!(tween.end, 10.0);
            });
        assert!(
            state
                .controller
                .is_animating()
        );
    }

    #[test]
    fn interrupted_animation_retargets_from_sampled_value() {
        let mut state = widget(0.0).create_state();
        state.adopt_config_from(&widget(10.0).create_state());
        state
            .controller
            .set_value(0.5);

        state.adopt_config_from(&widget(20.0).create_state());

        state
            .tween
            .with(|tween| {
                let tween = tween
                    .as_ref()
                    .unwrap();
                assert!((tween.begin - 5.0).abs() < f32::EPSILON);
                assert_eq!(tween.end, 20.0);
            });
    }

    #[test]
    fn unchanged_target_does_not_restart_animation() {
        let mut state = widget(3.0).create_state();

        state.adopt_config_from(&widget(3.0).create_state());

        assert!(
            state
                .tween
                .with(Option::is_none)
        );
        assert!(
            !state
                .controller
                .is_animating()
        );
    }
}
