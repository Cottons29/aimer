use animation::AnimInstant;
use attribute::size::ResolvedSize;
use std::cell::UnsafeCell;
use widget::base::{BuildContext, Color, Colors};
use widget::style::border::{BoxBorder, BoxOutline};
use widget::text::{FontWeight, TextAlign};
use widget::{Constructor, Drawable, Element, LayoutSpacing, Spacing, TextStyle};

use crate::input_field::controller::TextFieldController;
use events::element::{ElementEvent, KeyAction, NamedKey};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// Write text to the system clipboard.
#[cfg(not(target_arch = "wasm32"))]
fn clipboard_write(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        cb.set_text(text).ok();
    }
}

/// Read text from the system clipboard.
#[cfg(not(target_arch = "wasm32"))]
fn clipboard_read() -> Option<String> {
    arboard::Clipboard::new().ok().and_then(|mut cb| cb.get_text().ok())
}

/// Write text to the browser clipboard (fire-and-forget).
#[cfg(target_arch = "wasm32")]
fn clipboard_write(text: &str) {
    let Some(window) = web_sys::window() else { return };
    let clipboard = window.navigator().clipboard();
    let _ = clipboard.write_text(text);
}

/// Read text from the browser clipboard (synchronous fallback: returns None on wasm
/// because the async Clipboard API cannot be awaited here).
#[cfg(target_arch = "wasm32")]
fn clipboard_read() -> Option<String> {
    // The web Clipboard API is async-only; we read from the hidden input as a fallback.
    let window = web_sys::window()?;
    let document = window.document()?;
    let el = document.get_element_by_id("__aimer_hidden_input")?;
    use wasm_bindgen::JsCast;
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    let val = input.value();
    if val.is_empty() { None } else { Some(val) }
}

/// Inner enum distinguishing sync vs async text-field callbacks.
#[cfg(not(target_arch = "wasm32"))]
enum TextFieldCb {
    Sync(Box<dyn Fn(String)>),
    Async(Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>),
}

#[cfg(target_arch = "wasm32")]
enum TextFieldCb {
    Sync(Box<dyn Fn(String)>),
    Async(Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()>>>>),
}

/// A cloneable, optional callback that receives the current text value.
///
/// Used for `on_changed` (fired after every text mutation) and
/// `on_submitted` (fired when the user presses Enter).
///
/// Supports both synchronous and asynchronous closures.
///
/// # Examples
/// ```rust,ignore
/// // Sync
/// TextField::create_new()
///     .on_changed(|text| println!("changed: {text}"))
///
/// // Async (wrap with AsyncTextFieldCallback)
/// TextField::create_new()
///     .on_changed(AsyncTextFieldCallback(|text| async move {
///         println!("changed: {text}");
///     }))
/// ```
#[derive(Clone)]
pub struct TextFieldCallback(Option<Rc<TextFieldCb>>);

/// Wrapper to convert an async closure that takes a `String` into a
/// `TextFieldCallback`.
///
/// # Examples
/// ```rust,ignore
/// use control::input::AsyncTextFieldCallback;
///
/// TextField::create_new()
///     .on_changed(AsyncTextFieldCallback(|text| async move {
///         println!("async changed: {text}");
///     }))
/// ```
pub struct AsyncTextFieldCallback<F>(pub F);

impl Default for TextFieldCallback {
    fn default() -> Self {
        Self(None)
    }
}

impl TextFieldCallback {
    /// Invoke the callback if one is set.
    pub fn call(&self, text: &str) {
        if let Some(cb) = &self.0 {
            match cb.as_ref() {
                TextFieldCb::Sync(f) => f(text.to_owned()),
                TextFieldCb::Async(f) => {
                    let fut = f(text.to_owned());
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            handle.spawn(fut);
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        wasm_bindgen_futures::spawn_local(fut);
                    }
                }
            }
        }
    }

    /// Returns `true` if a callback is set.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }
}

