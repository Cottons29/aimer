use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use aimer_animation::{AnimInstant, AnimationController, Curve};
use aimer_provider::{Provider, ProviderHandle};
use aimer_widget::base::{BuildContext, ResolvedSize, Size, Vec2d};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable,
    RequiredChild, State, StateUpdater, StatefulElement, StatefulWidget, VisitorElement, Widget,
};

use crate::{Theme, ThemeData};

fn request_next_frame() {
    aimer_events::window::request_animation_frame();
}

/// Supplies a [`Theme`] value to descendants and animates changes to it.
///
/// Descendants read the interpolated value with [`Theme::of`]. When `data` changes,
/// `AnimatedTheme` interpolates the theme without replacing the descendant widget tree. If the
/// target changes during a transition, the new transition begins at the currently displayed
/// theme. A zero duration applies the target immediately.
///
/// The default transition lasts 200 milliseconds and uses [`Curve::Linear`].
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use aimer_animation::Curve;
/// use aimer_style::{AnimatedTheme, ThemeData};
/// use aimer_widget::Widget;
///
/// fn themed_app(child: impl Widget + 'static) -> impl Widget {
///     AnimatedTheme::new().data(ThemeData::dark())
///                         .duration(Duration::from_millis(300))
///                         .curve(Curve::EaseInOut)
///                         .child(child)
/// }
/// ```
pub struct AnimatedTheme<W = RequiredChild, T = ThemeData> {
    data: T,
    duration: Duration,
    curve: Curve,
    child: Rc<W>,
}

impl AnimatedTheme {
    /// Creates an animated theme with a light theme and the default transition
    /// settings.
    ///
    /// Attach the descendant subtree last with [`AnimatedTheme::child`].
    pub fn new() -> Self {
        Self {
            data: ThemeData::default(),
            duration: Duration::from_millis(200),
            curve: Curve::Linear,
            child: Rc::new(RequiredChild),
        }
    }
}

impl Default for AnimatedTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl<W, T> AnimatedTheme<W, T> {
    /// Sets the target theme supplied to descendants.
    pub fn data<U: Theme>(self, data: U) -> AnimatedTheme<W, U> {
        AnimatedTheme {
            data,
            duration: self.duration,
            curve: self.curve,
            child: self.child,
        }
    }

    /// Sets how long a theme transition lasts.
    ///
    /// A zero duration disables interpolation and publishes the target theme
    /// immediately.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the curve used to transform the transition's linear progress.
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// Attaches the descendant widget subtree and produces a valid widget.
    pub fn child<C: Widget>(self, child: C) -> AnimatedTheme<C, T> {
        AnimatedTheme {
            data: self.data,
            duration: self.duration,
            curve: self.curve,
            child: Rc::new(child),
        }
    }

    /// Attaches the descendant subtree and type-erases the completed theme
    /// widget.
    ///
    /// This is equivalent to calling [`AnimatedTheme::child`] followed by
    /// [`Widget::boxed`]. Use it when different code paths must return one
    /// [`AnyWidget`] type.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget
    where
        T: Theme,
    {
        self.child(child).boxed()
    }
}

#[derive(Clone, Debug)]
struct ThemeTransition<T> {
    begin: T,
    end: T,
}

impl<T: Theme> ThemeTransition<T> {
    fn new(value: T) -> Self {
        Self {
            begin: value.clone(),
            end: value,
        }
    }

    fn sample(&self, progress: f32) -> T {
        self.begin
            .lerp(&self.end, progress)
    }

    fn retarget(&mut self, target: T, progress: f32) -> bool {
        if self.end == target {
            return false;
        }
        self.begin = self.sample(progress);
        self.end = target;
        true
    }
}

#[doc(hidden)]
pub struct AnimatedThemeState<T: Theme> {
    target: T,
    current: Rc<RefCell<T>>,
    duration: Duration,
    curve: Curve,
    child: Rc<dyn Widget>,
    controller: AnimationController,
    transition: Rc<RefCell<ThemeTransition<T>>>,
    handle: ProviderHandle<T>,
}

