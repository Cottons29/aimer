pub mod caret;
pub mod context_menu;
#[cfg(test)]
mod controller;
pub mod raw_fields;

use std::sync::Arc;

use aimer_style::{BoxDecoration, LayoutSpacing, TextAlign, TextStyle};
use aimer_widget::base::{BuildContext, Color, Colors};
use aimer_widget::{
    AnyElement, FocusNode, Key, State, StateUpdater, StatefulElement, StatefulWidget, Widget,
};

use crate::input_field::caret::CaretBlink;
use crate::input_field::raw_fields::{
    ExpandDirection, InputType, RawFieldConfig, RawTextFieldWidget, TextFieldCallback,
};
use crate::TextEditingController;

#[allow(dead_code)]
///
/// A configurable `TextField` widget struct that provides input capabilities
/// with an array of customizable properties for text input, styling, behavior,
/// and event handling.
///
/// # Fields
///
/// * `controller` - The `TextEditingController` instance used to control the
///   field and observe immutable editing values.
///
/// * `input_type` - Specifies the type of input allowed (e.g., text, number,
///   password). Defaults to a default implementation of `InputType`.
///
/// * `prompt` - The text prompt displayed when the `TextField` is empty. This
///   field can be initialized using types that implement `Into<String>`.
///
/// * `hint` - Hint text displayed within the `TextField` to provide user
///   guidance. Can be initialized using types implementing `Into<String>`.
///
/// * `hint_style` - Styling applied to the hint text. Defaults to a `TextStyle`
///   implementation.
///
/// * `text_style` - Styling applied to the user-inputted text. Defaults to a
///   `TextStyle` implementation.
///
/// * `prompt_style` - Styling for the text prompt. Defaults to a `TextStyle`
///   implementation.
///
/// * `text_align` - The alignment of the text within the `TextField`. Defaults
///   to a default implementation of `TextAlign`.
///
/// * `auto_focus` - Boolean indicating if the field should be automatically
///   focused upon rendering. Defaults to `false`.
///
/// * `max_length` - An optional maximum number of characters allowed in the
///   input. Defaults to `None`.
///
/// * `enable` - Indicates whether the `TextField` is enabled for interaction.
///   Defaults to `true`.
///
/// * `decoration` - The default decoration applied to the `TextField`. Defaults
///   to `BoxDecoration`.
///
/// * `hover_decoration` - The decoration applied to the `TextField` when
///   hovered. Defaults to `None`.
///
/// * `focus_decoration` - The decoration applied to the `TextField` when it
///   gains focus. Defaults to `None`.
///
/// * `disabled_decoration` - The decoration applied to the `TextField` when it
///   is disabled. Defaults to `None`.
///
/// * `cursor_color` - Color of the text cursor. Defaults to a default `Colors`
///   implementation.
///
/// * `on_changed` - Callback triggered when the input text changes. Accepts a
///   `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `on_submitted` - Callback triggered when the user submits the input (e.g.,
///   pressing Enter). Accepts a `TextFieldCallback` which is wrapped with an
///   `AsyncTextFieldCallback`.
///
/// * `on_focus` - Callback triggered when the field gains focus. Accepts a
///   `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `on_blur` - Callback triggered when the field loses focus. Accepts a
///   `TextFieldCallback` which is wrapped with an `AsyncTextFieldCallback`.
///
/// * `read_only` - When `true`, text cannot be modified via keyboard input.
///   Selection, copy, and cursor movement still work. Defaults to `false`.
///
/// Decorations are selected in disabled, focused, hovered, then normal
/// priority. The field starts empty and enabled with a white background, four
/// logical pixels of padding, and no line or length
/// limits. [`TextField::auto_focus`] controls the initial focus state when the
/// element is created.
///
/// # Example
///
/// ```
/// use aimer_input::{TextEditingController, input::{InputType, TextField}};
///
/// let controller = TextEditingController::with_text("hello");
/// let field = TextField::new().controller(controller)
///                             .input_type(InputType::Text)
///                             .hint("Message")
///                             .max_length(Some(200))
///                             .on_changed(|text| println!("changed to {text}"));
/// ```
pub struct TextField {
    controller: TextEditingController,
    pub input_type: InputType,
    pub prompt: Arc<str>,
    pub hint: Arc<str>,
    pub hint_style: TextStyle,
    pub text_style: TextStyle,
    pub prompt_style: TextStyle,
    pub text_align: TextAlign,
    focus_node: Option<FocusNode>,
    pub auto_focus: bool,
    pub max_length: Option<usize>,
    pub enable: bool,
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
    widget_key: Option<Key>,
}

