use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use aimer_animation::AnimInstant;
use aimer_attribute::CacheBounds;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_events::window::get_window;
use aimer_macro::Rebuildable;
use aimer_style::{BoxDecoration, LayoutSpacing, TextAlign, TextStyle};
use aimer_text::RawTextWidget;
use aimer_widget::base::{BuildContext, Color, Colors};
use aimer_widget::{Drawable, Element, EventElement, LayoutCache, LayoutElement, VisitorElement};

use crate::input_field::controller::TextFieldController;

/// Write text to the system clipboard.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn clipboard_write(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        cb.set_text(text).ok();
    }
}

/// Read text from the system clipboard.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
fn clipboard_read() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
}

/// Clipboard stub for Android (not yet supported).
#[cfg(target_os = "android")]
fn clipboard_write(_text: &str) {}

#[cfg(target_os = "android")]
fn clipboard_read() -> Option<String> {
    None
}

/// Write text to the browser clipboard (fire-and-forget).
#[cfg(target_arch = "wasm32")]
fn clipboard_write(text: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let clipboard = window.navigator().clipboard();
    let _ = clipboard.write_text(text);
}

/// Read text from the browser clipboard (synchronous fallback: returns None on
/// wasm because the async Clipboard API cannot be awaited here).
#[cfg(target_arch = "wasm32")]
fn clipboard_read() -> Option<String> {
    // The web Clipboard API is async-only; we read from the hidden input as a
    // fallback.
    let window = web_sys::window()?;
    let document = window.document()?;
    let el = document.get_element_by_id("__aimer_hidden_input")?;
    use wasm_bindgen::JsCast;
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    let val = input.value();
    if val.is_empty() { None } else { Some(val) }
}
type BoxedTextFieldFuture = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Inner enum distinguishing sync vs async text-field callbacks.
#[cfg(not(target_arch = "wasm32"))]
enum TextFieldCb {
    Sync(Box<dyn Fn(String)>),
    Async(BoxedTextFieldFuture),
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
#[derive(Clone, Default)]
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
#[derive(Default)]
pub struct AsyncTextFieldCallback<F>(pub F);

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
        Self(Some(Rc::new(TextFieldCb::Async(Box::new(move |s| {
            Box::pin(ac.0(s))
        })))))
    }
}

