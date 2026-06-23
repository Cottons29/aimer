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

/// Attaches the current thread to the JVM and invokes `call` with a live `Env`
/// and the running `com.aimer.AimerActivity` instance.
///
/// The activity object and the `JavaVM` are obtained from `ndk-context`, which
/// `android-activity` populates on start-up. The Java side marshals the actual
/// UI work onto the main thread (`runOnUiThread`), so it is safe to invoke this
/// from the winit event-loop thread.
#[cfg(target_os = "android")]
fn with_activity(call: impl FnOnce(&mut jni::Env, &jni::objects::JObject)) {
    use jni::objects::JObject;

    let ctx = ndk_context::android_context();
    let vm_ptr = ctx.vm() as *mut jni::sys::JavaVM;
    let activity_ptr = ctx.context() as jni::sys::jobject;
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return;
    }

    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr) };
    let _ = vm.attach_current_thread(|env| -> std::result::Result<(), jni::errors::Error> {
        // SAFETY: `activity_ptr` is the global activity reference owned by the
        // android-activity runtime and stays valid for the lifetime of the app.
        let activity = unsafe { JObject::from_raw(&*env, activity_ptr) };
        call(env, &activity);
        Ok(())
    });
}

/// Raises the soft keyboard by focusing the hidden `EditText` owned by
/// `com.aimer.AimerActivity`, so IME-composed (CJK) text can be captured.
#[cfg(target_os = "android")]
pub fn show_keyboard() {
    with_activity(|env, activity| {
        let _ = env.call_method(activity, jni::jni_str!("showKeyboard"), jni::jni_sig!("()V"), &[]);
    });
}

/// Dismisses the soft keyboard.
#[cfg(target_os = "android")]
pub fn hide_keyboard() {
    with_activity(|env, activity| {
        let _ = env.call_method(activity, jni::jni_str!("hideKeyboard"), jni::jni_sig!("()V"), &[]);
    });
}
