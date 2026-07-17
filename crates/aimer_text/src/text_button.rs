use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_events::window::request_animation_frame;
use aimer_style::TextStyle;
use aimer_utils::AnimInstant;
use aimer_utils::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    Drawable, Element, EventElement, LayoutCache, LayoutElement, Rebuildable, VisitorElement,
    Widget,
};

use crate::RawTextWidget;

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

    pub fn hover_color(mut self, color: impl Into<Color>) -> Self {
        self.hover_color = Some(color.into());
        self
    }

    pub fn disabled_color(mut self, color: impl Into<Color>) -> Self {
        self.disabled_color = Some(color.into());
        self
    }

    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn hover_style(mut self, style: TextStyle) -> Self {
        self.hover_style = style;
        self
    }

    pub fn disabled_style(mut self, style: TextStyle) -> Self {
        self.disabled_style = style;
        self
    }

    pub fn on_press(mut self, callback: impl Into<VoidCallback>) -> Self {
        self.on_press = callback.into();
        self
    }

    pub fn on_double_press(mut self, callback: impl Into<VoidCallback>) -> Self {
        self.on_double_press = callback.into();
        self
    }
}

impl Widget for TextButton {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        RawTextButton {
            widget: self.clone(),
            hovered: Cell::new(false),
            interaction: RefCell::new(ButtonInteraction::default()),
            last_tap: Cell::new(None),
            bounds: CacheBounds::new(),
        }
        .boxed()
    }
}

#[derive(Debug, Default)]
struct ButtonInteraction {
    armed: bool,
}

#[derive(Debug, Eq, PartialEq)]
enum ButtonAction {
    None,
    Press,
}

impl ButtonInteraction {
    fn pointer_down(&mut self, inside: bool, disabled: bool) -> ButtonAction {
        self.armed = inside && !disabled;
        ButtonAction::None
    }

    fn pointer_up(&mut self, inside: bool, disabled: bool) -> ButtonAction {
        let pressed = self.armed && inside && !disabled;
        self.armed = false;
        if pressed { ButtonAction::Press } else { ButtonAction::None }
    }

    fn cancel(&mut self) {
        self.armed = false;
    }
}

struct RawTextButton {
    widget: TextButton,
    hovered: Cell<bool>,
    interaction: RefCell<ButtonInteraction>,
    last_tap: Cell<Option<AnimInstant>>,
    bounds: CacheBounds,
}

impl RawTextButton {
    const DOUBLE_TAP_INTERVAL: Duration = Duration::from_millis(500);

    fn active_style(&self) -> TextStyle {
        let (mut style, color) = if self.widget.disabled {
            (self.widget.disabled_style, self.widget.disabled_color)
        } else if self.hovered.get() {
            (self.widget.hover_style, self.widget.hover_color)
        } else {
            (self.widget.style, self.widget.color)
        };
        if let Some(color) = color {
            style.color = color;
        }
        style
    }

    fn text_element(&self) -> RawTextWidget {
        RawTextWidget {
            text: self.widget.label.clone(),
            text_style: self.active_style(),
            text_align: Default::default(),
            cache: LayoutCache::new(),
            _typeface: Mutex::new(None),
        }
    }

    fn execute(callback: &VoidCallback) {
        if let Some(callback) = callback.get().as_ref() {
            match callback {
                RawInnerCallback::Empty => {}
                RawInnerCallback::Sync(function) => function(()),
                RawInnerCallback::Async(function) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.spawn(function(()));
                    }
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(function(()));
                }
            }
        }
    }

    fn press(&self) {
        Self::execute(&self.widget.on_press);
        let now = AnimInstant::now();
        if self
            .last_tap
            .get()
            .is_some_and(|last| now.duration_since(last) <= Self::DOUBLE_TAP_INTERVAL)
        {
            Self::execute(&self.widget.on_double_press);
            self.last_tap.set(None);
        } else {
            self.last_tap.set(Some(now));
        }
    }

    fn set_hovered(&self, hovered: bool) {
        if self.hovered.replace(hovered) != hovered {
            request_animation_frame();
        }
    }
}

impl VisitorElement for RawTextButton {
    fn debug_name(&self) -> &'static str {
        "TextButton"
    }
}

impl EventElement for RawTextButton {
    fn on_event(&self, event: &ElementEvent) -> bool {
        match event {
            ElementEvent::PointerMove(pos, PointerSource::Mouse, _) => {
                self.set_hovered(self.bounds.is_inside(pos.x, pos.y) && !self.widget.disabled);
                false
            }
            ElementEvent::PointerExited(PointerSource::Mouse, _) => {
                self.set_hovered(false);
                self.interaction
                    .borrow_mut()
                    .cancel();
                false
            }
            ElementEvent::PointerDown(pos, _, _) => {
                let inside = self.bounds.is_inside(pos.x, pos.y);
                self.interaction
                    .borrow_mut()
                    .pointer_down(inside, self.widget.disabled);
                inside && !self.widget.disabled
            }
            ElementEvent::PointerUp(pos, _, _) => {
                let action = self
                    .interaction
                    .borrow_mut()
                    .pointer_up(self.bounds.is_inside(pos.x, pos.y), self.widget.disabled);
                if action == ButtonAction::Press {
                    self.press();
                    true
                } else {
                    false
                }
            }
            ElementEvent::Cancel => {
                self.interaction
                    .borrow_mut()
                    .cancel();
                false
            }
            _ => false,
        }
    }
}

impl LayoutElement for RawTextButton {
    fn layout(&self, ctx: &BuildContext) -> aimer_attribute::ResolvedSize {
        let size = self.text_element().layout(ctx);
        let (x, y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(ctx.scale, x, y, size.width, size.height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> aimer_attribute::ResolvedSize {
        self.text_element()
            .computed_size(ctx)
    }
}

impl Drawable for RawTextButton {
    fn draw(&self, ctx: &BuildContext) {
        let text = self.text_element();
        let size = text.computed_size(ctx);
        let (x, y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(ctx.scale, x, y, size.width, size.height);
        if !self.widget.disabled {
            self.set_hovered(
                self.bounds
                    .is_inside(ctx.cursor_pos.x, ctx.cursor_pos.y),
            );
        }
        text.draw(ctx);
    }
}

impl Rebuildable for RawTextButton {}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn press_requires_down_and_up_inside_the_text_bounds() {
        let mut state = ButtonInteraction::default();

        assert_eq!(state.pointer_down(true, false), ButtonAction::None);
        assert_eq!(state.pointer_up(true, false), ButtonAction::Press);

        state.pointer_down(true, false);
        assert_eq!(state.pointer_up(false, false), ButtonAction::None);
    }

    #[test]
    fn disabled_button_never_arms_or_presses() {
        let mut state = ButtonInteraction::default();

        state.pointer_down(true, true);

        assert_eq!(state.pointer_up(true, true), ButtonAction::None);
    }

    #[test]
    fn synchronous_press_callback_is_executed() {
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let callback = VoidCallback::from(move || observed.set(observed.get() + 1));

        RawTextButton::execute(&callback);

        assert_eq!(calls.get(), 1);
    }
}
