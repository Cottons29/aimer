extern crate objc;

use objc::runtime::{Class, Object};

use objc::msg_send;

use objc::sel;

use objc::sel_impl;
use utils::info;

pub fn get_screen_resolution_pixels() -> Option<(f64, f64)> {
    let screen_class = Class::get("UIScreen")?;

    let main_screen: *mut Object = unsafe { msg_send![screen_class, mainScreen] };

    if main_screen.is_null() {
        return None;
    }

    let native_bounds: CGRect = unsafe { msg_send![main_screen, nativeBounds] };

    Some((native_bounds.size.width, native_bounds.size.height))
}
#[derive(Debug)]
#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}
#[derive(Debug)]
#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[derive(Debug)]
#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}