impl<F> From<F> for TextFieldCallback
where
    F: Fn(String) + 'static,
{
    fn from(f: F) -> Self {
        Self(Some(Rc::new(TextFieldCb::Sync(Box::new(f)))))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut> From<AsyncTextFieldCallback<F>> for TextFieldCallback
where
    F: Fn(String) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn from(ac: AsyncTextFieldCallback<F>) -> Self {
        Self(Some(Rc::new(TextFieldCb::Async(Box::new(move |s| Box::pin(ac.0(s)))))))
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, Fut> From<AsyncTextFieldCallback<F>> for TextFieldCallback
where
    F: Fn(String) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn from(ac: AsyncTextFieldCallback<F>) -> Self {
        Self(Some(Rc::new(TextFieldCb::Async(Box::new(move |s| Box::pin(ac.0(s)))))))
    }
}

impl std::fmt::Debug for TextFieldCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_some() {
            write!(f, "TextFieldCallback(Some(...))")
        } else {
            write!(f, "TextFieldCallback(None)")
        }
    }
}

use canvas::CanvasRendering;

#[cfg(target_os = "ios")]
mod ios_keyboard {
    use std::ffi::{c_char, c_void, CStr};
    use std::sync::OnceLock;

    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

    unsafe extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    type VoidFn = unsafe extern "C" fn();

    static SHOW_FN: OnceLock<Option<VoidFn>> = OnceLock::new();
    static DISMISS_FN: OnceLock<Option<VoidFn>> = OnceLock::new();

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
}

#[cfg(target_os = "android")]
mod android_keyboard {
    pub fn show_keyboard() {
        if let Some(app) = events::android_app::get_android_app() {
            app.show_soft_input(false);
        }
    }

