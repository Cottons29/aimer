use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback};
use crate::mouse_region::{MouseRegion, SharedPointerState};
use aimer_attribute::CacheBounds;
use aimer_macro::WidgetConstructor;
use aimer_style::TextStyle;
use aimer_text::Text;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{Element, State, StateUpdater, StatefulElement, StatefulWidget, Widget};
use std::rc::Rc;

#[derive(WidgetConstructor, Clone)]
pub struct TextButton {
    #[constructor(default)]
    disabled: bool,
    #[constructor(first, into)]
    label: Rc<str>,
    #[constructor(default)]
    color: Option<Color>,
    #[constructor(default)]
    hover_color: Option<Color>,
    #[constructor(default)]
    disabled_color: Option<Color>,
    #[constructor(default)]
    style: TextStyle,
    #[constructor(default)]
    hover_style: TextStyle,
    #[constructor(default)]
    disabled_style: TextStyle,
    #[constructor(default, into)]
    on_press: VoidCallback,
    #[constructor(default, into)]
    on_double_press: VoidCallback,
}


impl TextButton {
    pub const TEXT_COLOR : Color = Color::BLUE;
    pub const HOVER_COLOR : Color = Color::BLUE.lighten(0.6);
    pub const DISABLED_COLOR : Color = Color::GRAY;
}

impl Widget for TextButton {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new(self, ctx).0.boxed()
    }
}

pub struct ButtonState {
    widget: TextButton,
    disabled: bool,
    hovered: bool,
    region_state: SharedPointerState,
    updater: StateUpdater<Self>,
}

impl<'a> StatefulWidget for TextButton {
    type State = ButtonState;

    fn create_state(&self) -> Self::State {
        ButtonState {
            widget: self.clone(),
            disabled: self.disabled,
            hovered: false,
            region_state: SharedPointerState::default(),
            updater: StateUpdater::new(),
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

    fn build(&self, _: &BuildContext) -> impl Widget {
        let mut text_style = if self.disabled {
            self.widget.disabled_style
        } else {
            if self.hovered { self.widget.hover_style } else { self.widget.style }
        };

        let color  = if self.disabled {
            self.widget.disabled_color
        } else {
            if self.hovered { self.widget.hover_color } else { self.widget.color }
        };

        if let Some(col) = color {
            text_style.color = col;
        }


        MouseRegion {
            on_hover_enter: {
                let updater = self.updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.hovered = true;
                    })
                }
            }
            .into(),
            on_hover_exit: {
                let updater = self.updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.hovered = false;
                    })
                }
            }
            .into(),
            cursor: None,
            // current_state: state.accept_state.clone(),
            current_state: self.region_state.clone(),
            cached_bounds: CacheBounds::new(),
            child: GestureDetector {
                on_tap: self.widget.on_press.clone(),
                on_double_press: self.widget.on_double_press.clone(),
                on_long_press: VoidCallback::default(),
                on_drag_start: DragCallback::default(),
                on_drag_update: DragUpdateCallback::default(),
                on_drag_end: VoidCallback::default(),
                on_right_tap: VoidCallback::default(),
                on_swipe: SwipeCallback::default(),
                on_scroll: ScrollCallback::default(),
                on_scale: ScaleCallback::default(),
                child: Text!(

                    self.widget.label.clone(),
                    text_style: text_style,
                ),
            },
        }
    }
}
