use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use aimer_animation::{AnimInstant, AnimationController, Curve};
use aimer_provider::{Provider, ProviderHandle};
use aimer_widget::base::{BuildContext, ResolvedSize, Size, Vec2d};
use aimer_widget::{
    AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable, RequiredChild, State,
    StateUpdater, StatefulElement, StatefulWidget, VisitorElement, Widget,
};

use crate::ThemeData;

/// Supplies a [`ThemeData`] value to descendants and animates changes to it.
///
/// Descendants read the interpolated theme with [`crate::Theme::of`]. When
/// `data` changes, `AnimatedTheme` interpolates every semantic color without
/// replacing the descendant widget tree. If the target changes during a
/// transition, the new transition begins at the currently displayed
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
pub struct AnimatedTheme<W = RequiredChild> {
    data: ThemeData,
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

impl<W> AnimatedTheme<W> {
    /// Sets the target theme supplied to descendants.
    pub fn data(mut self, data: ThemeData) -> Self {
        self.data = data;
        self
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
    pub fn child<C: Widget>(self, child: C) -> AnimatedTheme<C> {
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
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child)
            .boxed()
    }
}

#[derive(Clone, Copy, Debug)]
struct ThemeTransition {
    begin: ThemeData,
    end: ThemeData,
}

impl ThemeTransition {
    fn new(value: ThemeData) -> Self {
        Self { begin: value, end: value }
    }

    fn sample(&self, progress: f32) -> ThemeData {
        self.begin
            .lerp(self.end, progress)
    }

    fn retarget(&mut self, target: ThemeData, progress: f32) -> bool {
        if self.end == target {
            return false;
        }
        self.begin = self.sample(progress);
        self.end = target;
        true
    }
}

#[doc(hidden)]
pub struct AnimatedThemeState {
    target: ThemeData,
    current: Rc<Cell<ThemeData>>,
    duration: Duration,
    curve: Curve,
    child: Rc<dyn Widget>,
    controller: AnimationController,
    transition: Rc<RefCell<ThemeTransition>>,
    handle: ProviderHandle<ThemeData>,
}

impl<W: Widget + 'static> StatefulWidget for AnimatedTheme<W> {
    type State = AnimatedThemeState;

    fn create_state(&self) -> Self::State {
        AnimatedThemeState {
            target: self.data,
            current: Rc::new(Cell::new(self.data)),
            duration: self.duration,
            curve: self.curve,
            child: self
                .child
                .clone(),
            controller: AnimationController::new(self.duration, self.curve),
            transition: Rc::new(RefCell::new(ThemeTransition::new(self.data))),
            handle: ProviderHandle::new(self.data),
        }
    }
}

impl<W: Widget + 'static> State<AnimatedTheme<W>> for AnimatedThemeState {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: &Self) {
        self.duration = new.duration;
        self.curve = new.curve;
        self.child = new
            .child
            .clone();
        self.controller
            .set_duration(new.duration);
        self.controller
            .set_curve(new.curve);

        if !self
            .transition
            .borrow_mut()
            .retarget(
                new.target,
                self.controller
                    .value(),
            )
        {
            return;
        }

        self.target = new.target;
        self.controller
            .reset();
        if self
            .duration
            .is_zero()
        {
            self.controller
                .set_value(1.0);
            self.publish(self.target);
        } else {
            self.current
                .set(
                    self.transition
                        .borrow()
                        .sample(0.0),
                );
            self.controller
                .forward_from_first_tick();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedThemeFrame {
            current: self
                .current
                .clone(),
            child: self
                .child
                .clone(),
            controller: self
                .controller
                .clone(),
            transition: self
                .transition
                .clone(),
            handle: self
                .handle
                .clone(),
        }
    }
}

impl AnimatedThemeState {
    fn publish(&self, value: ThemeData) {
        if self
            .current
            .replace(value)
            != value
        {
            self.handle
                .update(|theme| *theme = value);
        }
    }
}

impl<W: Widget + 'static> Widget for AnimatedTheme<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, "AnimatedTheme", self.key())
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedTheme"
    }
}

struct AnimatedThemeFrame {
    current: Rc<Cell<ThemeData>>,
    child: Rc<dyn Widget>,
    controller: AnimationController,
    transition: Rc<RefCell<ThemeTransition>>,
    handle: ProviderHandle<ThemeData>,
}

impl Widget for AnimatedThemeFrame {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = Provider::new()
            .handle(
                self.handle
                    .clone(),
            )
            .child(
                self.child
                    .clone(),
            )
            .to_element(ctx);
        Box::new(AnimatedThemeElement {
            current: self
                .current
                .clone(),
            child,
            controller: self
                .controller
                .clone(),
            transition: self
                .transition
                .clone(),
            handle: self
                .handle
                .clone(),
        })
    }
}

struct AnimatedThemeElement {
    current: Rc<Cell<ThemeData>>,
    child: Box<dyn Element>,
    controller: AnimationController,
    transition: Rc<RefCell<ThemeTransition>>,
    handle: ProviderHandle<ThemeData>,
}

impl VisitorElement for AnimatedThemeElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            self.child
                .as_ref(),
        );
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedTheme"
    }
}

impl Drawable for AnimatedThemeElement {
    fn draw(&self, ctx: &BuildContext) {
        let progress = self
            .controller
            .tick(AnimInstant::now());
        let value = self
            .transition
            .borrow()
            .sample(progress);
        if self
            .current
            .replace(value)
            != value
        {
            self.handle
                .update(|theme| *theme = value);
        }

        self.child
            .rebuild_if_dirty(ctx);
        self.child
            .draw(ctx);

        if self
            .controller
            .is_animating()
        {
            ctx.window
                .request_redraw();
        }
    }
}

impl EventElement for AnimatedThemeElement {}

impl Rebuildable for AnimatedThemeElement {
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

impl LayoutElement for AnimatedThemeElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child
            .pos()
    }

    fn size(&self) -> Option<Size> {
        self.child
            .size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child
            .layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child
            .computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child
            .content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.child
            .layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child
            .flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child
            .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child
            .invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child
            .pos_start_end()
    }
}

#[cfg(test)]
mod tests {
    use aimer_color::prelude::Color;

    use super::*;

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
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
    fn zero_duration_uses_exact_target() {
        let mut state = widget(theme(0), Duration::from_millis(200)).create_state();
        let new_state = widget(theme(101), Duration::ZERO).create_state();

        <AnimatedThemeState as State<AnimatedTheme<TestWidget>>>::adopt_config_from(
            &mut state, &new_state,
        );

        assert_eq!(
            *state
                .handle
                .read(),
            theme(101)
        );
        assert_eq!(
            state
                .current
                .get(),
            theme(101)
        );
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

        <AnimatedThemeState as State<AnimatedTheme<TestWidget>>>::adopt_config_from(
            &mut state, &new_state,
        );

        assert_eq!(
            state
                .controller
                .duration(),
            Duration::from_millis(400)
        );
        assert!(
            state
                .controller
                .is_animating()
        );
        assert_eq!(
            state
                .current
                .get(),
            theme(0)
        );
    }
}