impl StatefulWidget for TextField {
    type State = TextFieldState;

    fn create_state(self) -> Self::State {
        self.create_state_with_config(self.config())
    }
}

impl Widget for TextField {
    fn key(&self) -> Option<Key> {
        self.widget_key.clone()
    }

    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let __key = Widget::key(&self);
        StatefulElement::new_with_name(self, ctx, "TextField", __key)
            .0
            .boxed()
    }
}

/// The mounted state shared by [`TextField`] and [`TextArea`](crate::TextArea).
///
/// The state owns the caret blink timeline, so the caret keeps its rhythm when
/// the field is rebuilt with a new configuration — a rebuild replaces the
/// configuration through [`State::adopt_config_from`] and leaves the timeline
/// untouched. The timeline itself is advanced by the frame clock while the field
/// holds focus, which is what makes the blink interval independent of thread
/// wake-up latency.
#[doc(hidden)]
pub struct TextFieldState {
    config: RawFieldConfig,
    caret: CaretBlink,
    focus_node: FocusNode,
    provided_focus_node: bool,
    updater: StateUpdater<Self>,
}

impl TextFieldState {
    /// Returns the raw configuration for state-construction tests.
    #[cfg(test)]
    #[inline]
    pub(crate) fn config(&self) -> &RawFieldConfig {
        &self.config
    }

    /// Attaches the updater supplied by the stateful element.
    #[inline]
    pub(crate) fn init(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    /// Replaces widget configuration while retaining mounted editing state.
    #[inline]
    pub(crate) fn adopt(&mut self, new: &Self) {
        self.config = new.config.clone();
        if new.provided_focus_node {
            self.focus_node = new.focus_node.clone();
        }
        self.provided_focus_node = new.provided_focus_node;
    }

    /// Builds the raw field widget from the current shared state.
    #[inline]
    pub(crate) fn raw_widget(&self) -> RawTextFieldWidget {
        RawTextFieldWidget::new(
            self.config.clone(),
            self.caret.clone(),
            self.focus_node.clone(),
        )
    }

    /// Returns the caret blink timeline this field paints from.
    #[inline]
    pub fn caret(&self) -> &CaretBlink {
        &self.caret
    }
}

impl State<TextField> for TextFieldState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.init(updater);
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.adopt(new);
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        self.raw_widget()
    }
}

impl TextField {

    /// Creates an empty, enabled, editable field with default styling and no-op
    /// callbacks.
    pub fn new() -> Self {
        Self {
            controller: TextEditingController::default(),
            input_type: InputType::default(),
            prompt: Arc::default(),
            hint: Arc::default(),
            hint_style: TextStyle::default(),
            text_style: TextStyle::default(),
            prompt_style: TextStyle::default(),
            text_align: TextAlign::default(),
            focus_node: None,
            auto_focus: false,
            max_length: None,
            enable: true,
            decoration: BoxDecoration {
                background_color: Some(Colors::White.into()),
                ..Default::default()
            },
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
            padding: LayoutSpacing::default(),
            widget_key: None,
        }
    }