#[cfg(target_arch = "wasm32")]
impl<F, Fut> From<AsyncTextFieldCallback<F>> for TextFieldCallback
where
    F: Fn(String) -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn from(ac: AsyncTextFieldCallback<F>) -> Self {
        Self(Some(Rc::new(TextFieldCb::Async(Box::new(move |s| {
            Box::pin(ac.0(s))
        })))))
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

#[cfg(target_os = "ios")]
mod ios_keyboard {
    use std::ffi::{CStr, c_char, c_void};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputType {
    #[default]
    Text,
    Number,
    Obscure,
}

#[allow(dead_code)]
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
        if now
            .duration_since(last)
            .as_millis() as u64
            >= self.blink_rate_ms
        {
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
        self.selection_anchor()
            .map(|anchor| {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandDirection {
    Horizontal,
    Vertical,
    Both,
    #[default]
    None,
}
#[allow(dead_code)]
#[derive(Rebuildable)]
pub(crate) struct RawTextField {
    pub input_type: InputType,
    pub controller: TextFieldController,
    pub prompt: Arc<str>,
    pub hint: Arc<str>,
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
    pub decoration: BoxDecoration,
    pub hover_decoration: Option<BoxDecoration>,
    pub focus_decoration: Option<BoxDecoration>,
    pub disabled_decoration: Option<BoxDecoration>,
    pub selection_color: Color,
    pub focused: Cell<bool>,
    pub hovered: Cell<bool>,
    pub cached_bounds: CacheBounds,
    pub on_changed: TextFieldCallback,
    pub on_submitted: TextFieldCallback,
    pub on_focus: TextFieldCallback,
    pub on_blur: TextFieldCallback,
    pub read_only: bool,
    pub mouse_held: Cell<bool>,
    pub last_click_time: Cell<AnimInstant>,
    pub click_count: Cell<u8>,
    pub pending_click: Cell<Option<Vec2d>>,
    pub scroll_x: Cell<f32>,
    pub preedit_text: Cell<String>,
    pub preedit_cursor: Cell<Option<(usize, usize)>>,
    pub blink_scheduled: Cell<bool>,
    pub padding: LayoutSpacing,
}

impl RawTextField {
    fn scaled_font_size(&self, style: &TextStyle, scale: f32) -> f32 {
        let fs = if style.font_size == 0 {
            14.0
        } else {
            style.font_size as f32
        };
        fs * scale
    }

    fn is_focused(&self) -> bool {
        self.focused.get()
    }

    fn set_focused(&self, focused: bool) {
        self.focused.set(focused);
        if !focused {
            self.blink_scheduled
                .set(false);
        }
    }

    fn is_hovered(&self) -> bool {
        self.hovered.get()
    }

    fn set_hovered(&self, hovered: bool) {
        self.hovered.set(hovered);
    }

    fn active_decoration(&self) -> &BoxDecoration {
        if let Some(ref s) = self.disabled_decoration
            && !self.enable
        {
            return s;
        }

        if let Some(ref s) = self.focus_decoration
            && self.is_focused()
        {
            return s;
        }

        if let Some(ref s) = self.hover_decoration
            && self.is_hovered()
        {
            return s;
        }

        &self.decoration
    }

    fn compute_dimensions(&self, ctx: &BuildContext) -> (f32, f32) {
        let constraint = ctx.box_constraint;

        (constraint.max_width, constraint.max_height)
    }

    fn outline_strokes(&self, box_width: f32, box_height: f32, scale: f32) -> (f32, f32, f32, f32) {
        self.active_decoration()
            .outline
            .strokes(box_width, box_height, scale)
    }

    fn cursor_x_offset_canvas(&self, canvas: &aimer_canvas::Canvas, font_size: f32) -> f32 {
        let text = self.controller.text();
        let offset = self.cursor.offset();
        let prefix: String = unicode_segmentation::UnicodeSegmentation::graphemes(text, true)
            .take(offset)
            .collect();
        canvas.measure_text(&prefix, font_size)
    }

    /// Measure text width up to a given grapheme offset.
    fn text_width_to_offset(
        &self,
        text: &str,
        offset: usize,
        canvas: &aimer_canvas::Canvas,
        font_size: f32,
    ) -> f32 {
        let prefix: String = unicode_segmentation::UnicodeSegmentation::graphemes(text, true)
            .take(offset)
            .collect();
        canvas.measure_text(&prefix, font_size)
    }

    fn align_x(&self, text_width: f32, content_width: f32) -> f32 {
        match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                (content_width - text_width) / 2.0
            }
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => {
                content_width - text_width
            }
        }
    }

    fn build_text_widget(
        &'_ self,
        text: &str,
        style: &TextStyle,
        align: TextAlign,
    ) -> RawTextWidget {
        RawTextWidget {
            text: text.into(),
            text_style: *style,
            text_align: align,
            cache: LayoutCache::new(),
            _typeface: std::sync::Mutex::new(None),
        }
    }

    // ── Word / line selection helpers ────────────────────────────────

    /// Select the word at the given grapheme offset using Unicode word
    /// boundaries.
    fn select_word_at(&self, grapheme_offset: usize) {
        use unicode_segmentation::UnicodeSegmentation;
        let text = self.controller.text();
        if text.is_empty() {
            return;
        }

        // Convert grapheme offset to byte offset
        let byte_offset: usize = text
            .chars()
            .take(grapheme_offset)
            .map(|c| c.len_utf8())
            .sum();

        // Find word boundaries
        let word_bounds: Vec<(usize, &str)> = text
            .split_word_bound_indices()
            .collect();

        for &(start, segment) in &word_bounds {
            let end = start + segment.len();
            if byte_offset >= start && byte_offset < end {
                let grapheme_start = text[..start].chars().count();
                let grapheme_end = text[..end].chars().count();
                self.cursor
                    .set_selection_anchor(Some(grapheme_start));
                self.cursor
                    .set_offset(grapheme_end);
                return;
            }
        }
    }

    /// Select the line (between newline characters) containing the given
    /// grapheme offset.
    fn select_line_at(&self, grapheme_offset: usize) {
        let text = self.controller.text();
        if text.is_empty() {
            return;
        }

        let chars: Vec<char> = text.chars().collect();
        let mut line_start = grapheme_offset;
        let mut line_end = grapheme_offset;

        while line_start > 0 && chars[line_start - 1] != '\n' {
            line_start -= 1;
        }
        while line_end < chars.len() && chars[line_end] != '\n' {
            line_end += 1;
        }

        self.cursor
            .set_selection_anchor(Some(line_start));
        self.cursor
            .set_offset(line_end);
    }

    /// Adjust `scroll_x` so the cursor is visible within `content_width`.
    fn ensure_cursor_visible(
        &self,
        content_width: f32,
        canvas: &aimer_canvas::Canvas,
        font_size: f32,
    ) {
        let cursor_x = self.cursor_x_offset_canvas(canvas, font_size);
        let scroll = self.scroll_x.get();

        if cursor_x < scroll {
            self.scroll_x
                .set(cursor_x.max(0.0));
        } else if cursor_x > scroll + content_width {
            self.scroll_x
                .set((cursor_x - content_width).max(0.0));
        }
    }

    /// Count the number of lines in the text (newlines + 1).
    fn line_count(&self) -> usize {
        self.controller
            .text()
            .chars()
            .filter(|&c| c == '\n')
            .count()
            + 1
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
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };

    let input: web_sys::HtmlInputElement = match document.get_element_by_id("__aimer_hidden_input")
    {
        Some(el) => el.unchecked_into(),
        None => {
            let el = document
                .create_element("input")
                .expect("failed to create hidden input")
                .unchecked_into::<web_sys::HtmlInputElement>();
            el.set_id("__aimer_hidden_input");
            el.set_type("text");
            el.set_attribute("autocapitalize", "off")
                .ok();
            el.set_attribute("autocomplete", "off")
                .ok();
            el.set_attribute("autocorrect", "off")
                .ok();
            el.set_attribute("spellcheck", "false")
                .ok();
            let style = el.style();
            style
                .set_property("position", "fixed")
                .ok();
            style
                .set_property("opacity", "0")
                .ok();
            style
                .set_property("left", "-9999px")
                .ok();
            style
                .set_property("top", "0")
                .ok();
            style
                .set_property("width", "1px")
                .ok();
            style
                .set_property("height", "1px")
                .ok();
            style
                .set_property("border", "none")
                .ok();
            style
                .set_property("outline", "none")
                .ok();
            style
                .set_property("padding", "0")
                .ok();
            style
                .set_property("font-size", "16px")
                .ok(); // prevents iOS zoom
            document
                .body()
                .unwrap()
                .append_child(&el)
                .ok();

            // Forward keydown events to the winit canvas so the framework
            // receives them through its normal WindowEvent::KeyboardInput path.
            {
                let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                    move |evt: web_sys::KeyboardEvent| {
                        evt.stop_propagation();
                        evt.prevent_default();
                        let Some(w) = web_sys::window() else { return };
                        let Some(doc) = w.document() else { return };
                        let Some(canvas) = doc.get_element_by_id("aimer_app") else {
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
                        canvas
                            .dispatch_event(&new_evt)
                            .ok();
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
                        let Some(canvas) = doc.get_element_by_id("aimer_app") else {
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
                        canvas
                            .dispatch_event(&new_evt)
                            .ok();
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
                        let Some(canvas) = doc.get_element_by_id("aimer_app") else {
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
                                canvas
                                    .dispatch_event(&synth)
                                    .ok();
                            }
                        }
                        // Clear the hidden input so subsequent input events keep working.
                        if let Some(el) = doc.get_element_by_id("__aimer_hidden_input") {
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
                        let Some(canvas) = doc.get_element_by_id("aimer_app") else {
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
                                canvas
                                    .dispatch_event(&synth)
                                    .ok();
                            }
                        }
                        // Clear the hidden input so the next composition starts clean.
                        if let Some(el) = doc.get_element_by_id("__aimer_hidden_input") {
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

impl VisitorElement for RawTextField {
    fn debug_name(&self) -> &'static str {
        "TextField"
    }
}

impl EventElement for RawTextField {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if !self.enable {
            return false;
        }

        // debug!("RawTextField on_event: {:?}", event);

        match event {
            ElementEvent::PointerDown(pos, _, _) => {
                let is_inside = self
                    .cached_bounds
                    .is_inside(pos.x, pos.y);

                if is_inside {
                    let was_focused = self.is_focused();
                    self.set_focused(true);
                    self.mouse_held.set(true);
                    self.cursor.clear_selection();

                    // Double/triple-click detection
                    let now = AnimInstant::now();
                    let elapsed = now.duration_since(self.last_click_time.get());
                    let prev_count = self.click_count.get();
                    let new_count = if elapsed.as_millis() < 500 {
                        prev_count + 1
                    } else {
                        1
                    };
                    self.click_count
                        .set(new_count);
                    self.last_click_time.set(now);

                    // Defer cursor placement to draw() where canvas is available
                    self.pending_click
                        .set(Some(*pos));
                    self.cursor.reset_blink();

                    if !was_focused {
                        self.on_focus
                            .call(self.controller.text());
                    }

                    // Clear IME preedit on new click
                    self.preedit_text
                        .set(String::new());
                    self.preedit_cursor.set(None);

                    #[cfg(target_os = "ios")]
                    ios_keyboard::show_keyboard();
                    #[cfg(target_os = "android")]
                    android_keyboard::show_keyboard();
                    #[cfg(not(any(
                        target_os = "ios",
                        target_os = "android",
                        target_arch = "wasm32"
                    )))]
                    if let Some(w) = get_window() {
                        w.set_ime_allowed(true);
                        if let Some((start, end)) = self
                            .cached_bounds
                            .pos_start_end()
                        {
                            use winit::dpi::{LogicalPosition, LogicalSize};
                            let pos = LogicalPosition::new(start.x as f64, start.y as f64);
                            let size = LogicalSize::new(
                                (end.x - start.x).max(1.0) as f64,
                                (end.y - start.y).max(1.0) as f64,
                            );
                            w.set_ime_cursor_area(pos, size);
                        }
                    }
                    #[cfg(target_arch = "wasm32")]
                    wasm_request_keyboard(true);
                    true
                } else {
                    self.set_focused(false);
                    self.mouse_held.set(false);
                    self.on_blur
                        .call(self.controller.text());
                    #[cfg(target_os = "ios")]
                    ios_keyboard::dismiss_keyboard();
                    #[cfg(target_os = "android")]
                    android_keyboard::dismiss_keyboard();
                    if let Some(w) = get_window() {
                        w.set_ime_allowed(false);
                    }
                    #[cfg(target_arch = "wasm32")]
                    wasm_request_keyboard(false);
                    false
                }
            }
            ElementEvent::CharInput { ch, action, .. } => {
                if !self.is_focused() || self.read_only {
                    return false;
                }
                if *action == KeyAction::Released {
                    return false;
                }

                // Enforce max_length: reject if at or over the limit
                if let Some(max) = self.max_length {
                    // If there's a selection, the deleted chars free up space
                    let selected_len = self
                        .cursor
                        .selection_range()
                        .map(|(s, e)| e - s)
                        .unwrap_or(0);
                    if self
                        .controller
                        .char_count()
                        .saturating_sub(selected_len)
                        >= max
                    {
                        return false;
                    }
                }

                // If there is a selection, delete it first
                if let Some((start, end)) = self.cursor.selection_range() {
                    self.controller
                        .delete_range(start, end);
                    self.cursor.set_offset(start);
                    self.cursor.clear_selection();
                }

                let offset = self.cursor.offset();
                unsafe {
                    self.controller
                        .insert_char(*ch, offset);
                }
                self.cursor
                    .set_offset(offset + 1);
                self.cursor.reset_blink();
                self.on_changed
                    .call(self.controller.text());
                true
            }
            ElementEvent::KeyInput {
                key,
                action,
                modifiers,
            } => {
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
                            self.cursor
                                .set_selection_anchor(Some(0));
                            self.cursor
                                .set_offset(self.controller.char_count());
                            true
                        }
                        NamedKey::Other(k) if k == "c" => {
                            // Copy
                            if let Some((start, end)) = self.cursor.selection_range() {
                                let selected = self
                                    .controller
                                    .get_range(start, end);
                                clipboard_write(&selected);
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "x" && !self.read_only => {
                            // Cut
                            if let Some((start, end)) = self.cursor.selection_range() {
                                let selected = self
                                    .controller
                                    .delete_range(start, end);
                                clipboard_write(&selected);
                                self.cursor.set_offset(start);
                                self.cursor.clear_selection();
                                self.on_changed
                                    .call(self.controller.text());
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "v" && !self.read_only => {
                            // Paste
                            if let Some(text) = clipboard_read() {
                                // Delete selection first if any
                                if let Some((start, end)) = self.cursor.selection_range() {
                                    self.controller
                                        .delete_range(start, end);
                                    self.cursor.set_offset(start);
                                    self.cursor.clear_selection();
                                }
                                let offset = self.cursor.offset();
                                let char_count = text.chars().count();
                                self.controller
                                    .insert_str(&text, offset);
                                self.cursor
                                    .set_offset(offset + char_count);
                                self.on_changed
                                    .call(self.controller.text());
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "z" && !modifiers.shift && !self.read_only => {
                            // Undo
                            if self.controller.undo() {
                                let len = self.controller.char_count();
                                let off = self.cursor.offset();
                                if off > len {
                                    self.cursor.set_offset(len);
                                }
                                self.on_changed
                                    .call(self.controller.text());
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "z" && modifiers.shift && !self.read_only => {
                            // Redo (Ctrl+Shift+Z)
                            if self.controller.redo() {
                                let len = self.controller.char_count();
                                let off = self.cursor.offset();
                                if off > len {
                                    self.cursor.set_offset(len);
                                }
                                self.on_changed
                                    .call(self.controller.text());
                            }
                            true
                        }
                        NamedKey::Other(k) if k == "y" && !self.read_only => {
                            // Redo (Ctrl+Y — Windows convention)
                            if self.controller.redo() {
                                let len = self.controller.char_count();
                                let off = self.cursor.offset();
                                if off > len {
                                    self.cursor.set_offset(len);
                                }
                                self.on_changed
                                    .call(self.controller.text());
                            }
                            true
                        }
                        NamedKey::Enter => {
                            // Ctrl+Enter / Cmd+Enter: submit even in multi-line mode
                            self.cursor.clear_selection();
                            self.on_submitted
                                .call(self.controller.text());
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
                    NamedKey::Backspace if !self.read_only => {
                        if let Some((start, end)) = self.cursor.selection_range() {
                            self.controller
                                .delete_range(start, end);
                            self.cursor.set_offset(start);
                            self.cursor.clear_selection();
                            self.on_changed
                                .call(self.controller.text());
                        } else {
                            let offset = self.cursor.offset();
                            if offset > 0 {
                                self.controller
                                    .delete_char(offset - 1);
                                self.cursor
                                    .set_offset(offset - 1);
                                self.on_changed
                                    .call(self.controller.text());
                            }
                        }
                        true
                    }
                    NamedKey::Delete if !self.read_only => {
                        if let Some((start, end)) = self.cursor.selection_range() {
                            self.controller
                                .delete_range(start, end);
                            self.cursor.set_offset(start);
                            self.cursor.clear_selection();
                            self.on_changed
                                .call(self.controller.text());
                        } else {
                            let offset = self.cursor.offset();
                            if offset < self.controller.char_count() {
                                self.controller
                                    .delete_char(offset);
                                self.on_changed
                                    .call(self.controller.text());
                            }
                        }
                        true
                    }
                    NamedKey::Enter
                        if !self.read_only
                            && self
                                .max_lines
                                .is_some_and(|max| max > 1) =>
                    {
                        // Multi-line mode: Enter inserts newline
                        if let Some(max) = self.max_lines
                            && self.line_count() >= max
                        {
                            return true;
                        }
                        // Delete selection first
                        if let Some((start, end)) = self.cursor.selection_range() {
                            self.controller
                                .delete_range(start, end);
                            self.cursor.set_offset(start);
                            self.cursor.clear_selection();
                        }
                        let offset = self.cursor.offset();
                        unsafe {
                            self.controller
                                .insert_char('\n', offset);
                        }
                        self.cursor
                            .set_offset(offset + 1);
                        self.on_changed
                            .call(self.controller.text());
                        true
                    }
                    NamedKey::Enter => {
                        // Single-line mode (or Ctrl+Enter in multi-line): submit
                        self.cursor.clear_selection();
                        self.on_submitted
                            .call(self.controller.text());
                        true
                    }
                    NamedKey::ArrowLeft => {
                        let offset = self.cursor.offset();
                        if modifiers.shift {
                            if self
                                .cursor
                                .selection_anchor()
                                .is_none()
                            {
                                self.cursor
                                    .set_selection_anchor(Some(offset));
                            }
                            if offset > 0 {
                                self.cursor
                                    .set_offset(offset - 1);
                            }
                        } else {
                            if let Some((start, _end)) = self.cursor.selection_range() {
                                self.cursor.set_offset(start);
                            } else if offset > 0 {
                                self.cursor
                                    .set_offset(offset - 1);
                            }
                            self.cursor.clear_selection();
                        }
                        true
                    }
                    NamedKey::ArrowRight => {
                        let offset = self.cursor.offset();
                        let len = self.controller.char_count();
                        if modifiers.shift {
                            if self
                                .cursor
                                .selection_anchor()
                                .is_none()
                            {
                                self.cursor
                                    .set_selection_anchor(Some(offset));
                            }
                            if offset < len {
                                self.cursor
                                    .set_offset(offset + 1);
                            }
                        } else {
                            if let Some((_start, end)) = self.cursor.selection_range() {
                                self.cursor.set_offset(end);
                            } else if offset < len {
                                self.cursor
                                    .set_offset(offset + 1);
                            }
                            self.cursor.clear_selection();
                        }
                        true
                    }
                    NamedKey::ArrowUp => {
                        let text = self.controller.text();
                        let offset = self.cursor.offset();
                        let chars: Vec<char> = text.chars().collect();
                        // Find start of current line
                        let line_start = chars[..offset]
                            .iter()
                            .rposition(|&c| c == '\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        if line_start == 0 {
                            return true;
                        } // already at first line
                        let col = offset - line_start;
                        // Find start of previous line
                        let prev_line_end = line_start - 1;
                        let prev_line_start = chars[..prev_line_end]
                            .iter()
                            .rposition(|&c| c == '\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let prev_line_len = prev_line_end - prev_line_start;
                        let new_offset = prev_line_start + col.min(prev_line_len);
                        if modifiers.shift {
                            if self
                                .cursor
                                .selection_anchor()
                                .is_none()
                            {
                                self.cursor
                                    .set_selection_anchor(Some(offset));
                            }
                        } else {
                            self.cursor.clear_selection();
                        }
                        self.cursor
                            .set_offset(new_offset);
                        true
                    }
                    NamedKey::ArrowDown => {
                        let text = self.controller.text();
                        let offset = self.cursor.offset();
                        let chars: Vec<char> = text.chars().collect();
                        // Find end of current line
                        let line_end = chars[offset..]
                            .iter()
                            .position(|&c| c == '\n')
                            .map(|p| offset + p)
                            .unwrap_or(chars.len());
                        if line_end >= chars.len() {
                            return true;
                        } // already at last line
                        let line_start = chars[..offset]
                            .iter()
                            .rposition(|&c| c == '\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let col = offset - line_start;
                        // Find next line
                        let next_line_start = line_end + 1;
                        let next_line_end = chars[next_line_start..]
                            .iter()
                            .position(|&c| c == '\n')
                            .map(|p| next_line_start + p)
                            .unwrap_or(chars.len());
                        let next_line_len = next_line_end - next_line_start;
                        let new_offset = next_line_start + col.min(next_line_len);
                        if modifiers.shift {
                            if self
                                .cursor
                                .selection_anchor()
                                .is_none()
                            {
                                self.cursor
                                    .set_selection_anchor(Some(offset));
                            }
                        } else {
                            self.cursor.clear_selection();
                        }
                        self.cursor
                            .set_offset(new_offset);
                        true
                    }
                    NamedKey::Home => {
                        if modifiers.shift {
                            let offset = self.cursor.offset();
                            if self
                                .cursor
                                .selection_anchor()
                                .is_none()
                            {
                                self.cursor
                                    .set_selection_anchor(Some(offset));
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
                            if self
                                .cursor
                                .selection_anchor()
                                .is_none()
                            {
                                self.cursor
                                    .set_selection_anchor(Some(offset));
                            }
                        } else {
                            self.cursor.clear_selection();
                        }
                        self.cursor
                            .set_offset(self.controller.char_count());
                        true
                    }
                    NamedKey::Escape => {
                        self.cursor.clear_selection();
                        self.set_focused(false);
                        self.on_blur
                            .call(self.controller.text());
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
            ElementEvent::PointerMove(pos, _, _) => {
                let is_inside = self
                    .cached_bounds
                    .is_inside(pos.x, pos.y);
                let was_hovered = self.is_hovered();
                if let Some(w) = get_window() {
                    if is_inside || self.mouse_held.get() {
                        w.set_cursor(winit::window::CursorIcon::Text);
                    } else {
                        w.set_cursor(winit::window::CursorIcon::Default);
                    }
                }
                self.set_hovered(is_inside);

                // Drag-to-select: when mouse is held, defer position resolution to draw()
                if self.mouse_held.get() {
                    self.pending_click
                        .set(Some(*pos));
                    return true;
                }

                was_hovered != is_inside
            }
            ElementEvent::PointerUp(_pos, _, _) => {
                self.mouse_held.set(false);
                false
            }
            ElementEvent::ImePreedit { text, cursor } => {
                if !self.is_focused() {
                    return false;
                }
                self.preedit_text
                    .set(text.clone());
                self.preedit_cursor
                    .set(*cursor);
                true
            }
            ElementEvent::Cancel => {
                self.set_focused(false);
                self.mouse_held.set(false);
                self.on_blur
                    .call(self.controller.text());
                self.preedit_text
                    .set(String::new());
                self.preedit_cursor.set(None);
                #[cfg(target_os = "ios")]
                ios_keyboard::dismiss_keyboard();
                #[cfg(target_os = "android")]
                android_keyboard::dismiss_keyboard();
                true
            }
            _ => false,
        }
    }
}

impl LayoutElement for RawTextField {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let (w, h) = self.compute_dimensions(ctx);
        let scale = ctx.scale;
        let (ol, ot, or, ob) = self.outline_strokes(w, h, scale);
        ResolvedSize {
            width: w + ol + or,
            height: h + ot + ob,
        }
    }
}

impl Drawable for RawTextField {
    fn draw(&self, ctx: &BuildContext) {
        ctx.canvas.save();

        let (box_width, box_height) = self.compute_dimensions(ctx);
        let scale = ctx.scale;

        // Translate inward by outline strokes so the outline has room to draw
        let (ol, ot, _or, _ob) = self.outline_strokes(box_width, box_height, scale);
        ctx.canvas
            .translate((ol, ot).into());

        // Cache absolute bounds for hit-testing
        let (abs_x, abs_y) = {
            let (tx, ty) = ctx
                .canvas
                .get_transform_translation();
            (tx, ty)
        };

        self.cached_bounds
            .save(scale, abs_x, abs_y, box_width, box_height);

        // --- Resolve active decoration ---
        let decoration = self.active_decoration();

        // --- Draw background + border + outline ---
        decoration.draw(ctx);

        // --- Padding ---
        let pad_top = self
            .padding
            .top
            .value(box_height, scale);
        let pad_bottom = self
            .padding
            .bottom
            .value(box_height, scale);
        let pad_left = self
            .padding
            .left
            .value(box_width, scale);
        let pad_right = self
            .padding
            .right
            .value(box_width, scale);

        ctx.canvas.save();
        let radii = decoration
            .border_radius
            .resolve(box_width, box_height, scale);
        let clip_radii = [
            if radii[0] > 0.0 {
                (radii[0]
                    - pad_left
                        .max(pad_top)
                        .min(radii[0]))
                .max(0.0)
            } else {
                0.0
            },
            if radii[1] > 0.0 {
                (radii[1]
                    - pad_right
                        .max(pad_top)
                        .min(radii[1]))
                .max(0.0)
            } else {
                0.0
            },
            if radii[2] > 0.0 {
                (radii[2]
                    - pad_right
                        .max(pad_bottom)
                        .min(radii[2]))
                .max(0.0)
            } else {
                0.0
            },
            if radii[3] > 0.0 {
                (radii[3]
                    - pad_left
                        .max(pad_bottom)
                        .min(radii[3]))
                .max(0.0)
            } else {
                0.0
            },
        ];
        ctx.canvas.set_clip_rounded(
            (pad_left, pad_top).into(),
            ResolvedSize {
                width: (box_width - pad_left - pad_right).max(0.0),
                height: (box_height - pad_top - pad_bottom).max(0.0),
            },
            clip_radii,
        );
        ctx.canvas
            .translate((pad_left, pad_top).into());

        let content_height = (box_height - pad_top - pad_bottom).max(0.0);
        let content_width = (box_width - pad_left - pad_right).max(0.0);

        let text = self.controller.text();
        let is_empty = text.is_empty();

        let font_size = self.scaled_font_size(&self.text_style, scale);

        // --- Process pending click (deferred from on_event for canvas access) ---
        if let Some(click_pos) = self.pending_click.take() {
            let display_for_measure = if is_empty {
                String::new()
            } else {
                match self.input_type {
                    InputType::Obscure => "\u{2022}".repeat(self.controller.char_count()),
                    _ => text.to_string(),
                }
            };
            let text_width = ctx
                .canvas
                .measure_text(&display_for_measure, font_size);
            let text_x = self.align_x(text_width, content_width);

            // click_pos is in logical (unscaled) coords; abs_x/pad_left/text_x
            // are in canvas (scaled) coords. Multiply by scale to align them.
            let click_canvas_x = click_pos.x * scale;
            let rel_x = click_canvas_x - abs_x - pad_left - text_x + self.scroll_x.get();

            use unicode_segmentation::UnicodeSegmentation;
            let graphemes: Vec<&str> = if display_for_measure.is_empty() {
                vec![]
            } else {
                display_for_measure
                    .graphemes(true)
                    .collect()
            };
            let mut click_offset = graphemes.len(); // default: past end
            if !graphemes.is_empty() {
                let mut acc_width = 0.0f32;
                for (i, g) in graphemes.iter().enumerate() {
                    let g_width = ctx
                        .canvas
                        .measure_text(g, font_size);
                    if rel_x <= acc_width + g_width / 2.0 {
                        click_offset = i;
                        break;
                    }
                    acc_width += g_width;
                }
            }

            // Apply double/triple-click selection
            let click_count = self.click_count.get();
            match click_count {
                2 => self.select_word_at(click_offset),
                3 => {
                    self.select_line_at(click_offset);
                    self.click_count.set(0);
                }
                _ => {
                    // For drag-to-select: set anchor to the click position (not the old cursor)
                    // so the selection extends from the click point to the drag destination.
                    if self.mouse_held.get()
                        && self
                            .cursor
                            .selection_anchor()
                            .is_none()
                    {
                        self.cursor
                            .set_selection_anchor(Some(click_offset));
                    }
                    self.cursor
                        .set_offset(click_offset);
                }
            }
            self.cursor.reset_blink();
        }

        // Context with parent_size set to the padded content area
        let mut content_ctx = ctx.clone();
        content_ctx.parent_size = ResolvedSize {
            width: content_width,
            height: content_height,
        };

        if is_empty {
            // --- Draw prompt (visible when field is empty) ---
            if !self.prompt.is_empty() {
                let prompt_widget =
                    self.build_text_widget(&self.prompt, &self.prompt_style, self.text_align);
                prompt_widget.draw(&content_ctx);
            } else if !self.hint.is_empty() {
                let hint_widget =
                    self.build_text_widget(&self.hint, &self.hint_style, self.text_align);
                hint_widget.draw(&content_ctx);
            }

            // --- Draw cursor when field is empty but focused ---
            if self.is_focused() && self.cursor.is_visible() {
                let cursor_x = self.align_x(0.0, content_width);
                let cursor_top = content_height * 0.15;
                let cursor_bottom = content_height * 0.85;
                let cursor_height = cursor_bottom - cursor_top;
                let cursor_color: Color = self.cursor.color.into();
                let stroke_w = 1.5 * scale;

                ctx.canvas.fill_color_rect(
                    (cursor_x, cursor_top).into(),
                    ResolvedSize {
                        width: stroke_w,
                        height: cursor_height,
                    },
                    cursor_color,
                    [0.0; 4],
                );
            }
        } else {
            // --- Draw text ---
            let display = match self.input_type {
                InputType::Obscure => "\u{2022}".repeat(self.controller.char_count()),
                _ => text.to_string(),
            };

            let is_multiline = display.contains('\n');

            if is_multiline {
                // --- Multi-line rendering ---
                let lines: Vec<&str> = display.split('\n').collect();
                // Use real font vertical metrics (ascent + descent + line_gap)
                // instead of an approximate multiplier. The hardcoded 1.4× could
                // clip descenders when the actual line height exceeds it.
                let line_metrics = ctx
                    .canvas
                    .measure_text_metrics("", font_size, 0.0);
                let line_height = line_metrics.line_height;
                let total_text_height = lines.len() as f32 * line_height;
                let base_y = match self.text_align {
                    TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => 0.0,
                    TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => {
                        (content_height - total_text_height) / 2.0
                    }
                    TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => {
                        content_height - total_text_height
                    }
                };

                // Track grapheme offset for selection/cursor across lines
                let mut grapheme_offset = 0usize;

                for (line_idx, line) in lines.iter().enumerate() {
                    let line_y = base_y + line_idx as f32 * line_height;
                    let line_graphemes: usize = line.chars().count();

                    let line_width = ctx
                        .canvas
                        .measure_text(line, font_size);
                    let line_x = self.align_x(line_width, content_width);

                    // Draw selection highlight for this line
                    if let Some((sel_start, sel_end)) = self.cursor.selection_range() {
                        let line_start = grapheme_offset;
                        let line_end = grapheme_offset + line_graphemes;

                        if sel_start < line_end && sel_end > line_start {
                            let local_start = sel_start.saturating_sub(line_start);
                            let local_end = (sel_end - line_start).min(line_graphemes);
                            let hl_x = line_x
                                + self.text_width_to_offset(
                                    line,
                                    local_start,
                                    &ctx.canvas,
                                    font_size,
                                );
                            let hl_end_x = line_x
                                + self.text_width_to_offset(
                                    line,
                                    local_end,
                                    &ctx.canvas,
                                    font_size,
                                );

                            ctx.canvas.fill_color_rect(
                                (hl_x, line_y).into(),
                                ResolvedSize {
                                    width: hl_end_x - hl_x,
                                    height: line_height,
                                },
                                self.selection_color,
                                [0.0; 4],
                            );
                        }
                    }

                    // Draw line text
                    ctx.canvas.save();
                    ctx.canvas
                        .translate((0.0, line_y).into());
                    let mut line_ctx = content_ctx.clone();
                    line_ctx.parent_size = ResolvedSize {
                        width: content_width,
                        height: line_height,
                    };
                    let line_widget =
                        self.build_text_widget(line, &self.text_style, self.text_align);
                    line_widget.draw(&line_ctx);
                    ctx.canvas.restore();

                    // Draw cursor if on this line
                    if self.is_focused() && self.cursor.is_visible() {
                        let cursor_off = self.cursor.offset();
                        if cursor_off >= grapheme_offset
                            && cursor_off <= grapheme_offset + line_graphemes
                        {
                            let local_off = cursor_off - grapheme_offset;
                            let cursor_x = line_x
                                + self.text_width_to_offset(
                                    line,
                                    local_off,
                                    &ctx.canvas,
                                    font_size,
                                );
                            let cursor_top = line_y + line_height * 0.15;
                            let cursor_bottom = line_y + line_height * 0.85;
                            let cursor_color: Color = self.cursor.color.into();
                            let stroke_w = 1.5 * scale;

                            ctx.canvas.fill_color_rect(
                                (cursor_x, cursor_top).into(),
                                ResolvedSize {
                                    width: stroke_w,
                                    height: cursor_bottom - cursor_top,
                                },
                                cursor_color,
                                [0.0; 4],
                            );
                        }
                    }

                    grapheme_offset += line_graphemes;
                    // Account for the '\n' character in offset counting
                    if line_idx < lines.len() - 1 {
                        grapheme_offset += 1;
                    }
                }
            } else {
                // --- Single-line rendering (with horizontal scroll) ---
                let text_width = ctx
                    .canvas
                    .measure_text(&display, font_size);
                let text_x = self.align_x(text_width, content_width);

                // Ensure cursor is visible
                self.ensure_cursor_visible(content_width, &ctx.canvas, font_size);
                let scroll = self.scroll_x.get();

                // Draw text — RawTextWidget handles alignment via text_align + parent_size.
                // Apply scroll by translating the canvas so the visible portion aligns.
                ctx.canvas.save();
                ctx.canvas
                    .translate((-scroll, 0.0).into());
                let text_widget =
                    self.build_text_widget(&display, &self.text_style, self.text_align);
                text_widget.draw(&content_ctx);
                ctx.canvas.restore();

                // --- Draw selection highlight ---
                if let Some((sel_start, sel_end)) = self.cursor.selection_range()
                    && sel_start != sel_end
                {
                    let highlight_x = text_x - scroll
                        + self.text_width_to_offset(&display, sel_start, &ctx.canvas, font_size);
                    let highlight_end_x = text_x - scroll
                        + self.text_width_to_offset(&display, sel_end, &ctx.canvas, font_size);
                    let highlight_width = highlight_end_x - highlight_x;

                    ctx.canvas.fill_color_rect(
                        (highlight_x, 0.0).into(),
                        ResolvedSize {
                            width: highlight_width,
                            height: content_height,
                        },
                        self.selection_color,
                        [0.0; 4],
                    );
                }

                // --- Draw cursor ---
                if self.is_focused() && self.cursor.is_visible() {
                    let cursor_x =
                        text_x - scroll + self.cursor_x_offset_canvas(&ctx.canvas, font_size);
                    let cursor_top = content_height * 0.15;
                    let cursor_bottom = content_height * 0.85;
                    let cursor_height = cursor_bottom - cursor_top;
                    let cursor_color: Color = self.cursor.color.into();
                    let stroke_w = 1.5 * scale;

                    ctx.canvas.fill_color_rect(
                        (cursor_x, cursor_top).into(),
                        ResolvedSize {
                            width: stroke_w,
                            height: cursor_height,
                        },
                        cursor_color,
                        [0.0; 4],
                    );
                }

                // --- Draw IME preedit text ---
                let preedit = self.preedit_text.take();
                if !preedit.is_empty() && self.is_focused() {
                    self.preedit_text
                        .set(preedit.clone());
                    let cursor_x =
                        text_x - scroll + self.cursor_x_offset_canvas(&ctx.canvas, font_size);
                    let preedit_width = ctx
                        .canvas
                        .measure_text(&preedit, font_size);

                    // Draw preedit text at cursor position
                    ctx.canvas.save();
                    ctx.canvas
                        .translate((cursor_x, 0.0).into());
                    let mut preedit_ctx = content_ctx.clone();
                    preedit_ctx.parent_size = ResolvedSize {
                        width: preedit_width,
                        height: content_height,
                    };
                    let preedit_widget =
                        self.build_text_widget(&preedit, &self.text_style, self.text_align);
                    preedit_widget.draw(&preedit_ctx);
                    ctx.canvas.restore();

                    // Draw underline under preedit text
                    let underline_y = content_height * 0.85;
                    let cursor_color: Color = self.cursor.color.into();
                    ctx.canvas.fill_color_rect(
                        (cursor_x, underline_y).into(),
                        ResolvedSize {
                            width: preedit_width,
                            height: 1.0 * scale,
                        },
                        cursor_color,
                        [0.0; 4],
                    );
                }
            }
        }

        ctx.canvas.clear_clip();
        ctx.canvas.restore(); // clip + translate
        ctx.canvas.restore(); // outer save

        // Drive cursor blink: only schedule a new frame when the blink actually
        // toggled (~500ms interval) instead of every frame (~16ms). This reduces
        // focused rendering from ~60fps to ~2fps.
        if self.is_focused() {
            let toggled = self.cursor.update_blink();
            if toggled || !self.blink_scheduled.get() {
                self.blink_scheduled.set(true);
                let rate = self.cursor.blink_rate_ms;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(rate));
                    aimer_events::window::request_animation_frame();
                });
            }
        }
    }
}