impl<W: Widget + 'static, T: Theme> StatefulWidget for AnimatedTheme<W, T> {
    type State = AnimatedThemeState<T>;

    fn create_state(&self) -> Self::State {
        AnimatedThemeState {
            target: self.data.clone(),
            current: Rc::new(RefCell::new(self.data.clone())),
            duration: self.duration,
            curve: self.curve,
            child: self.child.clone(),
            controller: AnimationController::new(self.duration, self.curve),
            transition: Rc::new(RefCell::new(ThemeTransition::new(self.data.clone()))),
            handle: ProviderHandle::new(self.data.clone()),
        }
    }
}

impl<W: Widget + 'static, T: Theme> State<AnimatedTheme<W, T>> for AnimatedThemeState<T> {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: &Self) {
        self.duration = new.duration;
        self.curve = new.curve;
        self.child = new.child.clone();
        self.controller
            .set_duration(new.duration);
        self.controller
            .set_curve(new.curve);

        if !self
            .transition
            .borrow_mut()
            .retarget(new.target.clone(), self.controller.value())
        {
            return;
        }

        self.target = new.target.clone();
        self.controller.reset();
        if self.duration.is_zero() {
            self.controller.set_value(1.0);
            self.publish(self.target.clone());
        } else {
            *self.current.borrow_mut() = self
                .transition
                .borrow()
                .sample(0.0);
            self.controller
                .forward_from_first_tick();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedThemeFrame {
            current: self.current.clone(),
            child: self.child.clone(),
            controller: self.controller.clone(),
            transition: self.transition.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<T: Theme> AnimatedThemeState<T> {
    fn publish(&self, value: T) {
        if *self.current.borrow() != value {
            *self.current.borrow_mut() = value.clone();
            self.handle
                .update(|theme| *theme = value);
        }
    }
}

impl<W: Widget + 'static, T: Theme> Widget for AnimatedTheme<W, T> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "AnimatedTheme", self.key())
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedTheme"
    }
}

struct AnimatedThemeFrame<T: Theme> {
    current: Rc<RefCell<T>>,
    child: Rc<dyn Widget>,
    controller: AnimationController,
    transition: Rc<RefCell<ThemeTransition<T>>>,
    handle: ProviderHandle<T>,
}

impl<T: Theme> Widget for AnimatedThemeFrame<T> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = Provider::new()
            .handle(self.handle.clone())
            .child(self.child.clone())
            .to_element(ctx);
        AnimatedThemeElement {
            current: self.current.clone(),
            child,
            controller: self.controller.clone(),
            transition: self.transition.clone(),
            handle: self.handle.clone(),
        }
        .boxed()
    }
}

struct AnimatedThemeElement<T: Theme> {
    current: Rc<RefCell<T>>,
    child: AnyElement,
    controller: AnimationController,
    transition: Rc<RefCell<ThemeTransition<T>>>,
    handle: ProviderHandle<T>,
}

impl<T: Theme> VisitorElement for AnimatedThemeElement<T> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedTheme"
    }
}

impl<T: Theme> Drawable for AnimatedThemeElement<T> {
    fn draw(&self, ctx: &BuildContext) {
        let progress = self
            .controller
            .tick(AnimInstant::now());
        let value = self
            .transition
            .borrow()
            .sample(progress);
        if *self.current.borrow() != value {
            *self.current.borrow_mut() = value.clone();
            self.handle
                .update(|theme| *theme = value);
        }

        self.child
            .rebuild_if_dirty(ctx);
        self.child.draw(ctx);

        if self.controller.is_animating() {
            request_next_frame();
        }
    }
}

impl<T: Theme> EventElement for AnimatedThemeElement<T> {}

impl<T: Theme> Rebuildable for AnimatedThemeElement<T> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child
            .rebuild_if_dirty(ctx);
    }

    fn is_carry_state(&self) -> bool {
        true
    }

    fn mark_needs_rebuild(&self) {
        self.child
            .mark_needs_rebuild();
    }
}

impl<T: Theme> LayoutElement for AnimatedThemeElement<T> {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child.flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child
            .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child.pos_start_end()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use aimer_animation::Animatable;
    use aimer_color::prelude::Color;

    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct CustomTheme {
        value: f32,
    }

    impl Animatable for CustomTheme {
        fn lerp(&self, other: &Self, t: f32) -> Self {
            if t <= 0.0 {
                return self.clone();
            }
            if t >= 1.0 {
                return other.clone();
            }
            Self {
                value: self
                    .value
                    .lerp(&other.value, t),
            }
        }
    }

