#[cfg(target_os = "ios")]
mod ios_keyboard {
    use std::ffi::{CStr, c_char, c_void};
    use std::sync::OnceLock;

    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    type VoidFn = unsafe extern "C" fn();
    type SyncFn = unsafe extern "C" fn(
        u64,
        u64,
        *const u8,
        usize,
        usize,
        usize,
        isize,
        isize,
        bool,
        i32,
    );

    static SHOW_FN: OnceLock<Option<VoidFn>> = OnceLock::new();
    static DISMISS_FN: OnceLock<Option<VoidFn>> = OnceLock::new();
    static SYNC_FN: OnceLock<Option<SyncFn>> = OnceLock::new();

    fn lookup(name: &CStr) -> Option<VoidFn> {
        unsafe {
            let ptr = dlsym(RTLD_DEFAULT, name.as_ptr());
            if ptr.is_null() {
                None
            } else {
                Some(std::mem::transmute::<*mut c_void, VoidFn>(ptr))
            }
        }
    }

    fn lookup_sync(name: &CStr) -> Option<SyncFn> {
        unsafe {
            let ptr = dlsym(RTLD_DEFAULT, name.as_ptr());
            (!ptr.is_null()).then(|| std::mem::transmute::<*mut c_void, SyncFn>(ptr))
        }
    }

    pub fn show_keyboard() {
        let f = SHOW_FN.get_or_init(|| lookup(c"aimer_ios_show_keyboard"));
        if let Some(f) = f {
            unsafe { f() }
        }
    }

    pub fn dismiss_keyboard() {
        let f = DISMISS_FN.get_or_init(|| lookup(c"aimer_ios_dismiss_keyboard"));
        if let Some(f) = f {
            unsafe { f() }
        }
    }

    pub fn sync_text_state(
        session_id: u64,
        revision: u64,
        text: &str,
        selection: (usize, usize),
        composing: Option<(usize, usize)>,
        secure: bool,
        input_kind: i32,
    ) {
        let f = SYNC_FN.get_or_init(|| lookup_sync(c"aimer_ios_sync_text_state"));
        if let Some(f) = f {
            let (composing_start, composing_end) = composing
                .map(|(start, end)| (start as isize, end as isize))
                .unwrap_or((-1, -1));
            unsafe {
                f(
                    session_id,
                    revision,
                    text.as_ptr(),
                    text.len(),
                    selection.0,
                    selection.1,
                    composing_start,
                    composing_end,
                    secure,
                    input_kind,
                )
            }
        }
    }
}

#[cfg(target_os = "android")]
mod android_keyboard {
    use aimer_events::android_app;

    pub fn show_keyboard() {
        // Focus the hidden `EditText` owned by `com.aimer.AimerActivity` and raise
        // the soft keyboard. Composed (CJK) text is captured there and forwarded
        // back into Rust via the `nativeInsertText` JNI bridge. The previous
        // `AndroidApp::show_soft_input` only raised the keyboard against the bare
        // native surface, which has no `InputConnection`, so IME-composed text was
        // silently dropped.
        android_app::show_keyboard();
    }

    pub fn dismiss_keyboard() {
        android_app::hide_keyboard();
    }

    pub fn sync_text_state(
        session_id: u64,
        revision: u64,
        text: &str,
        selection: (usize, usize),
        composing: Option<(usize, usize)>,
        secure: bool,
        input_kind: i32,
    ) {
        android_app::sync_text_state(
            session_id,
            revision,
            text,
            selection,
            composing,
            secure,
            input_kind,
        );
    }
}

