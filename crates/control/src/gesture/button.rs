use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use std::cell::UnsafeCell;

#[cfg(not(target_arch = "wasm32"))]
use skia_safe::{paint::Style, Color as SkColor, Paint, Rect};
use widget::{base::*, style::BoxConstraint, Constructor, Element, ElementEvent, LayoutCache, Widget};
use winit::window::Window;

use crate::event::{PointerEvent, PointerPosition};
use crate::gesture::{CallbackHolder, GestureActions};
use crate::gesture::gesture_detector::GestureDetectorElement;

#[allow(dead_code)]
#[derive(Default, Clone, Copy, Constructor)]
pub struct ButtonStyle {
    #[constructor(default, into)]
    pub color: Colors,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default, into)]
    pub width: Dimension,
}

#[allow(dead_code)]
#[derive(Constructor)]
pub struct Button {
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
    child: Box<dyn Widget>,
}

impl Widget for Button {
    #[inline]
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);

        let mut gesture = GestureActions::new();
        gesture.on_tap = self.on_press.clone();
        gesture.on_long_press = self.on_long_press.clone();
        gesture.runtime_handle = Some(ctx.async_handle.clone());

        Box::new(GestureDetectorElement {
            style: self.style,
            hover_style: self.hover_style,
            is_disabled: self.is_disabled,
            is_hovered: UnsafeCell::new(false),
            is_pressed: UnsafeCell::new(false),
            gesture: UnsafeCell::new(gesture),
            is_mouse_down: UnsafeCell::new(false),
            child,
            cache: LayoutCache::new(),
            cached_bounds: UnsafeCell::new(None),
            window: ctx.window,
        })
    }
}

