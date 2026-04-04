#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
static ANDROID_APP: OnceLock<AndroidApp> = OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_android_app(app: AndroidApp) {
    let _ = ANDROID_APP.set(app);
}

#[cfg(target_os = "android")]
pub fn get_android_app() -> Option<&'static AndroidApp> {
    ANDROID_APP.get()
}
