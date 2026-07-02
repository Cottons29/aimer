use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback};
use crate::mouse_region::{MouseRegion, PointerState};
use aimer_attribute::CacheBounds;
use aimer_container::Container;
use aimer_style::BoxDecoration;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, State, StateUpdater, StatefulElement, StatefulWidget, Widget, WidgetConstructor};
use std::cell::Cell;
use std::rc::Rc;

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and
/// provides gesture callbacks for tap, double-tap, long-press, right-click,
/// swipe, scroll, and scale. It dims when disabled.

#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Button<W: Widget + 'static> {
    #[constructor(default, into)]
    pub on_press: VoidCallback,
    #[constructor(default, into)]
    pub on_long_press: VoidCallback,
    #[constructor(default, into)]
    pub on_double_press: VoidCallback,
    #[constructor(default, into)]
    pub on_right_press: VoidCallback,
    #[constructor(default)]
    pub decoration: BoxDecoration,
    #[constructor(default)]
    pub is_disabled: bool,
    child: Rc<W>,
}

pub struct ButtonState<W: Widget + 'static> {
    is_disabled: bool,
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

impl<W: Widget + 'static> StatefulWidget for Button<W> {
    type State = ButtonState<W>;

    fn create_state(&self) -> Self::State {
        ButtonState {
            is_disabled: self.is_disabled,
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

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let child = self.child.clone();

        let mut decor = self.decoration.clone();

        if self.is_hover {
            if let Some(color) = decor.background_color {
                decor.background_color =  Some(color.lighten(0.2));
            }
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
                child: Container! {
                    box_decoration: decor,
                    child: child as Rc<dyn Widget>,
                },
            },
        }
    }
}
