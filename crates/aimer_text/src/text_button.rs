use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_events::window::request_animation_frame;
use aimer_style::{TextOverflow, TextStyle};
use aimer_utils::AnimInstant;
use aimer_utils::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutCache, LayoutElement, Rebuildable,
    VisitorElement, Widget,
};

use crate::RawTextWidget;

/// A label-sized text control that responds to primary presses.
///
/// The control lays out exactly like its text and has no container or padding. Its normal, hover,
/// and disabled styles each default to [`TextStyle::default`]; explicit color builders override the
/// color of the corresponding style. Wrapped labels shrink to their intrinsic width when space is
/// available and use the available width only when wrapping is necessary. For wrapped labels, each
/// line is hit-tested only across its rendered width, so blank space after a short line is not
/// interactive. A press fires on pointer-up only when pointer-down and pointer-up both occur inside
/// the label. Disabled controls neither hover nor invoke callbacks.
///
/// # Example
///
/// ```
/// use aimer_text::TextButton;
/// use aimer_widget::base::Color;
///
/// let button = TextButton::new("Learn more")
///     .color(Color::BLUE)
///     .on_press(|| println!("open"));
/// ```
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
    /// Conventional text color available to callers.
    pub const TEXT_COLOR: Color = Color::BLUE;
    /// Conventional hover color available to callers.
    pub const HOVER_COLOR: Color = Color::BLUE.lighten(0.6);
    /// Conventional disabled color available to callers.
    pub const DISABLED_COLOR: Color = Color::GRAY;

    /// Creates an enabled text button with `label`, default styles, and no-op callbacks.
    ///
    /// The color constants are not applied automatically; configure them with the color builders.
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

    /// Sets whether pointer interaction and hover styling are disabled.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Overrides the normal style's text color.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Overrides the hover style's text color.
    pub fn hover_color(mut self, color: impl Into<Color>) -> Self {
        self.hover_color = Some(color.into());
        self
    }

    /// Overrides the disabled style's text color.
    pub fn disabled_color(mut self, color: impl Into<Color>) -> Self {
        self.disabled_color = Some(color.into());
        self
    }

    /// Replaces the style used while enabled and not hovered.
    pub fn style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Replaces the style used while the mouse is over an enabled button.
    pub fn hover_style(mut self, style: TextStyle) -> Self {
        self.hover_style = style;
        self
    }

    /// Replaces the style used while disabled.
    pub fn disabled_style(mut self, style: TextStyle) -> Self {
        self.disabled_style = style;
        self
    }

    /// Sets the callback invoked for every completed primary press.
    ///
    /// Both the first and second presses of a double press invoke this callback.
    pub fn on_press(mut self, callback: impl Into<VoidCallback>) -> Self {
        self.on_press = callback.into();
        self
    }

    /// Sets the callback additionally invoked when two presses finish within 500 milliseconds.
    pub fn on_double_press(mut self, callback: impl Into<VoidCallback>) -> Self {
        self.on_double_press = callback.into();
        self
    }
}

impl Widget for TextButton {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        RawTextButton {
            widget: self.clone(),
            hovered: Cell::new(false),
            interaction: RefCell::new(ButtonInteraction::default()),
            last_tap: Cell::new(None),
            bounds: TextHitBounds::default(),
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
        if pressed {
            ButtonAction::Press
        } else {
            ButtonAction::None
        }
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
    bounds: TextHitBounds,
}

#[derive(Debug, Default)]
struct TextHitBounds {
    lines: RefCell<Vec<CacheBounds>>,
}

impl TextHitBounds {
    fn save(
        &self,
        scale: f32,
        x: f32,
        y: f32,
        line_widths: &[f32],
        line_height: f32,
        total_height: f32,
    ) {
        let mut lines = self.lines.borrow_mut();
        lines.clear();
        for (index, width) in line_widths
            .iter()
            .copied()
            .enumerate()
        {
            let offset_y = index as f32 * line_height;
            let height = line_height.min((total_height - offset_y).max(0.0));
            let bounds = CacheBounds::new();
            bounds.save(scale, x, y + offset_y, width, height);
            lines.push(bounds);
        }
    }

