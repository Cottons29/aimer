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
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutCache, LayoutElement,
    PointerKey, VisitorElement, Widget,
};

use crate::input_field::caret::CaretBlink;
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
    let el = document.get_element_by_id(HIDDEN_INPUT_ID)?;
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
    /// Blink timeline shared with the owning field state, so the phase outlives
    /// the element it is painted from.
    blink: CaretBlink,
    radius: Option<f32>,
    color: Colors,
}

impl Cursor {
    /// Creates a cursor with a blink timeline of its own.
    pub fn new(color: Colors) -> Self {
        Self::with_blink(color, CaretBlink::new())
    }

    /// Creates a cursor that blinks on `blink`.
    ///
    /// A field built by [`TextField`] passes the timeline owned by its state,
    /// which is what keeps the caret rhythm continuous across rebuilds.
    ///
    /// [`TextField`]: crate::input_field::TextField
    pub fn with_blink(color: Colors, blink: CaretBlink) -> Self {
        Self {
            cursor: "|".to_string(),
            offset: UnsafeCell::new(0),
            selection_anchor: UnsafeCell::new(None),
            blink,
            radius: None,
            color,
        }
    }

    /// Returns the blink timeline this cursor is painted from.
    #[inline]
    pub fn blink(&self) -> &CaretBlink {
        &self.blink
    }

    pub fn offset(&self) -> usize {
        unsafe { *self.offset.get() }
    }

    pub fn set_offset(&self, offset: usize) {
        unsafe {
            *self.offset.get() = offset;
        }
    }

    /// Returns whether the caret is opaque at the current blink phase.
    pub fn is_visible(&self) -> bool {
        self.blink.is_visible()
    }

    /// Restart the blink timeline so the caret is solid again.
    fn reset_blink(&self) {
        self.blink.reset();
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandDirection {
    Horizontal,
    Vertical,
    Both,
    #[default]
    None,
}
/// Everything a [`RawTextField`] needs that is configuration rather than
/// runtime state.
///
/// [`TextField`] keeps one of these in its state and hands a clone to every
/// element it builds, so the widget configuration and the caret timeline travel
/// separately: the configuration is replaced on reconciliation while the
/// timeline keeps running.
///
/// [`TextField`]: crate::input_field::TextField
#[derive(Clone)]
pub(crate) struct RawFieldConfig {
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
    pub decoration: BoxDecoration,
    pub hover_decoration: Option<BoxDecoration>,
    pub focus_decoration: Option<BoxDecoration>,
    pub disabled_decoration: Option<BoxDecoration>,
    pub selection_color: Color,
    pub cursor_color: Colors,
    pub on_changed: TextFieldCallback,
    pub on_submitted: TextFieldCallback,
    pub on_focus: TextFieldCallback,
    pub on_blur: TextFieldCallback,
    pub read_only: bool,
    pub padding: LayoutSpacing,
}

/// The widget that mounts a [`RawTextField`] element.
///
/// The element owns the interaction state that only makes sense while mounted
/// (focus, hover, selection drag, IME composition), so it must be produced from
/// a widget rather than stored in one. This wrapper is that widget: it carries
/// the configuration and the caret timeline of the field that built it.
pub(crate) struct RawTextFieldWidget {
    config: RawFieldConfig,
    caret: CaretBlink,
}

impl RawTextFieldWidget {
    /// Creates the widget for a field configured by `config` blinking on
    /// `caret`.
    #[inline]
    pub(crate) fn new(config: RawFieldConfig, caret: CaretBlink) -> Self {
        Self { config, caret }
    }
}

impl Widget for RawTextFieldWidget {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        RawTextField::new(self.config.clone(), self.caret.clone()).boxed()
    }
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
    pub mouse_held: Cell<Option<PointerKey>>,
    pub last_click_time: Cell<AnimInstant>,
    pub click_count: Cell<u8>,
    pub pending_click: Cell<Option<Vec2d>>,
    pub scroll_x: Cell<f32>,
    pub preedit_text: Cell<String>,
    pub preedit_cursor: Cell<Option<(usize, usize)>>,
    pub ime_enabled: Cell<bool>,
    pub ime_cursor_area: Cell<Option<ImeCaretArea>>,
    pub padding: LayoutSpacing,
}

/// Caret rectangle reported to the platform input method, in logical window
/// coordinates.
///
/// The input method places its candidate window relative to this rectangle, so
/// it has to follow the caret as text is typed or scrolled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImeCaretArea {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RawTextField {
    /// Smallest caret movement, in logical pixels, that is worth reporting to
    /// the platform input method.
    const IME_AREA_EPSILON: f32 = 0.1;

    /// Logical width of the caret rectangle reported to the platform input
    /// method.
    const IME_CARET_WIDTH: f32 = 1.0;

    /// Builds the element for `config`, painting its caret from `caret`.
    ///
    /// All runtime state starts empty except focus, which honors
    /// [`RawFieldConfig::auto_focus`].
    pub(crate) fn new(config: RawFieldConfig, caret: CaretBlink) -> Self {
        Self {
            input_type: config.input_type,
            controller: config.controller,
            prompt: config.prompt,
            hint: config.hint,
            hint_style: config.hint_style,
            text_style: config.text_style,
            prompt_style: config.prompt_style,
            text_align: config.text_align,
            auto_focus: config.auto_focus,
            max_lines: config.max_lines,
            min_lines: config.min_lines,
            max_length: config.max_length,
            enable: config.enable,
            expand: config.expand,
            cursor: Cursor::with_blink(config.cursor_color, caret),
            decoration: config.decoration,
            hover_decoration: config.hover_decoration,
            focus_decoration: config.focus_decoration,
            disabled_decoration: config.disabled_decoration,
            selection_color: config.selection_color,
            focused: Cell::new(config.auto_focus),
            hovered: Cell::new(false),
            cached_bounds: CacheBounds::new(),
            on_changed: config.on_changed,
            on_submitted: config.on_submitted,
            on_focus: config.on_focus,
            on_blur: config.on_blur,
            read_only: config.read_only,
            mouse_held: Cell::new(None),
            last_click_time: Cell::new(AnimInstant::now()),
            click_count: Cell::new(0),
            pending_click: Cell::new(None),
            scroll_x: Cell::new(0.0),
            preedit_text: Cell::new(String::new()),
            preedit_cursor: Cell::new(None),
            ime_enabled: Cell::new(false),
            ime_cursor_area: Cell::new(None),
            padding: config.padding,
        }
    }

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

    /// Focuses or blurs the field and brings platform text input in sync.
    ///
    /// Gaining focus is what enables the input method — not the click that
    /// usually causes it — so a field focused by [`auto_focus`] or by code can
    /// compose immediately. Losing focus abandons any composition.
    ///
    /// [`auto_focus`]: crate::input_field::text_field::TextField::auto_focus
    fn set_focused(&self, focused: bool) {
        let was_focused = self.focused.replace(focused);
        if focused {
            self.enable_platform_ime();
            return;
        }
        if was_focused {
            self.clear_preedit();
        }
        self.disable_platform_ime();
    }

