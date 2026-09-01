//! Composable visual parts used by [`Slider`](super::Slider) controls.

use std::cell::Cell;
use std::rc::Rc;

use aimer_attribute::{CacheBounds, Dimension, ResolvedSize, Size, Vec2d};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutElement, PortableWidget, Rebuildable,
    VisitorElement, Widget,
};

/// Runtime interaction state shared by a slider and its default visual parts.
///
/// This is crate-private because application code controls visuals by passing
/// ordinary widgets to [`Slider::thumb`](super::Slider::thumb) and
/// [`Slider::trail`](super::Slider::trail). The shared cell is used by the
/// slider's built-in visual widgets to update pressed, focus, and disabled
/// colors during a redraw without rebuilding the slider tree.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SliderVisualState {
    pub(crate) disabled: bool,
    pub(crate) pressed: bool,
    pub(crate) focused: bool,
}

/// The default circular thumb rendered by a [`Slider`](super::Slider).
///
/// A thumb is also a complete widget, so it can be used directly in a custom
/// slider composition or configured with the builder methods below. When the
/// slider creates its own default thumb it supplies interaction state so the
/// focused and disabled colors continue to follow pointer and keyboard input.
#[derive(Clone)]
pub struct SliderThumb {
    size: f32,
    radius: f32,
    color: Color,
    focused_color: Color,
    disabled_color: Color,
    visual_state: Option<Rc<Cell<SliderVisualState>>>,
}

impl SliderThumb {
    /// Creates the default 20-pixel circular thumb.
    #[inline]
    pub fn new() -> Self {
        Self {
            size: 20.0,
            radius: 10.0,
            color: Color::WHITE,
            focused_color: Color::Rgba(20, 80, 190, 255),
            disabled_color: Color::Rgba(150, 155, 165, 180),
            visual_state: None,
        }
    }

    /// Sets the thumb's square edge length in logical pixels.
    #[inline]
    pub fn size(mut self, size: f32) -> Self {
        if size.is_finite() && size >= 0.0 {
            self.size = size;
        }
        self
    }

    /// Sets the thumb corner radius in logical pixels.
    #[inline]
    pub fn radius(mut self, radius: f32) -> Self {
        if radius.is_finite() && radius >= 0.0 {
            self.radius = radius;
        }
        self
    }

    /// Sets the normal thumb color.
    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the thumb color while a connected slider owns keyboard focus.
    ///
    /// The slider supplies interaction state to its built-in thumb. A thumb
    /// passed as a custom child is rendered with its configured normal color.
    #[inline]
    pub fn focused_color(mut self, color: Color) -> Self {
        self.focused_color = color;
        self
    }

    /// Sets the thumb color while a connected slider is disabled.
    ///
    /// The slider supplies interaction state to its built-in thumb. A thumb
    /// passed as a custom child is rendered with its configured normal color.
    #[inline]
    pub fn disabled_color(mut self, color: Color) -> Self {
        self.disabled_color = color;
        self
    }

    pub(crate) fn with_visual_state(
        mut self,
        visual_state: Rc<Cell<SliderVisualState>>,
    ) -> Self {
        self.visual_state = Some(visual_state);
        self
    }
}

impl Default for SliderThumb {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SliderThumb {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        RawSliderThumb {
            size: self.size,
            radius: self.radius,
            color: self.color,
            focused_color: self.focused_color,
            disabled_color: self.disabled_color,
            visual_state: self.visual_state,
            bounds: CacheBounds::new(),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "SliderThumb"
    }
}

impl PortableWidget for SliderThumb {}

struct RawSliderThumb {
    size: f32,
    radius: f32,
    color: Color,
    focused_color: Color,
    disabled_color: Color,
    visual_state: Option<Rc<Cell<SliderVisualState>>>,
    bounds: CacheBounds,
}

impl Drawable for RawSliderThumb {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let (x, y) = ctx.canvas.get_transform_translation();
        self.bounds.save(ctx.scale, x, y, size.width, size.height);

        let state = self
            .visual_state
            .as_ref()
            .map(|state| state.get())
            .unwrap_or_default();
        let color = if state.disabled {
            self.disabled_color
        } else if state.focused {
            self.focused_color
        } else {
            self.color
        };
        let radius = (self.radius * ctx.scale)
            .min(size.width / 2.0)
            .min(size.height / 2.0)
            .max(0.0);
        ctx.canvas
            .fill_color_rect(Vec2d::default(), size, color, [radius; 4]);
    }
}

impl EventElement for RawSliderThumb {}

impl LayoutElement for RawSliderThumb {
    fn size(&self) -> Option<Size> {
        Some(Size::new(self.size, self.size))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let requested = (self.size.max(0.0) * ctx.scale.max(0.0)).max(0.0);
        ResolvedSize {
            width: requested.clamp(ctx.box_constraint.min_width, ctx.box_constraint.max_width),
            height: requested.clamp(ctx.box_constraint.min_height, ctx.box_constraint.max_height),
        }
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.pos_start_end()
    }
}

