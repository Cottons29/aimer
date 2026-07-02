use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback};
use crate::mouse_region::{MouseRegion, SharedPointerState};
use aimer_attribute::CacheBounds;
use aimer_macro::WidgetConstructor;
use aimer_style::{TextDecoration, TextStyle};
use aimer_text::Text;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{Element, State, StateUpdater, StatefulElement, StatefulWidget, Widget};
use std::rc::Rc;

#[derive(WidgetConstructor)]
pub struct TextButton {
    label: Rc<str>,
    color: Color,
    hover_color: Color,
    hover_style: TextDecoration,
    on_press: VoidCallback,
    on_double_press: VoidCallback,
}

impl Widget for TextButton {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new(self, ctx).0.boxed()
    }
}

pub struct ButtonState {
    label: Rc<str>,
    is_hovered: bool,
    updater: StateUpdater<Self>,
    current_state: SharedPointerState,
    on_press: VoidCallback,
    on_double_press: VoidCallback,
}

impl StatefulWidget for TextButton {
    type State = ButtonState;

    fn create_state(&self) -> Self::State {
        ButtonState {
            label: self.label.clone(),
            is_hovered: false,
            current_state: SharedPointerState::default(),
            updater: StateUpdater::new(),
            on_press: self.on_press.clone(),
            on_double_press: self.on_double_press.clone(),
        }
    }
}

impl State<TextButton> for ButtonState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        // let text_color = if  self.is_hovered {self.} else {self.is_hovered};


        MouseRegion {
            on_hover_enter: {
                let updater = self.updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hovered = true;
                    })
                }
            }
            .into(),
            on_hover_exit: {
                let updater = self.updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hovered = false;
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
                on_long_press: VoidCallback::default(),
                on_drag_start: DragCallback::default(),
                on_drag_update: DragUpdateCallback::default(),
                on_drag_end: VoidCallback::default(),
                on_right_tap: VoidCallback::default(),
                on_swipe: SwipeCallback::default(),
                on_scroll: ScrollCallback::default(),
                on_scale: ScaleCallback::default(),
                child: Text!(
                    self.label.clone(),
                    text_style: TextStyle! {
                        font_size: 20,
                        color: Color::BLACK,
                    },
                ),
            },
        }
    }
}
