use attribute::dimension::Dimension;
use std::cell::UnsafeCell;
use attribute::CacheBounds;
use widget::{Element, LayoutCache, Widget, base::*, WidgetConstructor};
use widget::style::box_decoration::BoxDecoration;
use crate::gesture::gesture_detector::GestureDetectorElement;
use crate::gesture::{CallbackHolder, GestureActions};

#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Button<W: Widget + 'static> {
    #[constructor(default, into)]
    pub on_press: CallbackHolder<(), ()>,
    #[constructor(default, into)]
    pub on_long_press: CallbackHolder<(), ()>,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default)]
    pub decoration: BoxDecoration,
    #[constructor(default)]
    pub hover_decoration: BoxDecoration,
    #[constructor(default)]
    pub is_disabled: bool,
    #[constructor(default)]
    pub pressed_decoration: BoxDecoration,
    #[constructor(default)]
    pub disabled_decoration: BoxDecoration,
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
            width: self.width,
            height: self.height,
            decoration: self.decoration.clone(),
            hover_decoration: self.hover_decoration.clone(),
            pressed_decoration: self.pressed_decoration.clone(),
            disabled_decoration: self.disabled_decoration.clone(),
            is_disabled: self.is_disabled,
            is_hovered: UnsafeCell::new(false),
            is_pressed: UnsafeCell::new(false),
            gesture: UnsafeCell::new(gesture),
            is_mouse_down: UnsafeCell::new(false),
            is_dirty: UnsafeCell::new(true),
            child,
            cache: LayoutCache::new(),
            cached_bounds: CacheBounds::new(),
            window: ctx.window,
        })
    }
}