    /// Turns platform text input on for this field.
    ///
    /// Idempotent: the platform is only told on the transition into the enabled
    /// state, because raising the keyboard and allowing IME are window-server
    /// round trips.
    fn enable_platform_ime(&self) {
        if self.ime_enabled.replace(true) {
            return;
        }
        #[cfg(target_os = "ios")]
        ios_keyboard::show_keyboard();
        #[cfg(target_os = "android")]
        android_keyboard::show_keyboard();
        #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
        if let Some(w) = get_window() {
            w.set_ime_allowed(true);
        }
        #[cfg(target_arch = "wasm32")]
        wasm_request_keyboard(true);
    }

    /// Turns platform text input off and forgets the reported caret area.
    fn disable_platform_ime(&self) {
        self.ime_cursor_area.set(None);
        if !self.ime_enabled.replace(false) {
            return;
        }
        #[cfg(target_os = "ios")]
        ios_keyboard::dismiss_keyboard();
        #[cfg(target_os = "android")]
        android_keyboard::dismiss_keyboard();
        #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
        if let Some(w) = get_window() {
            w.set_ime_allowed(false);
        }
        #[cfg(target_arch = "wasm32")]
        wasm_request_keyboard(false);
    }

    /// Reports `caret` to the platform so the candidate window follows the
    /// insertion point.
    ///
    /// The rectangle is submitted only once it has moved by at least
    /// [`Self::IME_AREA_EPSILON`] logical pixels: the field draws every blink
    /// frame, and telling the window server about an unchanged area on each of
    /// them is pure overhead.
    fn update_ime_cursor_area(&self, caret: ImeCaretArea) {
        // In the browser the caret is reported by moving the hidden input the
        // composition happens in, and that element is positioned against the
        // viewport rather than against the canvas. Converting before the
        // comparison also makes a scrolled or resized page resubmit the area,
        // because the canvas origin is part of what is being compared.
        #[cfg(target_arch = "wasm32")]
        let caret = ime_overlay_rect(caret, wasm_canvas_origin());

        if !self.ime_cursor_area_moved(caret) {
            return;
        }
        self.ime_cursor_area.set(Some(caret));
        #[cfg(not(any(target_os = "ios", target_os = "android", target_arch = "wasm32")))]
        if let Some(w) = get_window() {
            use winit::dpi::{LogicalPosition, LogicalSize};
            w.set_ime_cursor_area(
                LogicalPosition::new(caret.x as f64, caret.y as f64),
                LogicalSize::new(caret.width.max(1.0) as f64, caret.height.max(1.0) as f64),
            );
        }
        #[cfg(target_arch = "wasm32")]
        wasm_place_ime_input(caret);
    }

    /// Returns whether `caret` differs from the last reported caret area.
    fn ime_cursor_area_moved(&self, caret: ImeCaretArea) -> bool {
        match self.ime_cursor_area.get() {
            Some(previous) => {
                (previous.x - caret.x).abs() > Self::IME_AREA_EPSILON
                    || (previous.y - caret.y).abs() > Self::IME_AREA_EPSILON
                    || (previous.width - caret.width).abs() > Self::IME_AREA_EPSILON
                    || (previous.height - caret.height).abs() > Self::IME_AREA_EPSILON
            }
            None => true,
        }
    }

    /// Converts a caret drawn at content-local canvas coordinates into the
    /// logical window rectangle the platform expects and reports it.
    fn publish_ime_caret(
        &self,
        local_x: f32,
        local_y: f32,
        height: f32,
        content_origin: (f32, f32),
        scale: f32,
    ) {
        if !self.is_focused() {
            return;
        }
        let scale = if scale > 0.0 { scale } else { 1.0 };
        self.update_ime_cursor_area(ImeCaretArea {
            x: (content_origin.0 + local_x) / scale,
            y: (content_origin.1 + local_y) / scale,
            width: Self::IME_CARET_WIDTH,
            height: height / scale,
        });
    }

    /// Borrows the composition string without cloning it.
    ///
    /// The value is moved out of its cell for the duration of `f` and put back
    /// afterwards, so reading it every frame costs no allocation.
    fn with_preedit<R>(&self, f: impl FnOnce(&str) -> R) -> R {
        let preedit = self.preedit_text.take();
        let value = f(&preedit);
        self.preedit_text.set(preedit);
        value
    }

    /// Returns whether an input-method composition is currently in progress.
    fn is_composing(&self) -> bool {
        self.with_preedit(|preedit| !preedit.is_empty())
    }

    /// Replaces the composition, returning whether anything changed.
    ///
    /// Input methods resend an identical preedit while candidates are browsed;
    /// reporting no change lets the caller skip a repaint.
    fn set_preedit(&self, text: &str, cursor: Option<(usize, usize)>) -> bool {
        let cursor_changed = self.preedit_cursor.replace(cursor) != cursor;
        let text_changed = self.with_preedit(|preedit| preedit != text);
        if text_changed {
            self.preedit_text.set(text.to_owned());
        }
        text_changed || cursor_changed
    }

    /// Abandons any composition in progress.
    fn clear_preedit(&self) {
        self.preedit_cursor.set(None);
        if self.is_composing() {
            self.preedit_text.set(String::new());
        }
    }

    /// Inserts `text` at the cursor as a single edit.
    ///
    /// The current selection, if any, is replaced first. A `max_length` limit
    /// truncates the payload instead of rejecting it, so a committed phrase
    /// still inserts as much as fits. The whole payload produces one undo
    /// entry, one cursor advance, and one `on_changed` call, which keeps undo
    /// granularity at the phrase the user actually typed.
    ///
    /// Returns whether the text changed.
    fn insert_text(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        if let Some((start, end)) = self.cursor.selection_range() {
            if start != end {
                self.controller.delete_range(start, end);
            }
            self.cursor.set_offset(start);
            self.cursor.clear_selection();
        }

        let text = match self.max_length {
            Some(max) => {
                let room = max.saturating_sub(self.controller.grapheme_count());
                if room == 0 {
                    return false;
                }
                truncate_graphemes(text, room)
            }
            None => text,
        };
        if text.is_empty() {
            return false;
        }

        let offset = self.cursor.offset();
        let inserted = grapheme_count(text);
        self.controller.insert_str(text, offset);
        self.cursor.set_offset(offset + inserted);
        self.cursor.reset_blink();
        self.clear_preedit();
        self.on_changed.call(self.controller.text());
        true
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

        // Word boundaries are reported in bytes, so the click offset is
        // converted into byte space once and the matching bounds are converted
        // back into grapheme offsets. Counting `char`s here would land inside a
        // cluster and select the wrong word.
        let byte_offset = text
            .grapheme_indices(true)
            .nth(grapheme_offset)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len());