    pub fn dismiss_keyboard() {
        if let Some(app) = events::android_app::get_android_app() {
            app.hide_soft_input(false);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
#[cfg(target_arch = "wasm32")]
type Float = f64;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Text,
    Number,
    Obscure,
}

impl Default for InputType {
    fn default() -> Self {
        Self::Text
    }
}

pub struct Cursor {
    cursor: String,
    offset: UnsafeCell<usize>,
    /// Selection anchor (the end that doesn't move). `None` means no selection.
    selection_anchor: UnsafeCell<Option<usize>>,
    visible: UnsafeCell<bool>,
    blink_rate_ms: u64,
    last_blink: UnsafeCell<AnimInstant>,
    radius: Option<f32>,
    color: Colors,
}

impl Cursor {
    pub fn new(color: Colors) -> Self {
        Self {
            cursor: "|".to_string(),
            offset: UnsafeCell::new(0),
            selection_anchor: UnsafeCell::new(None),
            visible: UnsafeCell::new(true),
            blink_rate_ms: 500,
            last_blink: UnsafeCell::new(AnimInstant::now()),
            radius: None,
            color,
        }
    }

    pub fn offset(&self) -> usize {
        unsafe { *self.offset.get() }
    }

    pub fn set_offset(&self, offset: usize) {
        unsafe {
            *self.offset.get() = offset;
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { *self.visible.get() }
    }

    fn set_visible(&self, v: bool) {
        unsafe {
            *self.visible.get() = v;
        }
    }

    /// Toggle visibility if enough time has elapsed. Returns true if toggled.
    fn update_blink(&self) -> bool {
        let now = AnimInstant::now();
        let last = unsafe { *self.last_blink.get() };
        if now.duration_since(last).as_millis() as u64 >= self.blink_rate_ms {
            unsafe {
                *self.last_blink.get() = now;
            }
            let vis = self.is_visible();
            self.set_visible(!vis);
            true
        } else {
            false
        }
    }

    /// Reset cursor to visible and restart blink timer.
    fn reset_blink(&self) {
        self.set_visible(true);
        unsafe {
            *self.last_blink.get() = AnimInstant::now();
        }
    }

    /// Returns the selection anchor, if any.
    pub fn selection_anchor(&self) -> Option<usize> {
        unsafe { *self.selection_anchor.get() }
    }

    /// Set the selection anchor.
    pub fn set_selection_anchor(&self, anchor: Option<usize>) {
        unsafe {
            *self.selection_anchor.get() = anchor;
        }
    }

    /// Returns the ordered (start, end) of the current selection, or `None`.
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection_anchor().map(|anchor| {
            let offset = self.offset();
            if anchor <= offset {
                (anchor, offset)
            } else {
                (offset, anchor)
            }
        })
    }

    /// Clear the selection without moving the cursor.
    pub fn clear_selection(&self) {
        self.set_selection_anchor(None);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandDirection {
    Horizontal,
    Vertical,
    Both,
    None,
}

impl Default for ExpandDirection {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Constructor)]
pub struct TextFieldStyle {
    #[constructor(default)]
    pub background_color: Colors,
    #[constructor(default)]
    pub border: BoxBorder,
    #[constructor(default)]
    pub outline: BoxOutline,
    #[constructor(default)]
    pub padding: LayoutSpacing,
}

impl Default for TextFieldStyle {
    fn default() -> Self {
        Self {
            background_color: Colors::White,
            border: BoxBorder::default(),
            padding: LayoutSpacing::all(Spacing::Px(4)),
            outline: BoxOutline::default(),
        }
    }
}

pub(crate) struct RawTextField {
    pub input_type: InputType,
    pub controller: TextFieldController,
    pub prompt: String,
    pub hint: String,
    pub hint_style: TextStyle,
    pub text_style: TextStyle,
    pub prompt_style: TextStyle,
    pub text_align: TextAlign,
    pub auto_focus: bool,
    pub max_lines: Option<usize>,
    pub min_lines: Option<usize>,
    pub max_length: Option<usize>,
    pub enable: bool,
    pub expand: ExpandDirection,
    pub cursor: Cursor,
    pub style: TextFieldStyle,
    pub hover_style: Option<TextFieldStyle>,
    pub focus_style: Option<TextFieldStyle>,
    pub disabled_style: Option<TextFieldStyle>,
    pub focused: UnsafeCell<bool>,
    pub hovered: UnsafeCell<bool>,
    pub cached_bounds: UnsafeCell<Option<(f64, f64, f64, f64)>>,
    pub on_changed: TextFieldCallback,
    pub on_submitted: TextFieldCallback,
}

impl RawTextField {
    fn scaled_font_size(&self, style: &TextStyle, scale: Float) -> f32 {
        let fs = if style.font_size == 0 { 14.0 } else { style.font_size as f32 };
        fs * scale as f32
    }

    fn is_focused(&self) -> bool {
        unsafe { *self.focused.get() }
    }

    fn set_focused(&self, focused: bool) {
        unsafe {
            *self.focused.get() = focused;
        }
    }

    fn is_hovered(&self) -> bool {
        unsafe { *self.hovered.get() }
    }

    fn set_hovered(&self, hovered: bool) {
        unsafe {
            *self.hovered.get() = hovered;
        }
    }

    fn active_style(&self) -> &TextFieldStyle {
        if !self.enable {
            if let Some(ref s) = self.disabled_style {
                return s;
            }
        }
        if self.is_focused() {
            if let Some(ref s) = self.focus_style {
                return s;
            }
        }
        if self.is_hovered() {
            if let Some(ref s) = self.hover_style {
                return s;
            }
        }
        &self.style
    }

    fn compute_dimensions(&self, ctx: &BuildContext) -> (Float, Float) {
        let constraint = ctx.box_constraint;

        (constraint.max_width, constraint.max_height)
    }

    fn outline_strokes(&self, box_width: Float, box_height: Float, scale: Float) -> (Float, Float, Float, Float) {
        self.active_style()
            .outline
            .strokes(box_width, box_height, scale)
    }

    fn cursor_x_offset_canvas(&self, canvas: &canvas::Canvas, font_size: f32) -> Float {
        let text = self.controller.text();
        let offset = self.cursor.offset();
        let prefix: String = text.chars().take(offset).collect();
        canvas.measure_text(&prefix, font_size) as Float
    }
}

/// On wasm32 / mobile browsers, focusing a hidden `<input>` element inside a
/// user-gesture handler is the only reliable way to raise the virtual keyboard.
///
/// Event listeners on the hidden input re-dispatch `keydown` and `input` events
/// to the winit canvas (`#aimer_app`) so that the framework's normal keyboard
/// pipeline (`WindowEvent::KeyboardInput`) still fires.
#[cfg(target_arch = "wasm32")]
fn wasm_request_keyboard(show: bool) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };

    let input: web_sys::HtmlInputElement = match document.get_element_by_id("__aimer_hidden_input") {
        Some(el) => el.unchecked_into(),
        None => {
            let el = document
                .create_element("input")
                .expect("failed to create hidden input")
                .unchecked_into::<web_sys::HtmlInputElement>();
            el.set_id("__aimer_hidden_input");
            el.set_type("text");
            el.set_attribute("autocapitalize", "off").ok();
            el.set_attribute("autocomplete", "off").ok();
            el.set_attribute("autocorrect", "off").ok();
            el.set_attribute("spellcheck", "false").ok();
            let style = el.style();
            style.set_property("position", "fixed").ok();
            style.set_property("opacity", "0").ok();
            style.set_property("left", "-9999px").ok();
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
                let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |evt: web_sys::KeyboardEvent| {
                    evt.stop_propagation();
                    evt.prevent_default();
                    let Some(w) = web_sys::window() else { return };
                    let Some(doc) = w.document() else { return };
                    let Some(canvas) = doc.get_element_by_id("aimer_app") else { return };
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
                });
                el.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref()).ok();
                cb.forget();
            }

