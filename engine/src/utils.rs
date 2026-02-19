#[cfg(target_os = "ios")]
extern crate objc;
#[cfg(target_os = "ios")]
use objc::runtime::{Class, Object};
#[cfg(target_os = "ios")]
use objc::msg_send;
#[cfg(target_os = "ios")]
use objc::sel;
#[cfg(target_os = "ios")]
use objc::sel_impl;
#[cfg(target_os = "ios")]
pub fn get_screen_resolution_pixels() -> Option<(f64, f64)> {
    let uiscreen_class = Class::get("UIScreen")?;
    
    let main_screen: *mut Object = unsafe {
        msg_send![uiscreen_class, mainScreen]
    };

    if main_screen.is_null() {
        return None;
    }

    let native_bounds: CGRect = unsafe {
        msg_send![main_screen, nativeBounds]
    };

    Some((native_bounds.size.width, native_bounds.size.height))
}

#[cfg(target_os = "ios")]
#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[cfg(target_os = "ios")]
#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "ios")]
#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}
