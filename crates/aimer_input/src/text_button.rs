use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::gesture::{
    DragCallback, DragUpdateCallback, ScaleCallback, ScrollCallback, SwipeCallback,
};
use crate::mouse_region::{MouseRegion, SharedPointerState};
use aimer_attribute::CacheBounds;
use aimer_style::TextStyle;
use aimer_text::Text;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{Element, State, StateUpdater, StatefulElement, StatefulWidget, Widget};
use std::rc::Rc;

#[derive(Clone)]
pub struct TextButton {
    disabled: bool,
    label: Rc<str>,
    color: Option<Color>,
    hover_color: Option<Color>,
    disabled_color: Option<Color>,
    style: TextStyle,
    hover_style: TextStyle,
    disabled_style: TextStyle,
    on_press: VoidCallback,
    on_double_press: VoidCallback,
}

impl TextButton {
    pub const TEXT_COLOR: Color = Color::BLUE;
    pub const HOVER_COLOR: Color = Color::BLUE.lighten(0.6);
    pub const DISABLED_COLOR: Color = Color::GRAY;

    pub fn new(label: impl Into<Rc<str>>) -> Self {
        Self {
            disabled: false,
            label: label.into(),
            color: None,
            hover_color: None,
            disabled_color: None,
            style: TextStyle::default(),
            hover_style: TextStyle::default(),
            disabled_style: TextStyle::default(),
            on_press: VoidCallback::default(),
            on_double_press: VoidCallback::default(),
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn hover_color(mut self, hover_color: impl Into<Color>) -> Self {
        self.hover_color = Some(hover_color.into());
        self
    }

    pub fn disabled_color(mut self, disabled_color: impl Into<Color>) -> Self {
        self.disabled_color = Some(disabled_color.into());
        self
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn hover_style(mut self, hover_style: TextStyle) -> Self {
        self.hover_style = hover_style;
        self
    }

    pub fn disabled_style(mut self, disabled_style: TextStyle) -> Self {
        self.disabled_style = disabled_style;
        self
    }

    pub fn on_press(mut self, on_press: impl Into<VoidCallback>) -> Self {
        self.on_press = on_press.into();
        self
    }

    pub fn on_double_press(mut self, on_double_press: impl Into<VoidCallback>) -> Self {
        self.on_double_press = on_double_press.into();
        self
    }
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

impl StatefulWidget for TextButton {
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

    /// Refresh the button's configuration (label, colours, `style` /
    /// `hover_style`, `on_press`, `disabled`) from the freshly-built widget
    /// while preserving the live runtime state (`hovered`, `region_state`,
    /// `updater`). Called by the framework during reconciliation, e.g. after a
    /// window resize or a parent `set_state` re-emits this button with new
    /// props (such as a tab that just became selected). Without it the button
    /// would keep its stale look — the selected/hover styling would stay stuck
    /// on whatever it was when the state was first created.
    fn adopt_config_from(&mut self, new: &Self)
    where
        Self: Sized,
    {
        self.widget = new.widget.clone();
        self.disabled = new.disabled;
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        let mut text_style = if self.disabled {
            self.widget.disabled_style
        } else {
            if self.hovered { self.widget.hover_style } else { self.widget.style }
        };

        let color = if self.disabled {
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
                child: Text::new(self.widget.label.clone()).text_style(text_style),
            },
        }
    }
}
