extern crate objc;

use objc::runtime::{Class, Object};

use objc::msg_send;

use objc::sel;

use objc::sel_impl;

pub fn get_screen_resolution_pixels() -> Option<(f64, f64)> {
    let screen_class = Class::get("UIScreen")?;

    let main_screen: *mut Object = unsafe { msg_send![screen_class, mainScreen] };

    if main_screen.is_null() {
        return None;
    }

    let native_bounds: CGRect = unsafe { msg_send![main_screen, nativeBounds] };

    Some((native_bounds.size.width, native_bounds.size.height))
}

#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}
