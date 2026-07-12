use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback};
use crate::mouse_region::{MouseRegion, PointerState};
use aimer_attribute::CacheBounds;
use aimer_container::{Container, ZeroSizedBox};
use aimer_style::BoxDecoration;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, State, StateUpdater, StatefulElement, StatefulWidget, Widget};
use std::cell::Cell;
use std::rc::Rc;

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and
/// provides gesture callbacks for tap, double-tap, long-press, right-click,
/// swipe, scroll, and scale. It dims when disabled.
#[allow(dead_code)]
pub struct Button<W: Widget + 'static = ZeroSizedBox> {
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
    pub on_long_press: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_right_press: VoidCallback,
    pub decoration: BoxDecoration,
    current_state: Rc<Cell<PointerState>>,
    state_updater: StateUpdater<Self>,
    child: Rc<W>,
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
            child: Rc::new(ZeroSizedBox),
        }
    }
}

impl<W: Widget + 'static> Button<W> {
    pub fn on_press(mut self, on_press: impl Into<VoidCallback>) -> Self {
        self.on_press = on_press.into();
        self
    }

    pub fn on_long_press(mut self, on_long_press: impl Into<VoidCallback>) -> Self {
        self.on_long_press = on_long_press.into();
        self
    }

    pub fn on_double_press(mut self, on_double_press: impl Into<VoidCallback>) -> Self {
        self.on_double_press = on_double_press.into();
        self
    }

    pub fn on_right_press(mut self, on_right_press: impl Into<VoidCallback>) -> Self {
        self.on_right_press = on_right_press.into();
        self
    }

    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    pub fn is_disabled(mut self, is_disabled: bool) -> Self {
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
            on_press: self.on_press.clone(),
            on_long_press: self.on_long_press.clone(),
            on_double_press: self.on_double_press.clone(),
            on_right_press: self.on_right_press.clone(),
            decoration: self.decoration.clone(),
            state_updater: StateUpdater::empty(),
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            child: self.child.clone(),
        }
    }
}

impl<W: Widget + 'static> Widget for Button<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let (element, _updater) = StatefulElement::new(self, ctx);

        element.boxed()
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
        self.on_press = new.on_press.clone();
        self.on_long_press = new.on_long_press.clone();
        self.on_double_press = new.on_double_press.clone();
        self.on_right_press = new.on_right_press.clone();
        self.decoration = new.decoration.clone();
        self.child = new.child.clone();
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        let child = self.child.clone();

        let mut decor = self.decoration.clone();

        if self.is_hover && let Some(color) = decor.background_color {
            decor.background_color = Some(color.lighten(0.2));
        }

        MouseRegion {
            on_hover_enter: {
                let updater = self.state_updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hover = true;
                    })
                }
            }
            .into(),
            on_hover_exit: {
                let updater = self.state_updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hover = false;
                    })
                }
            }
            .into(),
            cursor: None,
            // current_state: state.accept_state.clone(),
            current_state: self.current_state.clone(),
            cached_bounds: CacheBounds::new(),
            child: GestureDetector {
                on_tap: self.on_press.clone(),
                on_double_press: self.on_double_press.clone(),
                on_long_press: self.on_long_press.clone(),
                on_drag_start: DragCallback::default(),
                on_drag_update: DragUpdateCallback::default(),
                on_drag_end: VoidCallback::default(),
                on_right_tap: self.on_right_press.clone(),
                on_swipe: SwipeCallback::default(),
                on_scroll: ScrollCallback::default(),
                on_scale: ScaleCallback::default(),
                child: Container::new()
                    .box_decoration(decor)
                    .child(child as Rc<dyn Widget>),
            },
        }
    }
}