impl RawTextField {
    /// Mirrors the controller state into the platform text editor.
    ///
    /// The pushed snapshot is also what re-anchors the native delta
    /// acceptance window: the editor bases its next deltas on this revision
    /// ([`Self::native_base_revision`]), and its buffer mirrors this value
    /// ([`Self::native_mirror_revision`]) until the controller changes for
    /// any reason other than those deltas.
    fn sync_platform_text_state(&self) {
        if self.native_session.get() == 0 {
            return;
        }
        self.native_base_revision.set(self.controller.revision());
        self.native_mirror_revision.set(self.controller.revision());
        ime_trace!(
            "snapshot push: session={} rev={} text={:?} composing={:?}",
            self.native_session.get(),
            self.controller.revision(),
            self.controller.text(),
            self.controller.value().composing().map(|r| (r.start(), r.end())),
        );
        self.push_platform_text_state();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn push_platform_text_state(&self) {
        let session_id = self.native_session.get();
        let value = self.controller.value();
        let selection = (
            crate::editable_text::byte_to_utf16(value.text(), value.selection().anchor())
                .unwrap_or(0),
            crate::editable_text::byte_to_utf16(value.text(), value.selection().focus())
                .unwrap_or(0),
        );
        let composing = value.composing().map(|range| {
            (
                crate::editable_text::byte_to_utf16(value.text(), range.start()).unwrap_or(0),
                crate::editable_text::byte_to_utf16(value.text(), range.end()).unwrap_or(0),
            )
        });
        let secure = self.input_type == InputType::Obscure;
        let input_kind = match self.input_type {
            InputType::Text => 0,
            InputType::Number => 1,
            InputType::Obscure => 2,
        };
        #[cfg(target_os = "ios")]
        ios_keyboard::sync_text_state(
            session_id,
            self.controller.revision(),
            value.text(),
            selection,
            composing,
            secure,
            input_kind,
        );
        #[cfg(target_os = "android")]
        android_keyboard::sync_text_state(
            session_id,
            self.controller.revision(),
            value.text(),
            selection,
            composing,
            secure,
            input_kind,
        );
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn push_platform_text_state(&self) {}
}

/// Identifier of the hidden `<input>` element the browser composes into.
#[cfg(target_arch = "wasm32")]
const HIDDEN_INPUT_ID: &str = "__aimer_hidden_input";

/// Identifier of the `<canvas>` element the application is rendered into.
#[cfg(target_arch = "wasm32")]
const CANVAS_ID: &str = "aimer_app";

/// Converts a caret rectangle expressed in logical canvas coordinates into the
/// viewport rectangle the hidden input element is placed at.
///
/// The browser anchors the candidate window to the element that owns the
/// composition, so the caret can only be reported by moving that element on top
/// of it. `canvas_origin` is the position of the rendering canvas inside the
/// viewport, which is what turns canvas-local coordinates into the coordinates
/// a `position: fixed` element is laid out in; it is not constant, because the
/// page may scroll or embed the canvas in a larger document.
///
/// The rectangle is never allowed to collapse: a zero-sized element has no
/// position to anchor to, and an empty field legitimately reports a caret of no
/// width.
#[cfg(any(target_arch = "wasm32", test))]
fn ime_overlay_rect(caret: ImeCaretArea, canvas_origin: (f32, f32)) -> ImeCaretArea {
    ImeCaretArea {
        x: canvas_origin.0 + caret.x,
        y: canvas_origin.1 + caret.y,
        width: caret.width.max(RawTextField::IME_CARET_WIDTH),
        height: caret.height.max(RawTextField::IME_CARET_WIDTH),
    }
}

/// Returns the position of the rendering canvas inside the viewport, in CSS
/// pixels.
///
/// A missing canvas resolves to the viewport origin so that a caret is still
/// reported at a sane place instead of not at all.
#[cfg(target_arch = "wasm32")]
fn wasm_canvas_origin() -> (f32, f32) {
    let Some(canvas) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(CANVAS_ID))
    else {
        return (0.0, 0.0);
    };
    let rect = canvas.get_bounding_client_rect();
    (rect.left() as f32, rect.top() as f32)
}

/// Moves the hidden input element over `caret`, given in viewport coordinates.
///
/// The element stays invisible; only its box moves, which is enough for the
/// browser to draw the composition popup at the insertion point instead of at
/// the corner the element would otherwise be parked in.
#[cfg(target_arch = "wasm32")]
fn wasm_place_ime_input(caret: ImeCaretArea) {
    use wasm_bindgen::JsCast;

    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(HIDDEN_INPUT_ID))
    else {
        return;
    };
    let style = element.unchecked_into::<web_sys::HtmlElement>().style();
    style.set_property("left", &format!("{}px", caret.x)).ok();
    style.set_property("top", &format!("{}px", caret.y)).ok();
    style
        .set_property("width", &format!("{}px", caret.width))
        .ok();
    style
        .set_property("height", &format!("{}px", caret.height))
        .ok();
}