    fn is_inside(&self, x: f32, y: f32) -> bool {
        self.lines
            .borrow()
            .iter()
            .any(|bounds| bounds.is_inside(x, y))
    }
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

    fn text_layout<'a>(
        &self,
        ctx: &BuildContext<'a>,
    ) -> (
        RawTextWidget,
        BuildContext<'a>,
        aimer_attribute::ResolvedSize,
        Vec<f32>,
        f32,
    ) {
        let mut intrinsic_text = self.text_element();
        let wraps = matches!(
            intrinsic_text
                .text_style
                .text_overflow,
            TextOverflow::Wrap
        );
        intrinsic_text
            .text_style
            .text_overflow = TextOverflow::Clip;
        let intrinsic_size = intrinsic_text.computed_size(ctx);

        let mut text_ctx = ctx.clone();
        if wraps {
            let available_width = if ctx.box_constraint.max_width > 0.0 {
                ctx.box_constraint.max_width
            } else {
                ctx.parent_size.width
            };
            let width = intrinsic_size
                .width
                .min(available_width);
            text_ctx
                .box_constraint
                .max_width = width;
            text_ctx.parent_size.width = width;
        }

        let text = self.text_element();
        let size = if wraps {
            text.computed_size(&text_ctx)
        } else {
            intrinsic_size
        };
        let (line_widths, line_height) = if wraps {
            let font_size = text.font_size(text_ctx.scale);
            let metrics = text_ctx
                .canvas
                .measure_text_metrics_styled(
                    &text.text,
                    font_size,
                    text_ctx.parent_size.width,
                    text.text_style.font_family,
                    text.text_style.font_style,
                    text.text_style
                        .font_weight
                        .numeric(),
                );
            let widths = text_ctx
                .canvas
                .measure_text_line_widths_styled(
                    &text.text,
                    font_size,
                    text_ctx.parent_size.width,
                    text.text_style.font_family,
                    text.text_style.font_style,
                    text.text_style
                        .font_weight
                        .numeric(),
                );
            (widths, metrics.line_height)
        } else {
            (vec![size.width], size.height)
        };
        (text, text_ctx, size, line_widths, line_height)
    }