    impl crate::Theme for CustomTheme {}

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            panic!("not needed for state lifecycle tests")
        }
    }

    fn theme(value: u8) -> ThemeData {
        ThemeData::new().primary_color(Color::Rgba(value, value, value, 255))
    }

    fn widget(data: ThemeData, duration: Duration) -> AnimatedTheme<TestWidget> {
        AnimatedTheme::new()
            .data(data)
            .duration(duration)
            .child(TestWidget)
    }

    fn custom_widget(
        data: CustomTheme,
        duration: Duration,
    ) -> AnimatedTheme<TestWidget, CustomTheme> {
        AnimatedTheme::new()
            .data(data)
            .duration(duration)
            .child(TestWidget)
    }

    #[test]
    fn unchanged_target_does_not_restart() {
        let data = theme(10);
        let mut transition = ThemeTransition::new(data);

        assert!(!transition.retarget(data, 0.5));
        assert_eq!(transition.sample(0.0), data);
    }

    #[test]
    fn interrupted_transition_starts_from_displayed_value() {
        let mut transition = ThemeTransition::new(theme(0));
        assert!(transition.retarget(theme(100), 0.0));

        assert!(transition.retarget(theme(200), 0.5));

        assert_eq!(
            transition
                .sample(0.0)
                .primary_color,
            Color::Rgba(50, 50, 50, 255)
        );
        assert_eq!(
            transition
                .sample(1.0)
                .primary_color,
            Color::Rgba(200, 200, 200, 255)
        );
    }

    #[test]
    fn custom_non_copy_theme_retargets_from_displayed_value() {
        let mut transition = ThemeTransition::new(CustomTheme { value: 0.0 });
        assert!(transition.retarget(CustomTheme { value: 100.0 }, 0.0));

        assert!(transition.retarget(CustomTheme { value: 200.0 }, 0.5));

        assert_eq!(transition.sample(0.0), CustomTheme { value: 50.0 });
        assert_eq!(transition.sample(1.0), CustomTheme { value: 200.0 });
    }

    #[test]
    fn zero_duration_uses_exact_target() {
        let mut state = widget(theme(0), Duration::from_millis(200)).create_state();
        let new_state = widget(theme(101), Duration::ZERO).create_state();

        <AnimatedThemeState<ThemeData> as State<AnimatedTheme<TestWidget>>>::adopt_config_from(
            &mut state, &new_state,
        );

        assert_eq!(*state.handle.read(), theme(101));
        assert_eq!(*state.current.borrow(), theme(101));
        assert!(
            !state
                .controller
                .is_animating()
        );
    }

    #[test]
    fn custom_non_copy_theme_zero_duration_publishes_exact_target() {
        let mut state =
            custom_widget(CustomTheme { value: 0.0 }, Duration::from_millis(200)).create_state();
        let new_state = custom_widget(CustomTheme { value: 101.0 }, Duration::ZERO).create_state();

        <AnimatedThemeState<CustomTheme> as State<AnimatedTheme<TestWidget, CustomTheme>>>::adopt_config_from(
            &mut state,
            &new_state,
        );

        assert_eq!(*state.handle.read(), CustomTheme { value: 101.0 });
        assert_eq!(*state.current.borrow(), CustomTheme { value: 101.0 });
        assert!(
            !state
                .controller
                .is_animating()
        );
    }

    #[test]
    fn changed_theme_starts_the_controller() {
        let mut state = widget(theme(0), Duration::from_millis(200)).create_state();
        let new_state = widget(theme(101), Duration::from_millis(400)).create_state();

        <AnimatedThemeState<ThemeData> as State<AnimatedTheme<TestWidget>>>::adopt_config_from(
            &mut state, &new_state,
        );

        assert_eq!(state.controller.duration(), Duration::from_millis(400));
        assert!(
            state
                .controller
                .is_animating()
        );
        assert_eq!(*state.current.borrow(), theme(0));
    }

    #[test]
    fn active_transition_requests_next_frame_through_animation_scheduler() {
        let requests = Arc::new(AtomicUsize::new(0));
        let observed_requests = requests.clone();
        aimer_events::window::set_redraw_requester(move || {
            observed_requests.fetch_add(1, Ordering::Relaxed);
        });

        request_next_frame();

        assert_eq!(requests.load(Ordering::Relaxed), 1);
    }
}