    /// Sets the identity of this field for widget reconciliation.
    ///
    /// A keyed field keeps its state — and therefore its caret timeline, focus,
    /// and selection — when the surrounding widget list is reordered.
    #[inline]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.widget_key = Some(key.into());
        self
    }

    /// Collects the configuration handed to the element on every build.
    pub(crate) fn config(&self) -> RawFieldConfig {
        RawFieldConfig {
            input_type: self.input_type,
            controller: self.controller.clone(),
            prompt: self.prompt.clone(),
            hint: self.hint.clone(),
            hint_style: self.hint_style,
            text_style: self.text_style,
            prompt_style: self.prompt_style,
            text_align: self.text_align,
            auto_focus: self.auto_focus,
            max_lines: Some(1),
            min_lines: Some(1),
            max_length: self.max_length,
            enable: self.enable,
            expand: ExpandDirection::Horizontal,
            decoration: self.decoration.clone(),
            hover_decoration: self.hover_decoration.clone(),
            focus_decoration: self.focus_decoration.clone(),
            disabled_decoration: self.disabled_decoration.clone(),
            selection_color: self.selection_color,
            cursor_color: self.cursor_color,
            on_changed: self.on_changed.clone(),
            on_submitted: self.on_submitted.clone(),
            on_focus: self.on_focus.clone(),
            on_blur: self.on_blur.clone(),
            read_only: self.read_only,
            padding: self.padding,
        }
    }

    /// Creates shared field state using `config` and this field's focus node.
    #[inline]
    pub(crate) fn create_state_with_config(&self, config: RawFieldConfig) -> TextFieldState {
        TextFieldState {
            config,
            caret: CaretBlink::new(),
            focus_node: self.focus_node.clone().unwrap_or_default(),
            provided_focus_node: self.focus_node.is_some(),
            updater: StateUpdater::empty(),
        }
    }

    /// Uses `controller` as the field's shared text and selection-history
    /// owner.
    ///
    /// Clones of the controller observe the same text, undo stack, and redo
    /// stack.
    #[inline]
    pub fn controller(mut self, controller: TextEditingController) -> Self {
        self.controller = controller;
        self
    }

    /// Sets the accepted input mode, including plain text, numeric, and
    /// obscured password input.
    #[inline]
    pub fn input_type(mut self, input_type: InputType) -> Self {
        self.input_type = input_type;
        self
    }

    /// Sets the prompt drawn when the field is empty and focused.
    #[inline]
    pub fn prompt(mut self, prompt: impl Into<Arc<str>>) -> Self {
        self.prompt = prompt.into();
        self
    }

    /// Sets the hint drawn when the field is empty and not showing its prompt.
    #[inline]
    pub fn hint(mut self, hint: impl Into<Arc<str>>) -> Self {
        self.hint = hint.into();
        self
    }

    /// Replaces the style used to lay out and paint the hint.
    #[inline]
    pub fn hint_style(mut self, hint_style: TextStyle) -> Self {
        self.hint_style = hint_style;
        self
    }

    /// Replaces the style used to lay out and paint entered text.
    #[inline]
    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }

    /// Replaces the style used to lay out and paint the focused empty prompt.
    #[inline]
    pub fn prompt_style(mut self, prompt_style: TextStyle) -> Self {
        self.prompt_style = prompt_style;
        self
    }

    /// Sets the alignment of text within the field's content area.
    #[inline]
    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    /// Attaches the handle used for imperative focus control.
    ///
    /// Retain a clone of `focus_node` to request or release this field's focus
    /// after it is mounted. The node remains attached across rebuilds.
    #[inline]
    pub fn focus_node(mut self, focus_node: FocusNode) -> Self {
        self.focus_node = Some(focus_node);
        self
    }

    /// Sets whether a newly created field starts focused.
    ///
    /// This initializes focus when the widget becomes an element; it is not an
    /// imperative request to focus an already mounted field.
    #[inline]
    pub fn auto_focus(mut self, auto_focus: bool) -> Self {
        self.auto_focus = auto_focus;
        self
    }


    /// Sets the optional maximum input length in Unicode scalar values.
    ///
    /// `None` removes the limit. Input beyond the limit is not inserted.
    #[inline]
    pub fn max_length(mut self, max_length: Option<usize>) -> Self {
        self.max_length = max_length;
        self
    }

    /// Enables or disables focus, editing, selection, and input callbacks.
    ///
    /// A disabled field uses its configured disabled decoration when present.
    #[inline]
    pub fn enable(mut self, enable: bool) -> Self {
        self.enable = enable;
        self
    }


    /// Replaces the normal field decoration.
    #[inline]
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    /// Sets the decoration used while an enabled, unfocused field is hovered.
    #[inline]
    pub fn hover_decoration(mut self, hover_decoration: BoxDecoration) -> Self {
        self.hover_decoration = Some(hover_decoration);
        self
    }

    /// Sets the decoration used while the enabled field is focused.
    ///
    /// Focus decoration takes precedence over hover decoration.
    #[inline]
    pub fn focus_decoration(mut self, focus_decoration: BoxDecoration) -> Self {
        self.focus_decoration = Some(focus_decoration);
        self
    }

    /// Sets the decoration used while the field is disabled.
    ///
    /// Disabled decoration takes precedence over focus and hover decorations.
    #[inline]
    pub fn disabled_decoration(mut self, disabled_decoration: BoxDecoration) -> Self {
        self.disabled_decoration = Some(disabled_decoration);
        self
    }

    /// Sets the color painted behind selected text.
    #[inline]
    pub fn selection_color(mut self, selection_color: impl Into<Color>) -> Self {
        self.selection_color = selection_color.into();
        self
    }

    /// Sets the color of the insertion cursor.
    #[inline]
    pub fn cursor_color(mut self, cursor_color: Colors) -> Self {
        self.cursor_color = cursor_color;
        self
    }

    /// Sets the callback invoked after a user edit changes the text.
    ///
    /// The callback receives the complete updated string. Programmatic
    /// controller mutations do not themselves dispatch widget callbacks.
    #[inline]
    pub fn on_changed(mut self, on_changed: impl Into<TextFieldCallback>) -> Self {
        self.on_changed = on_changed.into();
        self
    }

    /// Sets the callback invoked when the user submits the field.
    ///
    /// The callback receives the current complete string.
    #[inline]
    pub fn on_submitted(mut self, on_submitted: impl Into<TextFieldCallback>) -> Self {
        self.on_submitted = on_submitted.into();
        self
    }

    /// Sets the callback invoked when the field gains focus.
    ///
    /// The callback receives the current complete string.
    #[inline]
    pub fn on_focus(mut self, on_focus: impl Into<TextFieldCallback>) -> Self {
        self.on_focus = on_focus.into();
        self
    }

    /// Sets the callback invoked when the field loses focus.
    ///
    /// The callback receives the current complete string.
    #[inline]
    pub fn on_blur(mut self, on_blur: impl Into<TextFieldCallback>) -> Self {
        self.on_blur = on_blur.into();
        self
    }

    /// Sets whether user editing is blocked while focus, selection, copy, and
    /// navigation remain.
    #[inline]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Sets the spacing between the field decoration and its text content.
    #[inline]
    pub fn padding(mut self, padding: impl Into<LayoutSpacing>) -> Self {
        self.padding = padding.into();
        self
    }
}

