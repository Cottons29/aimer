use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_container::Container;
use aimer_style::BoxDecoration;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    Element, EmptyWidget, State, StateUpdater, StatefulElement, StatefulWidget, Widget,
};

use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{
    DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback,
};
use crate::mouse_region::{MouseRegion, PointerState};

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and
/// provides gesture callbacks for tap, double-tap, long-press, right-click,
/// swipe, scroll, and scale. It dims when disabled.
#[allow(dead_code)]
pub struct Button<W = EmptyWidget> {
    pub on_press: VoidCallback,
    pub on_long_press: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_right_press: VoidCallback,
    pub decoration: BoxDecoration,
    pub is_disabled: bool,
    child: Rc<W>,
}

pub struct ButtonState<W: Widget + 'static> {
    is_hover: bool,
    pub on_press: VoidCallback,
    pub is_disabled: bool,
    pub on_long_press: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_right_press: VoidCallback,
    pub decoration: BoxDecoration,
    current_state: Rc<Cell<PointerState>>,
    state_updater: StateUpdater<Self>,
    child: Rc<W>,
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl Button {
    pub fn new() -> Self {
        Self {
            on_press: VoidCallback::default(),
            on_long_press: VoidCallback::default(),
            on_double_press: VoidCallback::default(),
            on_right_press: VoidCallback::default(),
            decoration: BoxDecoration::default(),
            is_disabled: false,
            child: Rc::new(EmptyWidget),
        }
    }
}

impl<W> Button<W> {
    pub fn on_press(mut self, on_press: impl Into<VoidCallback>) -> Self {
        self.on_press = on_press.into();
        self
    }

    /// Register an **async** press callback.
    ///
    /// The closure must return a `Future` (e.g. an `async` block).
    /// The future is spawned by the framework's executor.
    ///
    /// **Note**: Since async closures capture state, they implement `FnOnce`.
    /// The closure is taken on first invocation — subsequent presses produce
    /// no action. If you need repeated invocations, clone your captured data
    /// before the async block or use `Rc<RefCell<...>>`.
    pub fn on_press_async<F, Fut>(mut self, on_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_press = VoidCallback::from_async(on_press);
        self
    }

    pub fn on_long_press(mut self, on_long_press: impl Into<VoidCallback>) -> Self {
        self.on_long_press = on_long_press.into();
        self
    }

    /// Register an **async** long-press callback.
    pub fn on_long_press_async<F, Fut>(mut self, on_long_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_long_press = VoidCallback::from_async(on_long_press);
        self
    }

    pub fn on_double_press(mut self, on_double_press: impl Into<VoidCallback>) -> Self {
        self.on_double_press = on_double_press.into();
        self
    }

    /// Register an **async** double-press callback.
    pub fn on_double_press_async<F, Fut>(mut self, on_double_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_double_press = VoidCallback::from_async(on_double_press);
        self
    }

    pub fn on_right_press(mut self, on_right_press: impl Into<VoidCallback>) -> Self {
        self.on_right_press = on_right_press.into();
        self
    }

    /// Register an **async** right-press callback.
    pub fn on_right_press_async<F, Fut>(mut self, on_right_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_right_press = VoidCallback::from_async(on_right_press);
        self
    }

    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    pub fn disabled(mut self, is_disabled: bool) -> Self {
        self.is_disabled = is_disabled;
        self
    }

    pub fn child<C: Widget>(self, child: C) -> Button<C> {
        Button {
            on_press: self.on_press,
            on_long_press: self.on_long_press,
            on_double_press: self.on_double_press,
            on_right_press: self.on_right_press,
            decoration: self.decoration,
            is_disabled: self.is_disabled,
            child: Rc::new(child),
        }
    }
}

impl<W: Widget + 'static> StatefulWidget for Button<W> {
    type State = ButtonState<W>;

    fn create_state(&self) -> Self::State {
        ButtonState {
            is_hover: false,
            on_press: self
                .on_press
                .clone(),
            on_long_press: self
                .on_long_press
                .clone(),
            on_double_press: self
                .on_double_press
                .clone(),
            on_right_press: self
                .on_right_press
                .clone(),
            decoration: self
                .decoration
                .clone(),
            state_updater: StateUpdater::empty(),
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            child: self
                .child
                .clone(),
            is_disabled: self.is_disabled,
        }
    }
}

impl<W: Widget + 'static> Widget for Button<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new(self, ctx)
            .0
            .boxed()
    }
}

impl<W: Widget + 'static> State<Button<W>> for ButtonState<W> {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.state_updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.on_press = new
            .on_press
            .clone();
        self.is_disabled = new.is_disabled;
        self.on_long_press = new
            .on_long_press
            .clone();
        self.on_double_press = new
            .on_double_press
            .clone();
        self.on_right_press = new
            .on_right_press
            .clone();
        self.decoration = new
            .decoration
            .clone();
        self.child = new
            .child
            .clone();
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        let child = self
            .child
            .clone();

        let mut decor = self
            .decoration
            .clone();

        if self.is_hover
            && let Some(color) = decor.background_color
        {
            decor.background_color = Some(color.lighten(0.2));
        }

        if self.is_disabled {
            decor.background_color = Option::from(Color::BLACK.with_opacity(120));
        }
        let child = Container::new()
            .box_decoration(decor)
            .child(child as Rc<dyn Widget>);

        if self.is_disabled {
            return child.boxed();
        }

        MouseRegion {
            on_hover_enter: {
                let updater = self
                    .state_updater
                    .clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hover = true;
                    })
                }
            }
            .into(),
            on_hover_exit: {
                let updater = self
                    .state_updater
                    .clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hover = false;
                    })
                }
            }
            .into(),
            cursor: None,
            // current_state: state.accept_state.clone(),
            current_state: self
                .current_state
                .clone(),
            cached_bounds: CacheBounds::new(),
            child: GestureDetector {
                on_tap: if self.is_disabled {
                    VoidCallback::default()
                } else {
                    self.on_press
                        .clone()
                },
                on_double_press: if self.is_disabled {
                    VoidCallback::default()
                } else {
                    self.on_double_press
                        .clone()
                },
                on_long_press: if self.is_disabled {
                    VoidCallback::default()
                } else {
                    self.on_long_press
                        .clone()
                },
                on_drag_start: DragCallback::default(),
                on_drag_update: DragUpdateCallback::default(),
                on_drag_end: VoidCallback::default(),
                on_right_tap: self
                    .on_right_press
                    .clone(),
                on_swipe: SwipeCallback::default(),
                on_scroll: ScrollCallback::default(),
                on_scale: ScaleCallback::default(),
                child,
            },
        }
        .boxed()
    }
}