/// On wasm32 / mobile browsers, focusing a hidden `<input>` element inside a
/// user-gesture handler is the only reliable way to raise the virtual keyboard.
///
/// Event listeners on the hidden input re-dispatch `keydown` and `input` events
/// to the winit canvas (`#aimer_app`) so that the framework's normal keyboard
/// pipeline (`WindowEvent::KeyboardInput`) still fires.
#[cfg(target_arch = "wasm32")]
fn wasm_request_keyboard(show: bool) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    let input: web_sys::HtmlInputElement = match document.get_element_by_id(HIDDEN_INPUT_ID) {
        Some(el) => el.unchecked_into(),
        None => {
            let el = document
                .create_element("input")
                .expect("failed to create hidden input")
                .unchecked_into::<web_sys::HtmlInputElement>();
            el.set_id(HIDDEN_INPUT_ID);
            el.set_type("text");
            el.set_attribute("autocapitalize", "off").ok();
            el.set_attribute("autocomplete", "off").ok();
            el.set_attribute("autocorrect", "off").ok();
            el.set_attribute("spellcheck", "false").ok();
            // The element is invisible but must stay inside the viewport: the
            // browser anchors the IME candidate window to the box of the
            // element being composed into, so parking it off-screen is what
            // made the popup appear away from the field. It is moved onto the
            // caret by `wasm_place_ime_input` and never receives pointer
            // events, so it cannot steal clicks from the canvas underneath.
            let style = el.style();
            style.set_property("position", "fixed").ok();
            style.set_property("opacity", "0").ok();
            style.set_property("pointer-events", "none").ok();
            style.set_property("left", "0").ok();
            style.set_property("top", "0").ok();
            style.set_property("width", "1px").ok();
            style.set_property("height", "1px").ok();
            style.set_property("border", "none").ok();
            style.set_property("outline", "none").ok();
            style.set_property("padding", "0").ok();
            style.set_property("font-size", "16px").ok(); // prevents iOS zoom
            document.body().unwrap().append_child(&el).ok();

            // Forward keydown events to the winit canvas so the framework
            // receives them through its normal WindowEvent::KeyboardInput path.
            {
                let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                    move |evt: web_sys::KeyboardEvent| {
                        evt.stop_propagation();
                        evt.prevent_default();
                        let Some(w) = web_sys::window() else { return };
                        let Some(doc) = w.document() else { return };
                        let Some(canvas) = doc.get_element_by_id(CANVAS_ID) else {
                            return;
                        };
                        let new_evt = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                            evt.type_().as_str(),
                            web_sys::KeyboardEventInit::new()
                                .key(&evt.key())
                                .code(&evt.code())
                                .location(evt.location())
                                .repeat(evt.repeat())
                                .is_composing(evt.is_composing())
                                .bubbles(true)
                                .cancelable(true)
                                .ctrl_key(evt.ctrl_key())
                                .shift_key(evt.shift_key())
                                .alt_key(evt.alt_key())
                                .meta_key(evt.meta_key()),
                        )
                        .unwrap();
                        canvas.dispatch_event(&new_evt).ok();
                    },
                );
                el.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }

            // Forward keyup events as well.
            {
                let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                    move |evt: web_sys::KeyboardEvent| {
                        evt.stop_propagation();
                        evt.prevent_default();
                        let Some(w) = web_sys::window() else { return };
                        let Some(doc) = w.document() else { return };
                        let Some(canvas) = doc.get_element_by_id(CANVAS_ID) else {
                            return;
                        };
                        let new_evt = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                            evt.type_().as_str(),
                            web_sys::KeyboardEventInit::new()
                                .key(&evt.key())
                                .code(&evt.code())
                                .location(evt.location())
                                .repeat(evt.repeat())
                                .is_composing(evt.is_composing())
                                .bubbles(true)
                                .cancelable(true)
                                .ctrl_key(evt.ctrl_key())
                                .shift_key(evt.shift_key())
                                .alt_key(evt.alt_key())
                                .meta_key(evt.meta_key()),
                        )
                        .unwrap();
                        canvas.dispatch_event(&new_evt).ok();
                    },
                );
                el.add_event_listener_with_callback("keyup", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }

            // Handle compositionless text input (e.g. mobile virtual keyboards)
            // that may not fire keydown for each character.
            {
                let cb = Closure::<dyn FnMut(web_sys::InputEvent)>::new(
                    move |evt: web_sys::InputEvent| {
                        // IME-composed text (Chinese/Japanese/Korean, ...) is committed
                        // through the `compositionend` handler below. Skip every
                        // composition-related `input` event here so the composed result
                        // is never inserted twice.
                        if evt.is_composing() || evt.input_type() == "insertCompositionText" {
                            return;
                        }
                        let Some(data) = evt.data() else { return };
                        let Some(w) = web_sys::window() else { return };
                        let Some(doc) = w.document() else { return };
                        let Some(canvas) = doc.get_element_by_id(CANVAS_ID) else {
                            return;
                        };
                        // Synthesize a keydown + keyup pair for each character so
                        // winit can translate them into KeyboardInput events.
                        let chars: Vec<char> = data.chars().collect();
                        for ch in chars {
                            let key = ch.to_string();
                            for event_type in &["keydown", "keyup"] {
                                let synth =
                                    web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                                        event_type,
                                        web_sys::KeyboardEventInit::new()
                                            .key(&key)
                                            .bubbles(true)
                                            .cancelable(true),
                                    )
                                    .unwrap();
                                canvas.dispatch_event(&synth).ok();
                            }
                        }
                        // Clear the hidden input so subsequent input events keep working.
                        if let Some(el) = doc.get_element_by_id(HIDDEN_INPUT_ID) {
                            let el: web_sys::HtmlInputElement = el.unchecked_into();
                            el.set_value("");
                        }
                    },
                );
                el.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }

            // Commit IME-composed text (Chinese / Japanese / Korean, ...). The
            // browser fires `compositionend` with the final string once the user
            // accepts a candidate. This is the authoritative commit signal and is
            // forwarded as synthesized key events, mirroring the plain `input`
            // path so the framework inserts the composed characters exactly once.
            {
                let cb = Closure::<dyn FnMut(web_sys::CompositionEvent)>::new(
                    move |evt: web_sys::CompositionEvent| {
                        let Some(data) = evt.data() else { return };
                        if data.is_empty() {
                            return;
                        }
                        let Some(w) = web_sys::window() else { return };
                        let Some(doc) = w.document() else { return };
                        let Some(canvas) = doc.get_element_by_id(CANVAS_ID) else {
                            return;
                        };
                        for ch in data.chars() {
                            let key = ch.to_string();
                            for event_type in &["keydown", "keyup"] {
                                let synth =
                                    web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                                        event_type,
                                        web_sys::KeyboardEventInit::new()
                                            .key(&key)
                                            .bubbles(true)
                                            .cancelable(true),
                                    )
                                    .unwrap();
                                canvas.dispatch_event(&synth).ok();
                            }
                        }
                        // Clear the hidden input so the next composition starts clean.
                        if let Some(el) = doc.get_element_by_id(HIDDEN_INPUT_ID) {
                            let el: web_sys::HtmlInputElement = el.unchecked_into();
                            el.set_value("");
                        }
                    },
                );
                el.add_event_listener_with_callback("compositionend", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }

            el
        }
    };

    if show {
        input.set_value("");
        input.focus().ok();
    } else {
        input.blur().ok();
    }
}