impl Default for TextField {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use aimer_animation::AnimInstant;

    use super::*;

    const HALF: Duration = Duration::from_millis(500);

    #[test]
    fn explicit_key_sets_reconciliation_identity() {
        let field = TextField::new().key("login-field");

        assert_eq!(Widget::key(&field), Some(Key::Value("login-field".to_owned())));
    }

    #[test]
    fn created_state_carries_the_widget_configuration() {
        let controller = TextEditingController::with_text("hello");
        let state = TextField::new()
            .controller(controller.clone())
            .hint("Message")
            .max_length(Some(7))
            .read_only(true)
            .create_state();

        assert_eq!(&*state.config.hint, "Message");
        assert_eq!(state.config.max_length, Some(7));
        assert!(state.config.read_only);
        assert_eq!(state.config.controller.value().text(), "hello");
    }

    #[test]
    fn text_field_is_always_single_line_and_number_is_only_an_input_hint() {
        let state = TextField::new().input_type(InputType::Number).create_state();

        assert_eq!(state.config.input_type, InputType::Number);
        assert_eq!(state.config.min_lines, Some(1));
        assert_eq!(state.config.max_lines, Some(1));
        assert_eq!(state.config.expand, ExpandDirection::Horizontal);
    }

    #[test]
    fn created_state_starts_with_a_visible_caret() {
        let state = TextField::new().create_state();

        assert!(state.caret().is_visible());
        assert_eq!(state.caret().period(), CaretBlink::DEFAULT_PERIOD);
    }

    #[test]
    fn adopting_a_configuration_keeps_the_caret_phase() {
        let mut state = TextField::new().hint("before").create_state();
        let start = AnimInstant::now();
        state.caret().tick(start);
        state.caret().tick(start + HALF);
        assert!(!state.caret().is_visible());

        let rebuilt = TextField::new().hint("after").create_state();
        State::<TextField>::adopt_config_from(&mut state, &rebuilt);

        assert_eq!(&*state.config.hint, "after");
        assert!(!state.caret().is_visible());
    }

    #[test]
    fn size_of_text_field() {
        eprintln!("Size Of TextField: {}", size_of::<TextField>())

    }

    #[test]
    fn adopting_a_configuration_does_not_restart_the_blink() {
        let mut state = TextField::new().create_state();
        let start = AnimInstant::now();
        state.caret().tick(start);

        State::<TextField>::adopt_config_from(&mut state, &TextField::new().create_state());

        assert!(state.caret().tick(start + HALF));
        assert!(!state.caret().is_visible());
    }
}