    fn save_bounds(
        &self,
        ctx: &BuildContext,
        size: aimer_attribute::ResolvedSize,
        line_widths: &[f32],
        line_height: f32,
    ) {
        let (x, y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(ctx.scale, x, y, line_widths, line_height, size.height);
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
                self.set_hovered(
                    self.bounds
                        .is_inside(pos.x, pos.y)
                        && !self.widget.disabled,
                );
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
                let inside = self
                    .bounds
                    .is_inside(pos.x, pos.y);
                self.interaction
                    .borrow_mut()
                    .pointer_down(inside, self.widget.disabled);
                inside && !self.widget.disabled
            }
            ElementEvent::PointerUp(pos, _, _) => {
                let action = self
                    .interaction
                    .borrow_mut()
                    .pointer_up(
                        self.bounds
                            .is_inside(pos.x, pos.y),
                        self.widget.disabled,
                    );
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
        let (_, _, size, line_widths, line_height) = self.text_layout(ctx);
        self.save_bounds(ctx, size, &line_widths, line_height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> aimer_attribute::ResolvedSize {
        self.text_layout(ctx).2
    }
}

impl Drawable for RawTextButton {
    fn draw(&self, ctx: &BuildContext) {
        let (text, text_ctx, size, line_widths, line_height) = self.text_layout(ctx);
        self.save_bounds(ctx, size, &line_widths, line_height);
        if !self.widget.disabled {
            self.set_hovered(
                self.bounds
                    .is_inside(ctx.cursor_pos.x, ctx.cursor_pos.y),
            );
        }
        text.draw(&text_ctx);
    }
}

impl Rebuildable for RawTextButton {}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use aimer_attribute::{BoxConstraint, ResolvedSize, Vec2d};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_style::TextDecoration;
    use aimer_widget::base::WindowHandle;

    use super::*;

    fn context(max_width: f32) -> BuildContext<'static> {
        context_with_canvas(max_width).0
    }

    fn context_with_canvas(max_width: f32) -> (BuildContext<'static>, aimer_canvas::InnerCanvas) {
        let canvas = aimer_canvas::InnerCanvas::new();
        let inner = Box::leak(Box::new(canvas.clone()));
        let mut ctx = BuildContext::new(
            aimer_canvas::Canvas::new(inner),
            ResolvedSize {
                width: max_width,
                height: 100.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = BoxConstraint::new()
            .max_width(max_width)
            .max_height(100.0);
        (ctx, canvas)
    }

    fn raw_button(widget: TextButton) -> RawTextButton {
        RawTextButton {
            widget,
            hovered: Cell::new(false),
            interaction: RefCell::new(ButtonInteraction::default()),
            last_tap: Cell::new(None),
            bounds: TextHitBounds::default(),
        }
    }

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

    #[test]
    fn wrapped_text_button_shrink_wraps_label_and_hitbox() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let ctx = context(300.0);
        let button = raw_button(
            TextButton::new("Open").style(TextStyle::default().text_overflow(TextOverflow::Wrap)),
        );
        let mut intrinsic_text = button.text_element();
        intrinsic_text
            .text_style
            .text_overflow = TextOverflow::Clip;
        let intrinsic = intrinsic_text.computed_size(&ctx);

        let size = button.layout(&ctx);

        assert_eq!(size, intrinsic);
        assert!(!button.on_event(&ElementEvent::PointerDown(
            Vec2d {
                x: intrinsic.width + 1.0,
                y: intrinsic.height / 2.0,
            },
            PointerSource::Mouse,
            0,
        )));
    }

    #[test]
    fn wrapped_text_button_respects_narrow_available_width() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let ctx = context(30.0);
        let button = raw_button(
            TextButton::new("A label that wraps")
                .style(TextStyle::default().text_overflow(TextOverflow::Wrap)),
        );

        let size = button.layout(&ctx);

        assert_eq!(size.width, 30.0);
        assert!(size.height > 17.0);
        assert!(!button.on_event(&ElementEvent::PointerDown(
            Vec2d {
                x: 31.0,
                y: size.height / 2.0,
            },
            PointerSource::Mouse,
            0,
        )));
    }

    #[test]
    fn wrapped_text_button_excludes_empty_space_after_short_final_line() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let ctx = context(45.0);
        let button = raw_button(
            TextButton::new("MMMM i").style(TextStyle::default().text_overflow(TextOverflow::Wrap)),
        );

        let size = button.layout(&ctx);

        assert!(size.height > 17.0);
        assert!(button.on_event(&ElementEvent::PointerDown(
            Vec2d {
                x: 1.0,
                y: size.height - 1.0,
            },
            PointerSource::Mouse,
            0,
        )));
        assert!(!button.on_event(&ElementEvent::PointerDown(
            Vec2d {
                x: size.width - 1.0,
                y: size.height - 1.0,
            },
            PointerSource::Mouse,
            0,
        )));
    }

    #[test]
    fn wrapped_underlined_text_button_draws_each_line_decoration() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _guard = runtime.enter();
        let (ctx, canvas) = context_with_canvas(45.0);
        let button = raw_button(
            TextButton::new("MMMM i").style(
                TextStyle::default()
                    .text_overflow(TextOverflow::Wrap)
                    .text_decoration(TextDecoration::Underline),
            ),
        );

        button.draw(&ctx);

        let draw_list = canvas.draw_list();
        let decorations = draw_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawTextDecoration { rect, .. } => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(decorations.len(), 2);
        assert!(decorations[1].y > decorations[0].y);
        assert!(decorations[1].width < decorations[0].width);
    }
}