#[cfg(test)]
mod ime_caret_area_tests {
    //! What a field tells the platform about its caret.

    use super::ImeCaretArea;
    use super::test_support::focused_field;
    use crate::TextEditingController as TextFieldController;

    fn caret() -> ImeCaretArea {
        ImeCaretArea {
            x: 10.0,
            y: 20.0,
            width: 1.0,
            height: 16.0,
        }
    }

    #[test]
    fn focusing_enables_platform_input_once() {
        let field = focused_field(TextFieldController::new());
        field.focused.set(false);

        field.set_focused(true);
        assert!(field.ime_enabled.get());

        // Re-focusing an already focused field must not re-issue the platform
        // call, which is why the flag is checked rather than the focus state.
        field.set_focused(true);
        assert!(field.ime_enabled.get());
    }

    #[test]
    fn the_caret_area_is_only_resubmitted_once_it_moves() {
        let field = focused_field(TextFieldController::new());

        assert!(field.ime_cursor_area_moved(caret()));
        field.update_ime_cursor_area(caret());
        assert!(!field.ime_cursor_area_moved(caret()));

        let moved = ImeCaretArea {
            x: caret().x + 4.0,
            ..caret()
        };
        assert!(field.ime_cursor_area_moved(moved));
    }

    #[test]
    fn the_caret_area_is_reported_in_logical_window_coordinates() {
        let field = focused_field(TextFieldController::new());

        field.publish_ime_caret(10.0, 4.0, 20.0, (100.0, 50.0), 2.0);

        assert_eq!(
            field.ime_cursor_area.get(),
            Some(ImeCaretArea {
                x: 55.0,
                y: 27.0,
                width: 1.0,
                height: 10.0,
            })
        );
    }

    #[test]
    fn an_unfocused_field_reports_no_caret_area() {
        let field = focused_field(TextFieldController::new());
        field.focused.set(false);

        field.publish_ime_caret(10.0, 4.0, 20.0, (0.0, 0.0), 1.0);

        assert_eq!(field.ime_cursor_area.get(), None);
    }

    #[test]
    fn the_overlay_follows_the_caret_from_the_canvas_origin() {
        let placed = super::ime_overlay_rect(caret(), (32.0, 64.0));

        assert_eq!(
            placed,
            ImeCaretArea {
                x: 42.0,
                y: 84.0,
                width: 1.0,
                height: 16.0,
            }
        );
    }

    #[test]
    fn the_overlay_never_collapses_to_nothing() {
        let empty = ImeCaretArea {
            x: 5.0,
            y: 6.0,
            width: 0.0,
            height: 0.0,
        };

        let placed = super::ime_overlay_rect(empty, (0.0, 0.0));

        assert_eq!(placed.x, 5.0);
        assert_eq!(placed.y, 6.0);
        assert_eq!(placed.width, super::RawTextField::IME_CARET_WIDTH);
        assert_eq!(placed.height, super::RawTextField::IME_CARET_WIDTH);
    }
}
