use attribute::dimension::Dimension;
use std::cell::UnsafeCell;

#[cfg(not(target_arch = "wasm32"))]
use skia_safe::{Color as SkColor, Paint, Rect, paint::Style};
use widget::{Constructor, Element, LayoutCache, Widget, base::*};
use widget::style::border::{BorderStyle, BoxBorder, BoxOutline};
use crate::gesture::gesture_detector::GestureDetectorElement;
use crate::gesture::{CallbackHolder, GestureActions};

#[allow(dead_code)]
#[derive(Default, Clone, Copy, Constructor)]
pub struct ButtonStyle {
    #[constructor(default, into)]
    pub color: Colors,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default)]
    pub border: BoxBorder,
    #[constructor(default)]
    pub outline: BoxOutline
}

#[allow(dead_code)]
#[derive(Constructor)]
pub struct Button<W: Widget> {
    #[constructor(default, into)]
    pub on_press: CallbackHolder,
    #[constructor(default, into)]
    pub on_long_press: CallbackHolder,
    #[constructor(default)]
    pub style: ButtonStyle,
    #[constructor(default)]
    pub hover_style: ButtonStyle,
    #[constructor(default)]
    pub is_disabled: bool,
    #[constructor(default)]
    pub pressed_style: ButtonStyle,
    #[constructor(default)]
    pub disabled_style: ButtonStyle,
    child: W,
}

impl<W: Widget> Widget for Button<W> {
    #[inline]
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);

        let mut gesture = GestureActions::new();
        gesture.on_tap = self.on_press.clone();
        gesture.on_long_press = self.on_long_press.clone();
        #[cfg(not(target_arch = "wasm32"))]
        {
            gesture.runtime_handle = Some(ctx.async_handle.clone());
        }

        Box::new(GestureDetectorElement {
            style: self.style,
            hover_style: self.hover_style,
            pressed_style: self.pressed_style,
            disabled_style: self.disabled_style,
            is_disabled: self.is_disabled,
            is_hovered: UnsafeCell::new(false),
            is_pressed: UnsafeCell::new(false),
            gesture: UnsafeCell::new(gesture),
            is_mouse_down: UnsafeCell::new(false),
            is_dirty: UnsafeCell::new(true),
            child,
            cache: LayoutCache::new(),
            cached_bounds: UnsafeCell::new(None),
            window: ctx.window,
        })
    }
}