            // Forward keyup events as well.
            {
                let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |evt: web_sys::KeyboardEvent| {
                    evt.stop_propagation();
                    evt.prevent_default();
                    let Some(w) = web_sys::window() else { return };
                    let Some(doc) = w.document() else { return };
                    let Some(canvas) = doc.get_element_by_id("aimer_app") else { return };
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
                });
                el.add_event_listener_with_callback("keyup", cb.as_ref().unchecked_ref()).ok();
                cb.forget();
            }

            // Handle compositionless text input (e.g. mobile virtual keyboards)
            // that may not fire keydown for each character.
            {
                let cb = Closure::<dyn FnMut(web_sys::InputEvent)>::new(move |evt: web_sys::InputEvent| {
                    if evt.is_composing() {
                        return;
                    }
                    let Some(data) = evt.data() else { return };
                    let Some(w) = web_sys::window() else { return };
                    let Some(doc) = w.document() else { return };
                    let Some(canvas) = doc.get_element_by_id("aimer_app") else { return };
                    // Synthesize a keydown + keyup pair for each character so
                    // winit can translate them into KeyboardInput events.
                    let chars: Vec<char> = data.chars().collect();
                    for ch in chars {
                        let key = ch.to_string();
                        for event_type in &["keydown", "keyup"] {
                            let synth = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
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
                    if let Some(el) = doc.get_element_by_id("__aimer_hidden_input") {
                        let el: web_sys::HtmlInputElement = el.unchecked_into();
                        el.set_value("");
                    }
                });
                el.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref()).ok();
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

impl Element for RawTextField {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if !self.enable {
            return false;
        }


        // debug!("RawTextField on_event: {:?}", event);

        match event {
            ElementEvent::PointerDown(pos) => {
                let is_inside = unsafe {
                    if let Some((left, top, right, bottom)) = *self.cached_bounds.get() {
                        pos.x as f64 >= left && pos.x as f64 <= right && pos.y as f64 >= top && pos.y as f64 <= bottom
                    } else {
                        false
                    }
                };

                if is_inside {
                    self.set_focused(true);
                    self.cursor.set_offset(self.controller.char_count());
                    self.cursor.reset_blink();
                    #[cfg(target_os = "ios")]
                    ios_keyboard::show_keyboard();
                    #[cfg(target_os = "android")]
                    android_keyboard::show_keyboard();
                    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                    if let Some(w) = events::window::get_window() {
                        w.set_ime_allowed(true);
                    }
                    #[cfg(target_arch = "wasm32")]
                    wasm_request_keyboard(true);
                    return true;
                } else {
                    self.set_focused(false);
                    #[cfg(target_os = "ios")]
                    ios_keyboard::dismiss_keyboard();
                    #[cfg(target_os = "android")]
                    android_keyboard::dismiss_keyboard();
                    #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
                    if let Some(w) = events::window::get_window() {
                        w.set_ime_allowed(false);
                    }
                    #[cfg(target_arch = "wasm32")]
                    wasm_request_keyboard(false);
                    return false;
                }
            }
            ElementEvent::CharInput { ch, action, modifiers } => {
                if !self.is_focused() {
                    return false;
                }
                if *action == KeyAction::Released {
                    return false;
                }

                // If there is a selection, delete it first
                if let Some((start, end)) = self.cursor.selection_range() {
                    self.controller.delete_range(start, end);
                    self.cursor.set_offset(start);
                    self.cursor.clear_selection();
                }

                let offset = self.cursor.offset();
                unsafe {
                    self.controller.insert_char(*ch, offset);
                }
                self.cursor.set_offset(offset + 1);
                self.cursor.reset_blink();
                self.on_changed.call(&self.controller.text());
                true
            }
            ElementEvent::KeyInput { key, action, modifiers } => {
                if !self.is_focused() {
                    return false;
                }
                if *action == KeyAction::Released {
                    return false;
                }

                let is_shortcut = modifiers.ctrl || modifiers.meta;

                // Handle Ctrl/Cmd shortcuts
                if is_shortcut {
                    let result = match key {
                        NamedKey::Other(k) if k == "a" => {
                            // Select all
                            self.cursor.set_selection_anchor(Some(0));
                            self.cursor.set_offset(self.controller.char_count());
                            true
                        }
                        NamedKey::Other(k) if k == "c" => {
                            // Copy
                            if let Some((start, end)) = self.cursor.selection_range() {
                                let selected = self.controller.get_range(start, end);
                                clipboard_write(&selected);
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "x" => {
                            // Cut
                            if let Some((start, end)) = self.cursor.selection_range() {
                                let selected = self.controller.delete_range(start, end);
                                clipboard_write(&selected);
                                self.cursor.set_offset(start);
                                self.cursor.clear_selection();
                                self.on_changed.call(&self.controller.text());
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "v" => {
                            // Paste
                            if let Some(text) = clipboard_read() {
                                // Delete selection first if any
                                if let Some((start, end)) = self.cursor.selection_range() {
                                    self.controller.delete_range(start, end);
                                    self.cursor.set_offset(start);
                                    self.cursor.clear_selection();
                                }
                                let offset = self.cursor.offset();
                                let char_count = text.chars().count();
                                self.controller.insert_str(&text, offset);
                                self.cursor.set_offset(offset + char_count);
                                self.on_changed.call(&self.controller.text());
                            }
                            true
                        }
                        _ => false,
                    };
                    if result {
                        self.cursor.reset_blink();
                        return true;
                    }
                }

                let result = match key {
                    NamedKey::Backspace => {
                        if let Some((start, end)) = self.cursor.selection_range() {
                            self.controller.delete_range(start, end);
                            self.cursor.set_offset(start);
                            self.cursor.clear_selection();
                            self.on_changed.call(&self.controller.text());
                        } else {
                            let offset = self.cursor.offset();
                            if offset > 0 {
                                self.controller.delete_char(offset - 1);
                                self.cursor.set_offset(offset - 1);
                                self.on_changed.call(&self.controller.text());
                            }
                        }
                        true
                    }
                    NamedKey::Delete => {
                        if let Some((start, end)) = self.cursor.selection_range() {
                            self.controller.delete_range(start, end);
                            self.cursor.set_offset(start);
                            self.cursor.clear_selection();
                            self.on_changed.call(&self.controller.text());
                        } else {
                            let offset = self.cursor.offset();
                            if offset < self.controller.char_count() {
                                self.controller.delete_char(offset);
                                self.on_changed.call(&self.controller.text());
                            }
                        }
                        true
                    }
                    NamedKey::Enter => {
                        self.cursor.clear_selection();
                        self.on_submitted.call(&self.controller.text());
                        true
                    }
                    NamedKey::ArrowLeft => {
                        let offset = self.cursor.offset();
                        if modifiers.shift {
                            if self.cursor.selection_anchor().is_none() {
                                self.cursor.set_selection_anchor(Some(offset));
                            }
                            if offset > 0 {
                                self.cursor.set_offset(offset - 1);
                            }
                        } else {
                            if let Some((start, _end)) = self.cursor.selection_range() {
                                self.cursor.set_offset(start);
                            } else if offset > 0 {
                                self.cursor.set_offset(offset - 1);
                            }
                            self.cursor.clear_selection();
                        }
                        true
                    }
                    NamedKey::ArrowRight => {
                        let offset = self.cursor.offset();
                        let len = self.controller.char_count();
                        if modifiers.shift {
                            if self.cursor.selection_anchor().is_none() {
                                self.cursor.set_selection_anchor(Some(offset));
                            }
                            if offset < len {
                                self.cursor.set_offset(offset + 1);
                            }
                        } else {
                            if let Some((_start, end)) = self.cursor.selection_range() {
                                self.cursor.set_offset(end);
                            } else if offset < len {
                                self.cursor.set_offset(offset + 1);
                            }
                            self.cursor.clear_selection();
                        }
                        true
                    }
                    NamedKey::Home => {
                        if modifiers.shift {
                            let offset = self.cursor.offset();
                            if self.cursor.selection_anchor().is_none() {
                                self.cursor.set_selection_anchor(Some(offset));
                            }
                        } else {
                            self.cursor.clear_selection();
                        }
                        self.cursor.set_offset(0);
                        true
                    }
                    NamedKey::End => {
                        if modifiers.shift {
                            let offset = self.cursor.offset();
                            if self.cursor.selection_anchor().is_none() {
                                self.cursor.set_selection_anchor(Some(offset));
                            }
                        } else {
                            self.cursor.clear_selection();
                        }
                        self.cursor.set_offset(self.controller.char_count());
                        true
                    }
                    NamedKey::Escape => {
                        self.cursor.clear_selection();
                        self.set_focused(false);
                        #[cfg(target_os = "ios")]
                        ios_keyboard::dismiss_keyboard();
                        #[cfg(target_os = "android")]
                        android_keyboard::dismiss_keyboard();
                        true
                    }
                    _ => false,
                };
                if result {
                    self.cursor.reset_blink();
                }
                result
            }
            ElementEvent::PointerMove(pos) => {
                let is_inside = unsafe {
                    if let Some((left, top, right, bottom)) = *self.cached_bounds.get() {
                        pos.x as f64 >= left && pos.x as f64 <= right && pos.y as f64 >= top && pos.y as f64 <= bottom
                    } else {
                        false
                    }
                };
                let was_hovered = self.is_hovered();
                self.set_hovered(is_inside);
                was_hovered != is_inside
            }
            ElementEvent::Cancel => {
                self.set_focused(false);
                #[cfg(target_os = "ios")]
                ios_keyboard::dismiss_keyboard();
                #[cfg(target_os = "android")]
                android_keyboard::dismiss_keyboard();
                true
            }
            _ => false,
        }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let (w, h) = self.compute_dimensions(ctx);
        let scale = ctx.scale;
        let (ol, ot, or, ob) = self.outline_strokes(w, h, scale);
        ResolvedSize { width: w + ol + or, height: h + ot + ob }
    }
}

impl Drawable for RawTextField {
    fn draw(&self, ctx: &BuildContext) {
        ctx.canvas.save();

        let (box_width, box_height) = self.compute_dimensions(ctx);
        let scale = ctx.scale;

        // Translate inward by outline strokes so the outline has room to draw
        let (ol, ot, _or, _ob) = self.outline_strokes(box_width, box_height, scale);
        ctx.canvas.translate((ol, ot).into());

        // Cache absolute bounds for hit-testing
        let (abs_x, abs_y) = {
            let (tx, ty) = ctx.canvas.get_transform_translation();
            (tx as f64, ty as f64)
        };
        unsafe {
            *self.cached_bounds.get() = Some((abs_x, abs_y, abs_x + box_width as f64, abs_y + box_height as f64));
        }

        // --- Resolve active style ---
        let style = self.active_style();

        // --- Draw background ---
        let bg_color: color::prelude::Color = style.background_color.into();
        let radius = style.border.get_uniform_radius(box_width, box_height, scale).unwrap_or(0.0);
        ctx.canvas.fill_color_rect(
            (0.0, 0.0).into(),
            ResolvedSize { width: box_width, height: box_height },
            bg_color,
            radius as f32,
        );

        // --- Draw border ---
        style.border.draw(ctx);
        style.outline.draw(ctx);

        // --- Padding ---
        let pad_top = style.padding.top.value(box_height, scale);
        let pad_bottom = style.padding.bottom.value(box_height, scale);
        let pad_left = style.padding.left.value(box_width, scale);
        let pad_right = style.padding.right.value(box_width, scale);

        ctx.canvas.save();
        ctx.canvas.set_clip(
            (pad_left, pad_top).into(),
            ResolvedSize {
                width: (box_width - pad_left - pad_right).max(0.0),
                height: (box_height - pad_top - pad_bottom).max(0.0),
            },
        );
        ctx.canvas.translate((pad_left, pad_top).into());

        let content_height = (box_height - pad_top - pad_bottom).max(0.0);

        let text = self.controller.text();
        let is_empty = text.is_empty();

        let font_size = self.scaled_font_size(&self.text_style, scale);
        // Approximate text_y for vertical centering (baseline offset)
        let text_y = (content_height as f32 - font_size) / 2.0;

        if is_empty {
            // --- Draw prompt (visible when field is empty) ---
            if !self.prompt.is_empty() {
                let prompt_fs = self.scaled_font_size(&self.prompt_style, scale);
                let prompt_color: color::prelude::Color = self.prompt_style.color.into();
                ctx.canvas.draw_text(
                    &self.prompt,
                    (0.0, text_y as Float).into(),
                    prompt_fs,
                    prompt_color,
                );
            }

            // --- Draw hint text (visible when field is empty and no prompt) ---
            if self.prompt.is_empty() && !self.hint.is_empty() {
                let hint_fs = self.scaled_font_size(&self.hint_style, scale);
                let hint_color: color::prelude::Color = self.hint_style.color.into();
                ctx.canvas.draw_text(
                    &self.hint,
                    (0.0, text_y as Float).into(),
                    hint_fs,
                    hint_color,
                );
            }
        } else {
            // --- Draw text ---
            let text_x: Float = 0.0;

            let display = match self.input_type {
                InputType::Obscure => "\u{2022}".repeat(self.controller.char_count()),
                _ => text.to_string(),
            };

            if !display.is_empty() {
                let text_color: color::prelude::Color = self.text_style.color.into();
                ctx.canvas.draw_text(
                    &display,
                    (text_x, text_y as Float).into(),
                    font_size,
                    text_color,
                );
            }

            // --- Draw cursor ---
            if self.is_focused() && self.cursor.is_visible() {
                let cursor_x = text_x + self.cursor_x_offset_canvas(&ctx.canvas, font_size);
                let cursor_top = content_height * 0.15;
                let cursor_bottom = content_height * 0.85;
                let cursor_height = cursor_bottom - cursor_top;
                let cursor_color: color::prelude::Color = self.cursor.color.into();
                let stroke_w = 1.5 * scale;

                ctx.canvas.fill_color_rect(
                    (cursor_x, cursor_top).into(),
                    ResolvedSize { width: stroke_w, height: cursor_height },
                    cursor_color,
                    0.0,
                );
            }
        }

        // --- Draw cursor when field is empty but focused ---
        if is_empty && self.is_focused() && self.cursor.is_visible() {
            let cursor_x: Float = 0.0;
            let cursor_top = content_height * 0.15;
            let cursor_bottom = content_height * 0.85;
            let cursor_height = cursor_bottom - cursor_top;
            let cursor_color: color::prelude::Color = self.cursor.color.into();
            let stroke_w = 1.5 * scale;

            ctx.canvas.fill_color_rect(
                (cursor_x, cursor_top).into(),
                ResolvedSize { width: stroke_w, height: cursor_height },
                cursor_color,
                0.0,
            );
        }

        ctx.canvas.clear_clip();
        ctx.canvas.restore(); // clip + translate
        ctx.canvas.restore(); // outer save

        // Drive cursor blink: toggle visibility and schedule next redraw while focused
        if self.is_focused() {
            self.cursor.update_blink();
            ctx.window.request_redraw();
        }
    }
}
