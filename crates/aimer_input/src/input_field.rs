pub mod caret;
pub mod context_menu;
#[cfg(test)]
mod controller;
pub mod raw_fields;

use std::sync::Arc;

use aimer_style::{BoxDecoration, LayoutSpacing, TextAlign, TextStyle};
use aimer_widget::base::{BuildContext, Color, Colors};
use aimer_widget::{
    AnyElement, FocusBehavior, Focusable, FocusNode, Key, State, StateUpdater, StatefulElement,
    StatefulWidget, Widget,
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
    pub(crate) fn adopt(&mut self, new: Self) {
        self.config = new.config;
        if new.provided_focus_node {
            self.focus_node = new.focus_node;
        }
        self.provided_focus_node = new.provided_focus_node;
    }

    /// Builds the raw field widget from the current shared state.
    ///
    /// Only [`TextFieldState::focusable_field`] mounts this: the raw element is
    /// never a focus target on its own, so nothing outside builds it bare.
    #[inline]
    fn raw_widget(&self) -> RawTextFieldWidget {
        RawTextFieldWidget::new(
            self.config.clone(),
            self.caret.clone(),
            self.focus_node.clone(),
        )
    }

    /// Reads how the field takes part in focus off its configuration.
    ///
    /// A disabled field is not a target at all — not by a press, not by `Tab`,
    /// and a press on it does not take focus from whoever holds it — which is
    /// [`FocusBehavior::Ignore`]. An enabled field asking to start focused is
    /// [`FocusBehavior::Auto`], and an ordinary enabled field is focused when it
    /// is pressed.
    ///
    /// Both inputs come from the widget configuration, so they only change when
    /// the field is rebuilt with a new one: a static behavior says all of this,
    /// and no [`Focusable::focusable_when`] gate is needed to re-ask per
    /// traversal.
    #[inline]
    fn focus_behavior(&self) -> FocusBehavior {
        match (self.config.enable, self.config.auto_focus) {
            (false, _) => FocusBehavior::Ignore,
            (true, true) => FocusBehavior::Auto,
            (true, false) => FocusBehavior::OnPress,
        }
    }

    /// Builds the field as a focus target of the standard focus mechanism.
    ///
    /// The state keeps owning the node and only lends it to the region, so the
    /// handle handed out by [`TextField::focus_node`] still drives this field:
    /// `request_focus()` and `unfocus()` on it move the keyboard exactly as they
    /// did while the field reported the node itself.
    ///
    /// The region wraps the raw field, which paints its own decoration and
    /// padding, so the area a press has to land in to focus the field is exactly
    /// the area the field covers.
    #[inline]
    pub(crate) fn focusable_field(&self) -> Focusable<RawTextFieldWidget> {
        Focusable::new()
            .node(self.focus_node.clone())
            .behavior(self.focus_behavior())
            .child(self.raw_widget())
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

    fn adopt_config_from(&mut self, new: Self) {
        self.adopt(new);
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        self.focusable_field()
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
        State::<TextField>::adopt_config_from(&mut state, rebuilt);

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

        State::<TextField>::adopt_config_from(&mut state, TextField::new().create_state());

        assert!(state.caret().tick(start + HALF));
        assert!(!state.caret().is_visible());
    }
}

#[cfg(test)]
mod focus_tests {
    //! What focus has to do for a field, asserted on the whole widget.
    //!
    //! A field is focused by a press, focused and blurred by code through the
    //! node an application supplies, and claims the keyboard on arrival when it
    //! autofocuses; while it owns focus, typed text and committed phrases reach
    //! its controller. None of that depends on *which* element of the field
    //! offers the focus node, so these tests drive the built widget the way the
    //! framework does — build the element, let the dispatcher resolve focus,
    //! read the controller — rather than poking the raw element, and they hold
    //! whether the offer is made by the field itself or by an enclosing
    //! [`Focusable`] region.
    //!
    //! [`Focusable`]: aimer_widget::Focusable

    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_attribute::position::Vec2d;
    use aimer_events::element::{ElementEvent, KeyAction, Modifiers};
    use aimer_events::pointer::{PointerButton, PointerInfo};
    use aimer_widget::{
        AnyElement, Drawable, Element, EventDispatcher, FocusNode, VisitorElement,
    };

    use super::*;
    use crate::input_field::raw_fields::test_support::dummy_build_context;

    /// The extent the field is built and drawn at.
    const FIELD_WIDTH: f32 = 200.0;
    const FIELD_HEIGHT: f32 = 40.0;

    /// A point the drawn field covers.
    const INSIDE: Vec2d = Vec2d { x: 20.0, y: 12.0 };

    /// Builds `field` and draws it once, so the element has the bounds a press
    /// is resolved against.
    ///
    /// Nothing reaches a GPU; the draw exists because the field caches its
    /// hit-test rectangle while painting, which is where a real frame puts it.
    fn drawn(field: TextField) -> (AnyElement, BuildContext<'static>) {
        let ctx = dummy_build_context(FIELD_WIDTH, FIELD_HEIGHT);
        let element = field.to_element(&ctx);
        element.draw(&ctx);
        (element, ctx)
    }

    /// Presses at `pos`, the way a mouse would.
    fn press(dispatcher: &mut EventDispatcher, root: &AnyElement, pos: Vec2d) {
        let _ = dispatcher.dispatch(
            root.as_ref(),
            pos,
            &ElementEvent::PointerDown(PointerInfo::mouse(pos, PointerButton::Primary)),
        );
    }

    /// Types `text` the way a platform input method commits a phrase.
    fn commit(dispatcher: &mut EventDispatcher, root: &AnyElement, text: &str) {
        let _ = dispatcher.dispatch(
            root.as_ref(),
            INSIDE,
            &ElementEvent::TextInput {
                text: text.to_owned(),
                action: KeyAction::Pressed,
                modifiers: Modifiers::default(),
            },
        );
    }

    /// Pumps one frame of focus resolution without touching the field.
    ///
    /// Focus is settled while an event is dispatched, so a node that asked for
    /// focus — or a region that autofocuses — only becomes the owner once
    /// something is delivered.
    fn settle(dispatcher: &mut EventDispatcher, root: &AnyElement) {
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);
    }

    /// Names every element of `root` that offers itself as a focus target.
    fn focus_targets(root: &AnyElement) -> Vec<&'static str> {
        fn walk(element: &dyn Element, names: &mut Vec<&'static str>) {
            if element.focus_node().is_some() {
                names.push(VisitorElement::debug_name(element));
            }
            element.visit_children(&mut |child| walk(child, names));
        }

        let mut names = Vec::new();
        walk(root.as_ref(), &mut names);
        names
    }

    /// A field is one focus target, offered by one element of it.
    ///
    /// The same node offered at two depths would be gathered twice in a single
    /// traversal, and every rule that reads tree order — `Tab` above all — would
    /// then have to guess which of the two it meant. The element that makes the
    /// offer is named here so that moving the offer stays a deliberate change:
    /// wrapping the field in [`Focusable`] turns this into `["Focusable"]`, and
    /// the field itself must stop offering the node in the same breath.
    ///
    /// [`Focusable`]: aimer_widget::Focusable
    #[test]
    fn the_field_is_offered_exactly_once_as_a_focus_target() {
        let (element, _ctx) = drawn(TextField::new());

        assert_eq!(focus_targets(&element), ["Focusable"]);
    }

    /// A press on the field focuses it, and what is typed afterwards reaches
    /// the controller.
    ///
    /// This is the pair a field lives by, and the press half is the fragile
    /// one: the field consumes the press that lands on it, so whatever offers
    /// its focus node has to be reached by that same press.
    #[test]
    fn a_press_focuses_the_field_and_typed_text_reaches_its_controller() {
        let controller = TextEditingController::new();
        let node = FocusNode::new();
        let (element, _ctx) = drawn(
            TextField::new()
                .controller(controller.clone())
                .focus_node(node.clone()),
        );
        let mut dispatcher = EventDispatcher::new();

        press(&mut dispatcher, &element, INSIDE);
        assert!(node.has_focus(), "a press must focus the field it landed on");

        commit(&mut dispatcher, &element, "你好");

        assert_eq!(controller.text(), "你好");
    }

    /// Pressing a field that already holds focus changes nothing about its
    /// focus.
    ///
    /// Every click inside a field is a caret placement, and there are many of
    /// them in a row while text is being edited. If such a click blurred the
    /// field before focusing it again, each one would report a focus edge that
    /// did not happen — `on_blur` then `on_focus` — and, worse, the blur tears
    /// down the platform input method mid-composition, so a phrase being
    /// composed would be dropped by a click meant to move the caret. So the
    /// second press must leave the counts where the first one put them.
    #[test]
    fn pressing_a_field_that_already_holds_focus_reports_no_further_edge() {
        let node = FocusNode::new();
        let focuses = Rc::new(Cell::new(0));
        let blurs = Rc::new(Cell::new(0));
        let (element, _ctx) = drawn(
            TextField::new()
                .focus_node(node.clone())
                .on_focus({
                    let focuses = Rc::clone(&focuses);
                    move |_: String| focuses.set(focuses.get() + 1)
                })
                .on_blur({
                    let blurs = Rc::clone(&blurs);
                    move |_: String| blurs.set(blurs.get() + 1)
                }),
        );
        let mut dispatcher = EventDispatcher::new();

        press(&mut dispatcher, &element, INSIDE);
        assert_eq!((focuses.get(), blurs.get()), (1, 0), "the first press focuses");

        press(&mut dispatcher, &element, INSIDE);
        press(&mut dispatcher, &element, INSIDE);

        assert!(node.has_focus(), "the field keeps the focus it was given");
        assert_eq!((focuses.get(), blurs.get()), (1, 0));
    }

    /// A disabled field is not a target at all: pressing it neither focuses it
    /// nor takes focus from whoever holds it.
    #[test]
    fn a_disabled_field_is_not_a_focus_target() {
        let node = FocusNode::new();
        let (element, _ctx) = drawn(TextField::new().enable(false).focus_node(node.clone()));
        let mut dispatcher = EventDispatcher::new();

        press(&mut dispatcher, &element, INSIDE);

        assert!(!node.has_focus());
        assert!(focus_targets(&element).is_empty());
    }

    /// An autofocusing field claims the keyboard as soon as it is in the tree,
    /// without anybody pointing at it.
    #[test]
    fn an_auto_focus_field_claims_focus_when_it_appears() {
        let node = FocusNode::new();
        let (element, _ctx) = drawn(TextField::new().auto_focus(true).focus_node(node.clone()));
        let mut dispatcher = EventDispatcher::new();

        settle(&mut dispatcher, &element);

        assert!(node.has_focus());
    }

    /// The node stays the application's to drive: [`TextField::focus_node`]
    /// hands out a handle, and focusing or blurring it moves the keyboard
    /// without anything being pressed.
    ///
    /// Each edge is also reported exactly once. The callbacks fire on the focus
    /// notifications the field is delivered, so a notification repeated while
    /// nothing changed — a frame that resolves focus again, an offer made twice
    /// — would call them again.
    #[test]
    fn a_supplied_node_focuses_and_blurs_the_field_and_reports_each_edge_once() {
        let controller = TextEditingController::new();
        let node = FocusNode::new();
        let focuses = Rc::new(Cell::new(0));
        let blurs = Rc::new(Cell::new(0));
        let (element, _ctx) = drawn(
            TextField::new()
                .controller(controller.clone())
                .focus_node(node.clone())
                .on_focus({
                    let focuses = Rc::clone(&focuses);
                    move |_: String| focuses.set(focuses.get() + 1)
                })
                .on_blur({
                    let blurs = Rc::clone(&blurs);
                    move |_: String| blurs.set(blurs.get() + 1)
                }),
        );
        let mut dispatcher = EventDispatcher::new();

        node.request_focus();
        settle(&mut dispatcher, &element);
        settle(&mut dispatcher, &element);
        commit(&mut dispatcher, &element, "hi");
        node.unfocus();
        settle(&mut dispatcher, &element);
        settle(&mut dispatcher, &element);

        assert_eq!(controller.text(), "hi");
        assert_eq!((focuses.get(), blurs.get()), (1, 1));
        assert!(!node.has_focus());
    }
}