        for (start, segment) in text.split_word_bound_indices() {
            let end = start + segment.len();
            if byte_offset >= start && byte_offset < end {
                self.cursor
                    .set_selection_anchor(Some(text[..start].graphemes(true).count()));
                self.cursor.set_offset(text[..end].graphemes(true).count());
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

        let graphemes = grapheme_slices(text);
        let mut line_start = grapheme_offset.min(graphemes.len());
        let mut line_end = line_start;

        while line_start > 0 && graphemes[line_start - 1] != "\n" {
            line_start -= 1;
        }
        while line_end < graphemes.len() && graphemes[line_end] != "\n" {
            line_end += 1;
        }

        self.cursor.set_selection_anchor(Some(line_start));
        self.cursor.set_offset(line_end);
    }

    /// Adjust `scroll_x` so the cursor is visible within `content_width`.
    ///
    /// While an input method is composing, the end of the composition is the
    /// point the user is looking at, so the composition width is included;
    /// otherwise a long phrase would grow out of the clipped viewport.
    fn ensure_cursor_visible(
        &self,
        content_width: f32,
        canvas: &aimer_canvas::Canvas,
        font_size: f32,
    ) {
        let cursor_x = self.cursor_x_offset_canvas(canvas, font_size);
        let composition_end = cursor_x
            + self.with_preedit(|preedit| {
                if preedit.is_empty() {
                    0.0
                } else {
                    canvas.measure_text(preedit, font_size)
                }
            });
        let scroll = self.scroll_x.get();

        if cursor_x < scroll {
            self.scroll_x.set(cursor_x.max(0.0));
        } else if composition_end > scroll + content_width {
            self.scroll_x.set((composition_end - content_width).max(0.0));
        }
    }

    /// Draws the input-method composition, its underlines, and its caret.
    ///
    /// `origin_x` and `top` are the content-local canvas coordinates of the
    /// insertion point, and `height` is the line height the composition shares
    /// with the surrounding text. `cursor` is the byte range the input method
    /// reports inside `preedit`: an empty range is the composition caret, while
    /// a non-empty one marks the clause being edited and is underlined twice as
    /// thick so long Japanese or Korean compositions show which part is active.
    #[allow(clippy::too_many_arguments)]
    fn draw_preedit(
        &self,
        preedit: &str,
        cursor: Option<(usize, usize)>,
        origin_x: f32,
        top: f32,
        height: f32,
        content_ctx: &BuildContext,
        font_size: f32,
        scale: f32,
    ) {
        let canvas = &content_ctx.canvas;
        let width = canvas.measure_text(preedit, font_size);

        canvas.save();
        canvas.translate((origin_x, top).into());
        let mut preedit_ctx = content_ctx.clone();
        preedit_ctx.parent_size = ResolvedSize { width, height };
        let preedit_widget = self.build_text_widget(preedit, &self.text_style, self.text_align);
        preedit_widget.draw(&preedit_ctx);
        canvas.restore();

        let color: Color = self.cursor.color.into();
        let underline_y = top + height * 0.85;
        canvas.fill_color_rect(
            (origin_x, underline_y).into(),
            ResolvedSize {
                width,
                height: scale,
            },
            color,
            [0.0; 4],
        );

        let Some((start, end)) = cursor else {
            return;
        };
        let start = floor_char_boundary(preedit, start);
        let end = floor_char_boundary(preedit, end.max(start));
        let start_x = origin_x + canvas.measure_text(&preedit[..start], font_size);

        if end > start {
            let end_x = origin_x + canvas.measure_text(&preedit[..end], font_size);
            canvas.fill_color_rect(
                (start_x, underline_y - scale).into(),
                ResolvedSize {
                    width: end_x - start_x,
                    height: 2.0 * scale,
                },
                color,
                [0.0; 4],
            );
        } else {
            canvas.fill_color_rect(
                (start_x, top + height * 0.15).into(),
                ResolvedSize {
                    width: 1.5 * scale,
                    height: height * 0.7,
                },
                color,
                [0.0; 4],
            );
        }
    }

    /// The text as it is drawn.
    ///
    /// An obscured field shows one bullet per grapheme cluster, so a family
    /// emoji hides behind a single dot and the bullets stay in step with the
    /// cursor offsets used for hit testing and caret placement.
    fn display_text(&self) -> String {
        match self.input_type {
            InputType::Obscure => "\u{2022}".repeat(self.controller.grapheme_count()),
            _ => self.controller.text().to_owned(),
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

/// Returns the longest prefix of `text` holding at most `max_graphemes`
/// grapheme clusters.
///
/// Cutting on a cluster boundary is what keeps a length limit from splitting a
/// family emoji into stray code points or stranding a combining accent without
/// its base letter.
fn truncate_graphemes(text: &str, max_graphemes: usize) -> &str {
    use unicode_segmentation::UnicodeSegmentation;
    match text.grapheme_indices(true).nth(max_graphemes) {
        Some((byte, _)) => &text[..byte],
        None => text,
    }
}

/// Splits `text` into grapheme clusters, the unit every cursor offset counts.
fn grapheme_slices(text: &str) -> Vec<&str> {
    unicode_segmentation::UnicodeSegmentation::graphemes(text, true).collect()
}

/// Counts the grapheme clusters in `text` without allocating.
fn grapheme_count(text: &str) -> usize {
    unicode_segmentation::UnicodeSegmentation::graphemes(text, true).count()
}

/// Clamps `byte` down to the nearest character boundary of `text`.
///
/// Input methods report composition ranges in bytes; a range that lands inside
/// a multi-byte character must not be used to slice the string.
fn floor_char_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

fn event_pointer_key(event: &ElementEvent) -> Option<PointerKey> {
    match event {
        ElementEvent::PointerDown(_, source, id)
        | ElementEvent::PointerUp(_, source, id)
        | ElementEvent::PointerMove(_, source, id)
        | ElementEvent::PointerExited(source, id) => Some(PointerKey::new(*source, *id)),
        _ => None,
    }
}

fn owns_selection_pointer(active: Option<PointerKey>, event: &ElementEvent) -> bool {
    active.is_some() && active == event_pointer_key(event)
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

impl VisitorElement for RawTextField {
    fn debug_name(&self) -> &'static str {
        "TextField"
    }
}

impl EventElement for RawTextField {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let active_before = self.mouse_held.get();
        let consumed = (|| {
            if !self.enable {
                return false;
            }

            // debug!("RawTextField on_event: {:?}", event);

            match event {
                ElementEvent::PointerDown(pos, source, id) => {
                    let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

                    if is_inside {
                        let was_focused = self.is_focused();
                        self.set_focused(true);
                        self.mouse_held.set(Some(PointerKey::new(*source, *id)));
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
                        self.click_count.set(new_count);
                        self.last_click_time.set(now);

                        // Defer cursor placement to draw() where canvas is available
                        self.pending_click.set(Some(*pos));
                        self.cursor.reset_blink();

                        if !was_focused {
                            self.on_focus.call(self.controller.text());
                        }

                        // Clear IME preedit on new click
                        self.clear_preedit();
                        true
                    } else {
                        if self.mouse_held.get().is_some() {
                            return false;
                        }
                        self.set_focused(false);
                        self.on_blur.call(self.controller.text());
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

                    let mut encoded = [0u8; 4];
                    self.insert_text(ch.encode_utf8(&mut encoded))
                }
                ElementEvent::TextInput { text, action, .. } => {
                    if !self.is_focused() || self.read_only {
                        return false;
                    }
                    if *action == KeyAction::Released {
                        return false;
                    }

                    self.insert_text(text)
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
                                self.cursor.set_selection_anchor(Some(0));
                                self.cursor.set_offset(self.controller.grapheme_count());
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
                            NamedKey::Other(k) if k == "x" && !self.read_only => {
                                // Cut
                                if let Some((start, end)) = self.cursor.selection_range() {
                                    let selected = self.controller.delete_range(start, end);
                                    clipboard_write(&selected);
                                    self.cursor.set_offset(start);
                                    self.cursor.clear_selection();
                                    self.on_changed.call(self.controller.text());
                                }
                                true
                            }
                            NamedKey::Other(k) if k == "v" && !self.read_only => {
                                // Paste. Routing through `insert_text` replaces
                                // the selection, advances the cursor by whole
                                // grapheme clusters, honours `max_length`, and
                                // records the paste as a single undo entry.
                                if let Some(text) = clipboard_read() {
                                    self.insert_text(&text);
                                }
                                true
                            }
                            NamedKey::Other(k)
                                if k == "z" && !modifiers.shift && !self.read_only =>
                            {
                                // Undo
                                if self.controller.undo() {
                                    let len = self.controller.grapheme_count();
                                    let off = self.cursor.offset();
                                    if off > len {
                                        self.cursor.set_offset(len);
                                    }
                                    self.on_changed.call(self.controller.text());
                                }
                                true
                            }
                            NamedKey::Other(k)
                                if k == "z" && modifiers.shift && !self.read_only =>
                            {
                                // Redo (Ctrl+Shift+Z)
                                if self.controller.redo() {
                                    let len = self.controller.grapheme_count();
                                    let off = self.cursor.offset();
                                    if off > len {
                                        self.cursor.set_offset(len);
                                    }
                                    self.on_changed.call(self.controller.text());
                                }
                                true
                            }
                            NamedKey::Other(k) if k == "y" && !self.read_only => {
                                // Redo (Ctrl+Y — Windows convention)
                                if self.controller.redo() {
                                    let len = self.controller.grapheme_count();
                                    let off = self.cursor.offset();
                                    if off > len {
                                        self.cursor.set_offset(len);
                                    }
                                    self.on_changed.call(self.controller.text());
                                }
                                true
                            }
                            NamedKey::Enter => {
                                // Ctrl+Enter / Cmd+Enter: submit even in multi-line mode
                                self.cursor.clear_selection();
                                self.on_submitted.call(self.controller.text());
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
                                self.controller.delete_range(start, end);
                                self.cursor.set_offset(start);
                                self.cursor.clear_selection();
                                self.on_changed.call(self.controller.text());
                            } else {
                                let offset = self.cursor.offset();
                                if offset > 0 {
                                    self.controller.delete_grapheme(offset - 1);
                                    self.cursor.set_offset(offset - 1);
                                    self.on_changed.call(self.controller.text());
                                }
                            }
                            true
                        }
                        NamedKey::Delete if !self.read_only => {
                            if let Some((start, end)) = self.cursor.selection_range() {
                                self.controller.delete_range(start, end);
                                self.cursor.set_offset(start);
                                self.cursor.clear_selection();
                                self.on_changed.call(self.controller.text());
                            } else {
                                let offset = self.cursor.offset();
                                if offset < self.controller.grapheme_count() {
                                    self.controller.delete_grapheme(offset);
                                    self.on_changed.call(self.controller.text());
                                }
                            }
                            true
                        }
                        NamedKey::Enter
                            if !self.read_only && self.max_lines.is_some_and(|max| max > 1) =>
                        {
                            // Multi-line mode: Enter inserts newline
                            if let Some(max) = self.max_lines
                                && self.line_count() >= max
                            {
                                return true;
                            }
                            // Delete selection first
                            if let Some((start, end)) = self.cursor.selection_range() {
                                self.controller.delete_range(start, end);
                                self.cursor.set_offset(start);
                                self.cursor.clear_selection();
                            }
                            let offset = self.cursor.offset();
                            unsafe {
                                self.controller.insert_char('\n', offset);
                            }
                            self.cursor.set_offset(offset + 1);
                            self.on_changed.call(self.controller.text());
                            true
                        }
                        NamedKey::Enter => {
                            // Single-line mode (or Ctrl+Enter in multi-line): submit
                            self.cursor.clear_selection();
                            self.on_submitted.call(self.controller.text());
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
                            let len = self.controller.grapheme_count();
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
                        NamedKey::ArrowUp => {
                            let text = self.controller.text();
                            let graphemes = grapheme_slices(text);
                            let offset = self.cursor.offset().min(graphemes.len());
                            // Find start of current line
                            let line_start = graphemes[..offset]
                                .iter()
                                .rposition(|&g| g == "\n")
                                .map(|p| p + 1)
                                .unwrap_or(0);
                            if line_start == 0 {
                                return true;
                            } // already at first line
                            let col = offset - line_start;
                            // Find start of previous line
                            let prev_line_end = line_start - 1;
                            let prev_line_start = graphemes[..prev_line_end]
                                .iter()
                                .rposition(|&g| g == "\n")
                                .map(|p| p + 1)
                                .unwrap_or(0);
                            let prev_line_len = prev_line_end - prev_line_start;
                            let new_offset = prev_line_start + col.min(prev_line_len);
                            if modifiers.shift {
                                if self.cursor.selection_anchor().is_none() {
                                    self.cursor.set_selection_anchor(Some(offset));
                                }
                            } else {
                                self.cursor.clear_selection();
                            }
                            self.cursor.set_offset(new_offset);
                            true
                        }
                        NamedKey::ArrowDown => {
                            let text = self.controller.text();
                            let graphemes = grapheme_slices(text);
                            let offset = self.cursor.offset().min(graphemes.len());
                            // Find end of current line
                            let line_end = graphemes[offset..]
                                .iter()
                                .position(|&g| g == "\n")
                                .map(|p| offset + p)
                                .unwrap_or(graphemes.len());
                            if line_end >= graphemes.len() {
                                return true;
                            } // already at last line
                            let line_start = graphemes[..offset]
                                .iter()
                                .rposition(|&g| g == "\n")
                                .map(|p| p + 1)
                                .unwrap_or(0);
                            let col = offset - line_start;
                            // Find next line
                            let next_line_start = line_end + 1;
                            let next_line_end = graphemes[next_line_start..]
                                .iter()
                                .position(|&g| g == "\n")
                                .map(|p| next_line_start + p)
                                .unwrap_or(graphemes.len());
                            let next_line_len = next_line_end - next_line_start;
                            let new_offset = next_line_start + col.min(next_line_len);
                            if modifiers.shift {
                                if self.cursor.selection_anchor().is_none() {
                                    self.cursor.set_selection_anchor(Some(offset));
                                }
                            } else {
                                self.cursor.clear_selection();
                            }
                            self.cursor.set_offset(new_offset);
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
                            self.cursor.set_offset(self.controller.grapheme_count());
                            true
                        }
                        NamedKey::Escape => {
                            self.cursor.clear_selection();
                            self.set_focused(false);
                            self.on_blur.call(self.controller.text());
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
                    let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);
                    let was_hovered = self.is_hovered();
                    if let Some(w) = get_window() {
                        if is_inside || self.mouse_held.get().is_some() {
                            w.set_cursor(winit::window::CursorIcon::Text);
                        } else {
                            w.set_cursor(winit::window::CursorIcon::Default);
                        }
                    }
                    self.set_hovered(is_inside);

                    // Drag-to-select: when mouse is held, defer position resolution to draw()
                    if owns_selection_pointer(self.mouse_held.get(), event) {
                        self.pending_click.set(Some(*pos));
                        return true;
                    }

                    was_hovered != is_inside
                }
                ElementEvent::PointerUp(_pos, _, _) => {
                    if owns_selection_pointer(self.mouse_held.get(), event) {
                        self.mouse_held.set(None);
                        true
                    } else {
                        false
                    }
                }
                ElementEvent::ImePreedit { text, cursor } => {
                    if !self.is_focused() {
                        return false;
                    }
                    // Input methods resend an identical composition while the
                    // user browses candidates; reporting no change keeps those
                    // keystrokes from repainting the window.
                    self.set_preedit(text, *cursor)
                }
                ElementEvent::Cancel => {
                    self.set_focused(false);
                    self.mouse_held.set(None);
                    self.on_blur.call(self.controller.text());
                    true
                }
                _ => false,
            }
        })();

        let result = if consumed {
            EventResult::consumed().with_redraw()
        } else {
            EventResult::ignored()
        };
        let active_after = self.mouse_held.get();
        match (event_pointer_key(event), active_before, active_after) {
            (Some(pointer), before, Some(after)) if before != Some(after) && pointer == after => {
                result.with_pointer_capture(pointer)
            }
            (Some(pointer), Some(before), None) if pointer == before => {
                result.with_pointer_release(pointer)
            }
            _ => result,
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
        ctx.canvas.translate((ol, ot).into());

        // Cache absolute bounds for hit-testing
        let (abs_x, abs_y) = {
            let (tx, ty) = ctx.canvas.get_transform_translation();
            (tx, ty)
        };

        self.cached_bounds
            .save(scale, abs_x, abs_y, box_width, box_height);

        // A field focused at construction time — `auto_focus`, or a rebuild that
        // preserved focus — never passed through `set_focused`, so make sure
        // platform text input is on before the first composition arrives. The
        // call is idempotent, so repeating it every frame is free.
        if self.enable && self.is_focused() {
            self.enable_platform_ime();
        }

        // --- Resolve active decoration ---
        let decoration = self.active_decoration();

        // --- Draw background + border + outline ---
        decoration.draw(ctx);

        // --- Padding ---
        let pad_top = self.padding.top.value(box_height, scale);
        let pad_bottom = self.padding.bottom.value(box_height, scale);
        let pad_left = self.padding.left.value(box_width, scale);
        let pad_right = self.padding.right.value(box_width, scale);

        ctx.canvas.save();
        let radii = decoration
            .border_radius
            .resolve(box_width, box_height, scale);
        let clip_radii = [
            if radii[0] > 0.0 {
                (radii[0] - pad_left.max(pad_top).min(radii[0])).max(0.0)
            } else {
                0.0
            },
            if radii[1] > 0.0 {
                (radii[1] - pad_right.max(pad_top).min(radii[1])).max(0.0)
            } else {
                0.0
            },
            if radii[2] > 0.0 {
                (radii[2] - pad_right.max(pad_bottom).min(radii[2])).max(0.0)
            } else {
                0.0
            },
            if radii[3] > 0.0 {
                (radii[3] - pad_left.max(pad_bottom).min(radii[3])).max(0.0)
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
        ctx.canvas.translate((pad_left, pad_top).into());

        let content_height = (box_height - pad_top - pad_bottom).max(0.0);
        let content_width = (box_width - pad_left - pad_right).max(0.0);

        let text = self.controller.text();
        let is_empty = text.is_empty();

        let font_size = self.scaled_font_size(&self.text_style, scale);

        // --- Process pending click (deferred from on_event for canvas access) ---
        if let Some(click_pos) = self.pending_click.take() {
            let display_for_measure = self.display_text();
            let text_width = ctx.canvas.measure_text(&display_for_measure, font_size);
            let text_x = self.align_x(text_width, content_width);

            // click_pos is in logical (unscaled) coords; abs_x/pad_left/text_x
            // are in canvas (scaled) coords. Multiply by scale to align them.
            let click_canvas_x = click_pos.x * scale;
            let rel_x = click_canvas_x - abs_x - pad_left - text_x + self.scroll_x.get();

            use unicode_segmentation::UnicodeSegmentation;
            let graphemes: Vec<&str> = if display_for_measure.is_empty() {
                vec![]
            } else {
                display_for_measure.graphemes(true).collect()
            };
            let mut click_offset = graphemes.len(); // default: past end
            if !graphemes.is_empty() {
                let mut acc_width = 0.0f32;
                for (i, g) in graphemes.iter().enumerate() {
                    let g_width = ctx.canvas.measure_text(g, font_size);
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
                    if self.mouse_held.get().is_some() && self.cursor.selection_anchor().is_none() {
                        self.cursor.set_selection_anchor(Some(click_offset));
                    }
                    self.cursor.set_offset(click_offset);
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

        // Absolute canvas origin of the content area, used to translate caret
        // positions into the logical window coordinates the IME expects.
        let content_origin = (abs_x + pad_left, abs_y + pad_top);

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

            // --- Draw cursor / composition when field is empty but focused ---
            if self.is_focused() {
                let cursor_x = self.align_x(0.0, content_width);
                let cursor_top = content_height * 0.15;
                let cursor_bottom = content_height * 0.85;
                let cursor_height = cursor_bottom - cursor_top;

                self.publish_ime_caret(
                    cursor_x,
                    cursor_top,
                    cursor_height,
                    content_origin,
                    scale,
                );

                if self.is_composing() {
                    self.with_preedit(|preedit| {
                        self.draw_preedit(
                            preedit,
                            self.preedit_cursor.get(),
                            cursor_x,
                            0.0,
                            content_height,
                            &content_ctx,
                            font_size,
                            scale,
                        );
                    });
                } else if self.cursor.is_visible() {
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
            }
        } else {
            // --- Draw text ---
            let display = self.display_text();

            let is_multiline = display.contains('\n');

            if is_multiline {
                // --- Multi-line rendering ---
                let lines: Vec<&str> = display.split('\n').collect();
                // Use real font vertical metrics (ascent + descent + line_gap)
                // instead of an approximate multiplier. The hardcoded 1.4× could
                // clip descenders when the actual line height exceeds it.
                let line_metrics = ctx.canvas.measure_text_metrics("", font_size, 0.0);
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
                    // Offsets across lines are counted in the same clusters the
                    // caret and selection use, so `char`s must not be counted
                    // here.
                    let line_graphemes = grapheme_count(line);

                    let line_width = ctx.canvas.measure_text(line, font_size);
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
                    ctx.canvas.translate((0.0, line_y).into());
                    let mut line_ctx = content_ctx.clone();
                    line_ctx.parent_size = ResolvedSize {
                        width: content_width,
                        height: line_height,
                    };
                    let line_widget =
                        self.build_text_widget(line, &self.text_style, self.text_align);
                    line_widget.draw(&line_ctx);
                    ctx.canvas.restore();

                    // Draw cursor / composition if on this line
                    if self.is_focused() {
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

                            self.publish_ime_caret(
                                cursor_x,
                                cursor_top,
                                cursor_bottom - cursor_top,
                                content_origin,
                                scale,
                            );

                            // The composition replaces the caret: drawing both
                            // would blink an insertion bar over the first
                            // composing glyph.
                            if self.is_composing() {
                                self.with_preedit(|preedit| {
                                    self.draw_preedit(
                                        preedit,
                                        self.preedit_cursor.get(),
                                        cursor_x,
                                        line_y,
                                        line_height,
                                        &content_ctx,
                                        font_size,
                                        scale,
                                    );
                                });
                            } else if self.cursor.is_visible() {
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
                    }

                    grapheme_offset += line_graphemes;
                    // Account for the '\n' character in offset counting
                    if line_idx < lines.len() - 1 {
                        grapheme_offset += 1;
                    }
                }
            } else {
                // --- Single-line rendering (with horizontal scroll) ---
                let text_width = ctx.canvas.measure_text(&display, font_size);
                let text_x = self.align_x(text_width, content_width);

                // Ensure cursor is visible
                self.ensure_cursor_visible(content_width, &ctx.canvas, font_size);
                let scroll = self.scroll_x.get();

                // Draw text — RawTextWidget handles alignment via text_align + parent_size.
                // Apply scroll by translating the canvas so the visible portion aligns.
                ctx.canvas.save();
                ctx.canvas.translate((-scroll, 0.0).into());
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

                // --- Draw cursor / IME composition ---
                if self.is_focused() {
                    let cursor_x =
                        text_x - scroll + self.cursor_x_offset_canvas(&ctx.canvas, font_size);
                    let cursor_top = content_height * 0.15;
                    let cursor_bottom = content_height * 0.85;
                    let cursor_height = cursor_bottom - cursor_top;

                    self.publish_ime_caret(
                        cursor_x,
                        cursor_top,
                        cursor_height,
                        content_origin,
                        scale,
                    );

                    // The composition replaces the caret: drawing both would
                    // blink an insertion bar over the first composing glyph.
                    if self.is_composing() {
                        self.with_preedit(|preedit| {
                            self.draw_preedit(
                                preedit,
                                self.preedit_cursor.get(),
                                cursor_x,
                                0.0,
                                content_height,
                                &content_ctx,
                                font_size,
                                scale,
                            );
                        });
                    } else if self.cursor.is_visible() {
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
                }
            }
        }

        ctx.canvas.clear_clip();
        ctx.canvas.restore(); // clip + translate
        ctx.canvas.restore(); // outer save

        // Drive the caret from the frame clock: advance the shared blink
        // timeline owned by the field state and keep the frame loop awake while
        // this field holds focus. Detached sleeping threads used to schedule the
        // next toggle, which drifted with thread wake-up latency and restarted
        // whenever the element was rebuilt.
        if self.is_focused() {
            self.cursor.blink().tick(AnimInstant::now());
            aimer_events::window::request_animation_frame();
        }
    }
}

#[cfg(test)]
mod test_support {
    use std::sync::Arc;

    use aimer_events::element::{ElementEvent, KeyAction, Modifiers};
    use aimer_style::{BoxDecoration, LayoutSpacing, Spacing, TextAlign, TextStyle};
    use aimer_widget::base::{Color, Colors};

    use super::{ExpandDirection, InputType, RawFieldConfig, RawTextField, TextFieldCallback};
    use crate::input_field::caret::CaretBlink;
    use crate::input_field::controller::TextFieldController;

    /// Builds the configuration of a focused, editable single-line field around
    /// `controller`.
    pub(super) fn field_config(controller: TextFieldController) -> RawFieldConfig {
        RawFieldConfig {
            input_type: InputType::Text,
            controller,
            prompt: Arc::from(""),
            hint: Arc::from(""),
            hint_style: TextStyle::default(),
            text_style: TextStyle::default(),
            prompt_style: TextStyle::default(),
            text_align: TextAlign::default(),
            auto_focus: true,
            max_lines: None,
            min_lines: None,
            max_length: None,
            enable: true,
            expand: ExpandDirection::default(),
            decoration: BoxDecoration::default(),
            hover_decoration: None,
            focus_decoration: None,
            disabled_decoration: None,
            selection_color: Color::Rgba(66, 133, 244, 100),
            cursor_color: Colors::default(),
            on_changed: TextFieldCallback::default(),
            on_submitted: TextFieldCallback::default(),
            on_focus: TextFieldCallback::default(),
            on_blur: TextFieldCallback::default(),
            read_only: false,
            padding: LayoutSpacing::all(Spacing::Px(4)),
        }
    }

    /// Builds a focused, editable single-line field around `controller`.
    pub(super) fn focused_field(controller: TextFieldController) -> RawTextField {
        RawTextField::new(field_config(controller), CaretBlink::new())
    }

    /// Builds a focused field that blinks on `caret`.
    pub(super) fn focused_field_with_caret(
        controller: TextFieldController,
        caret: CaretBlink,
    ) -> RawTextField {
        RawTextField::new(field_config(controller), caret)
    }

    /// A text payload delivered as one batched edit, like an IME commit.
    pub(super) fn commit(text: &str) -> ElementEvent {
        ElementEvent::TextInput {
            text: text.to_owned(),
            action: KeyAction::Pressed,
            modifiers: Modifiers::default(),
        }
    }

    /// A key press with no modifiers held.
    pub(super) fn key(key: aimer_events::element::NamedKey) -> ElementEvent {
        ElementEvent::KeyInput {
            key,
            action: KeyAction::Pressed,
            modifiers: Modifiers::default(),
        }
    }
}

#[cfg(test)]
mod ime_tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_events::element::ElementEvent;
    use aimer_widget::EventElement;

    use super::ImeCaretArea;
    use super::test_support::{commit, focused_field};
    use crate::input_field::controller::TextFieldController;
    use crate::input_field::raw_fields::TextFieldCallback;

    fn preedit(text: &str, cursor: Option<(usize, usize)>) -> ElementEvent {
        ElementEvent::ImePreedit {
            text: text.to_owned(),
            cursor,
        }
    }

    fn caret() -> ImeCaretArea {
        ImeCaretArea {
            x: 10.0,
            y: 20.0,
            width: 1.0,
            height: 16.0,
        }
    }

    #[test]
    fn committed_phrase_is_one_edit_with_one_change_notification() {
        let controller = TextFieldController::new();
        let changes = Rc::new(Cell::new(0));
        let mut field = focused_field(controller.clone());
        let counter = changes.clone();
        field.on_changed = TextFieldCallback::from(move |_: String| {
            counter.set(counter.get() + 1);
        });

        assert!(field.on_event(&commit("你好世界")).is_consumed());

        assert_eq!(controller.text(), "你好世界");
        assert_eq!(field.cursor.offset(), 4);
        assert_eq!(changes.get(), 1);
        assert!(controller.undo());
        assert_eq!(controller.text(), "");
        assert!(!controller.undo());
    }

    #[test]
    fn committed_phrase_is_inserted_at_the_cursor() {
        let controller = TextFieldController::with_initial("ab");
        let field = focused_field(controller.clone());
        field.cursor.set_offset(1);

        let _ = field.on_event(&commit("你好"));

        assert_eq!(controller.text(), "a你好b");
        assert_eq!(field.cursor.offset(), 3);
    }

    #[test]
    fn committed_phrase_replaces_the_selection() {
        let controller = TextFieldController::with_initial("abc");
        let field = focused_field(controller.clone());
        field.cursor.set_selection_anchor(Some(0));
        field.cursor.set_offset(3);

        let _ = field.on_event(&commit("你好"));

        assert_eq!(controller.text(), "你好");
        assert_eq!(field.cursor.offset(), 2);
        assert_eq!(field.cursor.selection_anchor(), None);
    }

    #[test]
    fn max_length_truncates_the_commit_instead_of_rejecting_it() {
        let controller = TextFieldController::with_initial("a");
        let mut field = focused_field(controller.clone());
        field.max_length = Some(3);
        field.cursor.set_offset(1);

        assert!(field.on_event(&commit("你好世界")).is_consumed());

        assert_eq!(controller.text(), "a你好");
        assert_eq!(field.cursor.offset(), 3);
    }

    #[test]
    fn a_full_field_ignores_a_commit() {
        let controller = TextFieldController::with_initial("ab");
        let mut field = focused_field(controller.clone());
        field.max_length = Some(2);
        field.cursor.set_offset(2);

        assert!(!field.on_event(&commit("你好")).is_consumed());

        assert_eq!(controller.text(), "ab");
    }

    #[test]
    fn read_only_and_unfocused_fields_ignore_a_commit() {
        let controller = TextFieldController::new();
        let mut read_only = focused_field(controller.clone());
        read_only.read_only = true;
        let unfocused = focused_field(controller.clone());
        unfocused.focused.set(false);

        assert!(!read_only.on_event(&commit("你好")).is_consumed());
        assert!(!unfocused.on_event(&commit("你好")).is_consumed());
        assert_eq!(controller.text(), "");
    }

    #[test]
    fn a_commit_ends_the_composition() {
        let controller = TextFieldController::new();
        let field = focused_field(controller.clone());
        let _ = field.on_event(&preedit("ni", Some((2, 2))));

        let _ = field.on_event(&commit("你"));

        assert!(!field.is_composing());
        assert_eq!(field.preedit_cursor.get(), None);
    }

    #[test]
    fn an_unchanged_preedit_is_not_consumed() {
        let field = focused_field(TextFieldController::new());

        assert!(field.on_event(&preedit("nihao", Some((5, 5)))).is_consumed());
        assert!(!field.on_event(&preedit("nihao", Some((5, 5)))).is_consumed());
        assert!(field.on_event(&preedit("nihao", Some((3, 5)))).is_consumed());
        assert!(field.is_composing());
    }

    #[test]
    fn an_empty_preedit_clears_the_composition() {
        let field = focused_field(TextFieldController::new());
        let _ = field.on_event(&preedit("ni", Some((2, 2))));

        assert!(field.on_event(&preedit("", None)).is_consumed());

        assert!(!field.is_composing());
        assert_eq!(field.preedit_cursor.get(), None);
        assert!(!field.on_event(&preedit("", None)).is_consumed());
    }

    #[test]
    fn blurring_abandons_the_composition_and_platform_input() {
        let field = focused_field(TextFieldController::new());
        let _ = field.on_event(&preedit("ni", Some((2, 2))));
        field.enable_platform_ime();
        field.update_ime_cursor_area(caret());

        let _ = field.on_event(&ElementEvent::Cancel);

        assert!(!field.is_focused());
        assert!(!field.is_composing());
        assert!(!field.ime_enabled.get());
        assert_eq!(field.ime_cursor_area.get(), None);
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

    #[test]
    fn composition_ranges_are_clamped_to_character_boundaries() {
        assert_eq!(super::floor_char_boundary("你好", 1), 0);
        assert_eq!(super::floor_char_boundary("你好", 3), 3);
        assert_eq!(super::floor_char_boundary("你好", 99), 6);
    }
}

/// Every offset the field exposes — cursor, selection, hit testing — counts
/// grapheme clusters, so editing and drawing agree on multi-code-point text.
/// These are the scripts an input method exists for, which is why they get
/// their own suite.
#[cfg(test)]
mod grapheme_tests {
    use aimer_events::element::{KeyAction, Modifiers, NamedKey};
    use aimer_widget::EventElement;

    use super::test_support::{commit, focused_field, key};
    use super::{ElementEvent, InputType};
    use crate::input_field::controller::TextFieldController;

    /// A single family emoji: five `char`s joined by zero-width joiners, one
    /// grapheme cluster.
    const FAMILY: &str = "👨‍👩‍👧";

    /// A shortcut press with the control modifier held.
    fn shortcut(letter: &str) -> ElementEvent {
        ElementEvent::KeyInput {
            key: NamedKey::Other(letter.to_owned()),
            action: KeyAction::Pressed,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn backspace_removes_a_whole_grapheme_cluster() {
        let controller = TextFieldController::with_initial(format!("a{FAMILY}"));
        let field = focused_field(controller.clone());
        field.cursor.set_offset(2);

        assert!(field.on_event(&key(NamedKey::Backspace)).is_consumed());

        assert_eq!(controller.text(), "a");
        assert_eq!(field.cursor.offset(), 1);
    }

    #[test]
    fn delete_removes_a_whole_grapheme_cluster() {
        let controller = TextFieldController::with_initial(format!("{FAMILY}a"));
        let field = focused_field(controller.clone());
        field.cursor.set_offset(0);

        assert!(field.on_event(&key(NamedKey::Delete)).is_consumed());

        assert_eq!(controller.text(), "a");
        assert_eq!(field.cursor.offset(), 0);
    }

    #[test]
    fn horizontal_movement_steps_over_whole_clusters() {
        let field = focused_field(TextFieldController::with_initial(format!("{FAMILY}a")));
        field.cursor.set_offset(0);

        let _ = field.on_event(&key(NamedKey::ArrowRight));
        assert_eq!(field.cursor.offset(), 1);
        let _ = field.on_event(&key(NamedKey::ArrowRight));
        assert_eq!(field.cursor.offset(), 2);
        // Already at the end: the offset must not run past the cluster count.
        let _ = field.on_event(&key(NamedKey::ArrowRight));
        assert_eq!(field.cursor.offset(), 2);

        let _ = field.on_event(&key(NamedKey::ArrowLeft));
        assert_eq!(field.cursor.offset(), 1);
    }

    #[test]
    fn end_and_select_all_reach_the_last_cluster() {
        let field = focused_field(TextFieldController::with_initial(format!("{FAMILY}a")));
        field.cursor.set_offset(0);

        let _ = field.on_event(&key(NamedKey::End));
        assert_eq!(field.cursor.offset(), 2);

        let _ = field.on_event(&shortcut("a"));
        assert_eq!(field.cursor.selection_anchor(), Some(0));
        assert_eq!(field.cursor.offset(), 2);
    }

    #[test]
    fn word_selection_uses_grapheme_offsets() {
        let field = focused_field(TextFieldController::with_initial(format!("{FAMILY} word")));

        // Clusters: family(0) space(1) w(2) o(3) r(4) d(5).
        field.select_word_at(2);

        assert_eq!(field.cursor.selection_anchor(), Some(2));
        assert_eq!(field.cursor.offset(), 6);
    }

    #[test]
    fn line_selection_uses_grapheme_offsets() {
        let field = focused_field(TextFieldController::with_initial(format!("{FAMILY}a\nbc")));

        // Clusters: family(0) a(1) newline(2) b(3) c(4).
        field.select_line_at(4);

        assert_eq!(field.cursor.selection_anchor(), Some(3));
        assert_eq!(field.cursor.offset(), 5);
    }

    #[test]
    fn vertical_movement_walks_lines_in_graphemes() {
        // Clusters: family(0) newline(1) a(2) b(3) c(4) d(5). The first line is
        // a single cluster made of five `char`s, so counting `char`s misplaces
        // both the current column and the target line.
        let controller = TextFieldController::with_initial(format!("{FAMILY}\nabcd"));
        let mut field = focused_field(controller);
        field.max_lines = Some(2);
        field.cursor.set_offset(6);

        // Column 4 of the second line clamps to the end of the one-cluster
        // first line.
        let _ = field.on_event(&key(NamedKey::ArrowUp));
        assert_eq!(field.cursor.offset(), 1);

        // Back down into the second line, keeping the clamped column.
        let _ = field.on_event(&key(NamedKey::ArrowDown));
        assert_eq!(field.cursor.offset(), 3);
    }

    #[test]
    fn max_length_counts_clusters_not_chars() {
        let controller = TextFieldController::with_initial("a");
        let mut field = focused_field(controller.clone());
        field.max_length = Some(2);
        field.cursor.set_offset(1);

        assert!(field.on_event(&commit(&format!("{FAMILY}{FAMILY}"))).is_consumed());

        assert_eq!(controller.text(), format!("a{FAMILY}"));
        assert_eq!(field.cursor.offset(), 2);
    }

    #[test]
    fn obscured_text_shows_one_bullet_per_cluster() {
        let mut field = focused_field(TextFieldController::with_initial(format!("{FAMILY}a")));
        field.input_type = InputType::Obscure;

        assert_eq!(field.display_text(), "••");
    }

    #[test]
    fn truncation_counts_clusters_not_chars() {
        assert_eq!(super::truncate_graphemes("你好世界", 2), "你好");
        assert_eq!(super::truncate_graphemes("你好", 9), "你好");
        assert_eq!(super::truncate_graphemes("你好", 0), "");
        // Never splits a cluster: one unit of room takes the whole family.
        assert_eq!(
            super::truncate_graphemes(&format!("{FAMILY}{FAMILY}"), 1),
            FAMILY
        );
    }
}

#[cfg(test)]
mod pointer_capture_tests {
    use aimer_attribute::Vec2d;
    use aimer_events::element::ElementEvent;
    use aimer_events::pointer::PointerSource;
    use aimer_widget::PointerKey;

    use super::owns_selection_pointer;

    #[test]
    fn selection_drag_matches_pointer_source_and_id() {
        let touch = PointerKey::new(PointerSource::Touch, 0);
        let touch_move = ElementEvent::PointerMove(Vec2d::default(), PointerSource::Touch, 0);
        let mouse_move = ElementEvent::PointerMove(Vec2d::default(), PointerSource::Mouse, 0);

        assert!(owns_selection_pointer(Some(touch), &touch_move));
        assert!(!owns_selection_pointer(Some(touch), &mouse_move));
        assert!(!owns_selection_pointer(None, &touch_move));
    }
}

#[cfg(test)]
mod caret_tests {
    use std::time::Duration;

    use aimer_animation::AnimInstant;
    use aimer_widget::EventElement;

    use super::test_support::{commit, focused_field_with_caret, key};
    use crate::input_field::caret::CaretBlink;
    use crate::input_field::controller::TextFieldController;

    const HALF: Duration = Duration::from_millis(500);

    /// Advances `caret` to the middle of its hidden half.
    fn hide(caret: &CaretBlink) {
        let start = AnimInstant::now();
        caret.tick(start);
        caret.tick(start + HALF);
    }

    #[test]
    fn caret_visibility_follows_the_shared_timeline() {
        let caret = CaretBlink::new();
        let field = focused_field_with_caret(TextFieldController::new(), caret.clone());

        assert!(field.cursor.is_visible());

        hide(&caret);

        assert!(!field.cursor.is_visible());
    }

    #[test]
    fn typing_keeps_the_caret_solid() {
        let caret = CaretBlink::new();
        let field = focused_field_with_caret(TextFieldController::new(), caret.clone());
        hide(&caret);

        assert!(field.on_event(&commit("a")).is_consumed());

        assert!(field.cursor.is_visible());
    }

    #[test]
    fn moving_the_caret_keeps_it_solid() {
        let controller = TextFieldController::with_initial("hello");
        let caret = CaretBlink::new();
        let field = focused_field_with_caret(controller, caret.clone());
        field.cursor.set_offset(5);
        hide(&caret);

        assert!(
            field
                .on_event(&key(aimer_events::element::NamedKey::ArrowLeft))
                .is_consumed()
        );

        assert!(field.cursor.is_visible());
        assert_eq!(field.cursor.offset(), 4);
    }
}