impl Rebuildable for RawSliderThumb {}

impl VisitorElement for RawSliderThumb {
    fn debug_name(&self) -> &'static str {
        "SliderThumb"
    }
}

/// The default active trail rendered by a [`Slider`](super::Slider).
///
/// The slider positions and clips this widget to the portion represented by
/// the current value. Consequently a custom trail can use gradients, images,
/// or any other widget while retaining the slider's pointer and keyboard
/// behavior.
#[derive(Clone)]
pub struct SliderTrail {
    width: Dimension,
    height: f32,
    radius: f32,
    color: Color,
    pressed_color: Color,
    disabled_color: Color,
    visual_state: Option<Rc<Cell<SliderVisualState>>>,
}

impl SliderTrail {
    /// Creates a full-width, four-pixel rounded trail.
    #[inline]
    pub fn new() -> Self {
        Self {
            width: Dimension::Percent(100.0),
            height: 4.0,
            radius: 2.0,
            color: Color::Rgba(35, 110, 220, 255),
            pressed_color: Color::Rgba(20, 80, 190, 255),
            disabled_color: Color::Rgba(35, 110, 220, 100),
            visual_state: None,
        }
    }

    /// Sets the trail width before the slider clips it to the active segment.
    #[inline]
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the trail height in logical pixels.
    #[inline]
    pub fn height(mut self, height: f32) -> Self {
        if height.is_finite() && height >= 0.0 {
            self.height = height;
        }
        self
    }

    /// Sets the trail corner radius in logical pixels.
    #[inline]
    pub fn radius(mut self, radius: f32) -> Self {
        if radius.is_finite() && radius >= 0.0 {
            self.radius = radius;
        }
        self
    }

    /// Sets the normal active-trail color.
    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the active-trail color while a connected slider is pressed.
    ///
    /// The slider supplies interaction state to its built-in trail. A trail
    /// passed as a custom child is rendered with its configured normal color.
    #[inline]
    pub fn pressed_color(mut self, color: Color) -> Self {
        self.pressed_color = color;
        self
    }

    /// Sets the active-trail color while a connected slider is disabled.
    ///
    /// The slider supplies interaction state to its built-in trail. A trail
    /// passed as a custom child is rendered with its configured normal color.
    #[inline]
    pub fn disabled_color(mut self, color: Color) -> Self {
        self.disabled_color = color;
        self
    }

    pub(crate) fn with_visual_state(
        mut self,
        visual_state: Rc<Cell<SliderVisualState>>,
    ) -> Self {
        self.visual_state = Some(visual_state);
        self
    }
}

impl Default for SliderTrail {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SliderTrail {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        RawSliderTrail {
            width: self.width,
            height: self.height,
            radius: self.radius,
            color: self.color,
            pressed_color: self.pressed_color,
            disabled_color: self.disabled_color,
            visual_state: self.visual_state,
            bounds: CacheBounds::new(),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "SliderTrail"
    }
}

impl PortableWidget for SliderTrail {}

struct RawSliderTrail {
    width: Dimension,
    height: f32,
    radius: f32,
    color: Color,
    pressed_color: Color,
    disabled_color: Color,
    visual_state: Option<Rc<Cell<SliderVisualState>>>,
    bounds: CacheBounds,
}

impl Drawable for RawSliderTrail {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let (x, y) = ctx.canvas.get_transform_translation();
        self.bounds.save(ctx.scale, x, y, size.width, size.height);

        let state = self
            .visual_state
            .as_ref()
            .map(|state| state.get())
            .unwrap_or_default();
        let color = if state.disabled {
            self.disabled_color
        } else if state.pressed {
            self.pressed_color
        } else {
            self.color
        };
        let radius = (self.radius * ctx.scale)
            .min(size.width / 2.0)
            .min(size.height / 2.0)
            .max(0.0);
        ctx.canvas
            .fill_color_rect(Vec2d::default(), size, color, [radius; 4]);
    }
}

impl EventElement for RawSliderTrail {}

impl LayoutElement for RawSliderTrail {
    fn size(&self) -> Option<Size> {
        Some(Size::new(self.width, Dimension::Px(self.height)))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let width = self
            .width
            .resolve(ctx.box_constraint.max_width, ctx.scale)
            .max(0.0);
        let height = (self.height.max(0.0) * ctx.scale.max(0.0)).max(0.0);
        ResolvedSize {
            width: width.clamp(ctx.box_constraint.min_width, ctx.box_constraint.max_width),
            height: height.clamp(ctx.box_constraint.min_height, ctx.box_constraint.max_height),
        }
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.pos_start_end()
    }
}

impl Rebuildable for RawSliderTrail {}

impl VisitorElement for RawSliderTrail {
    fn debug_name(&self) -> &'static str {
        "SliderTrail"
    }
}
