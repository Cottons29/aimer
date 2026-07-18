extern crate objc;

use aimer_utils::info;
use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Associate winit's `UIWindow` with the foreground-active `UIWindowScene`.
///
/// winit (0.30) creates its `UIWindow` with `initWithFrame:` and calls
/// `makeKeyAndVisible()`, but it never sets a `UIWindowScene`. Starting with
/// the iOS 26 / 27 SDK the UIScene life cycle is mandatory (see Apple TN3187):
/// a scene-less window is never displayed and never receives layout / redraw
/// callbacks, so the app shows a black screen and `RedrawRequested` never
/// fires.
///
/// Here we recover winit's `UIWindow` from the `UIView` exposed through
/// `raw-window-handle`, attach it to the active window scene and re-assert
/// `makeKeyAndVisible()` so the content — and the GPU redraw loop — start.
pub fn attach_window_to_active_scene(window: &Window) {
    let ui_view = match window.window_handle() {
        Ok(handle) => match handle.as_raw() {
            RawWindowHandle::UiKit(uikit) => uikit
                .ui_view
                .as_ptr() as *mut Object,
            _ => return,
        },
        Err(_) => return,
    };

    if ui_view.is_null() {
        return;
    }

    unsafe {
        #[allow(unexpected_cfgs)]
        let ui_window: *mut Object = msg_send![ui_view, window];
        if ui_window.is_null() {
            info!("attach_window_to_active_scene: winit view has no UIWindow yet");
            return;
        }

        // Already associated with a scene: nothing to do.
        #[allow(unexpected_cfgs)]
        let existing_scene: *mut Object = msg_send![ui_window, windowScene];
        if !existing_scene.is_null() {
            return;
        }

        let Some(app_class) = Class::get("UIApplication") else { return };
        #[allow(unexpected_cfgs)]
        let app: *mut Object = msg_send![app_class, sharedApplication];
        if app.is_null() {
            return;
        }

        #[allow(unexpected_cfgs)]
        let scenes: *mut Object = msg_send![app, connectedScenes];
        #[allow(unexpected_cfgs)]
        let all: *mut Object = msg_send![scenes, allObjects];
        #[allow(unexpected_cfgs)]
        let count: usize = msg_send![all, count];

        let Some(window_scene_class) = Class::get("UIWindowScene") else { return };

        // Prefer a foreground-active window scene, otherwise the first one found.
        let mut chosen: *mut Object = std::ptr::null_mut();
        for i in 0..count {
            #[allow(unexpected_cfgs)]
            let scene: *mut Object = msg_send![all, objectAtIndex: i];
            #[allow(unexpected_cfgs)]
            let is_window_scene: bool = msg_send![scene, isKindOfClass: window_scene_class];
            if !is_window_scene {
                continue;
            }
            // `UISceneActivationStateForegroundActive == 0`.
            #[allow(unexpected_cfgs)]
            let activation_state: isize = msg_send![scene, activationState];
            if activation_state == 0 {
                chosen = scene;
                break;
            }
            if chosen.is_null() {
                chosen = scene;
            }
        }

        if chosen.is_null() {
            info!("attach_window_to_active_scene: no UIWindowScene available yet");
            return;
        }

        #[allow(unexpected_cfgs)]
        let _: () = msg_send![ui_window, setWindowScene: chosen];
        #[allow(unexpected_cfgs)]
        let _: () = msg_send![ui_window, makeKeyAndVisible];
        info!("attach_window_to_active_scene: attached winit UIWindow to active UIWindowScene");
    }
}

pub fn get_screen_resolution_pixels() -> Option<(f64, f64)> {
    let screen_class = Class::get("UIScreen")?;

    let main_screen: *mut Object = unsafe {
        #[allow(unexpected_cfgs)]
        msg_send![screen_class, mainScreen]
    };

    if main_screen.is_null() {
        return None;
    }

    let native_bounds: CGRect = unsafe {
        #[allow(unexpected_cfgs)]
        msg_send![main_screen, nativeBounds]
    };

    Some((
        native_bounds
            .size
            .width,
        native_bounds
            .size
            .height,
    ))
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
