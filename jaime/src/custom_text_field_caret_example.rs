//! A themed text field with a retained custom insertion caret.

use aimer::style::*;
use aimer::*;

use crate::theme;

/// Starts the custom text-field caret showcase.
pub fn start_custom_text_field_caret_example() {
    AimerApp::start(theme::provide(CustomTextFieldCaretExample::new().boxed()))
}

/// Demonstrates a themed [`TextField`] with a custom caret widget.
pub struct CustomTextFieldCaretExample {
    controller: TextEditingController,
}

impl CustomTextFieldCaretExample {
    /// Creates the showcase with text positioned around the initial caret.
    #[inline]
    pub fn new() -> Self {
        Self {
            controller: TextEditingController::with_text("Move the caret around"),
        }
    }
}

impl Default for CustomTextFieldCaretExample {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for CustomTextFieldCaretExample {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let theme = ThemeData::copied(ctx);
        let text_style = TextStyle::new().font_size(16).color(theme.on_surface_color);
        let caret_color = theme.primary_color;

        Container::new()
            .color(theme.background_color)
            .padding(LayoutSpacing::all(Spacing::Px(32)))
            .child(
                Column::new().children(vec![
                    Text::new("Custom TextField caret")
                        .text_style(
                            TextStyle::new()
                                .font_size(28)
                                .font_weight(FontWeight::Bold)
                                .color(theme.on_background_color),
                        )
                        .boxed(),
                    Text::new("The accent caret is retained while the field edits and rebuilds.")
                        .text_style(TextStyle::new().font_size(16).color(theme::muted_text(&theme)))
                        .boxed(),
                    TextField::new()
                        .controller(self.controller)
                        .input_type(InputType::Text)
                        .hint("Type, select, and move the caret")
                        .hint_style(text_style.color(theme::muted_text(&theme)))
                        .text_style(text_style)
                        .max_length(Some(120))
                        .decoration(theme::input_decoration(&theme))
                        .hover_decoration(theme::input_hover_decoration(&theme))
                        .focus_decoration(theme::input_focus_decoration(&theme))
                        .cursor_color(theme::input_cursor_color(&theme))
                        .selection_color(theme.primary_color.with_alpha(0.28))
                        .caret(move |context| ThemedCaret::new(context, caret_color))
                        .padding(LayoutSpacing::all(Spacing::Px(12)))
                        .boxed(),
                    SizedBox::new().height(16).boxed(),
                    Text::new("Focus the field to see the rounded caret blink with the app accent.")
                        .text_style(TextStyle::new().font_size(14).color(theme::muted_text(&theme)))
                        .boxed(),
                ]),
            )
            .to_element(ctx)
    }

    fn debug_name(&self) -> &'static str {
        "CustomTextFieldCaretExample"
    }
}

impl aimer::PortableWidget for CustomTextFieldCaretExample {}

struct ThemedCaret {
    context: CaretContext,
    color: Color,
}

impl ThemedCaret {
    #[inline]
    fn new(context: CaretContext, color: Color) -> Self {
        Self { context, color }
    }
}

impl aimer::PortableWidget for ThemedCaret {}

impl Widget for ThemedCaret {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        ThemedCaretElement {
            context: self.context,
            color: self.color,
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "ThemedCaret"
    }
}

struct ThemedCaretElement {
    context: CaretContext,
    color: Color,
}

impl Drawable for ThemedCaretElement {
    fn draw(&self, ctx: &BuildContext) {
        if !self.context.is_visible() {
            return;
        }

        let width = ctx.parent_size.width.max(2.0);
        let height = ctx.parent_size.height;
        if height <= 0.0 {
            return;
        }

        let color = if self.context.is_composing() {
            self.color.lighten(0.18)
        } else {
            self.color
        };

        ctx.canvas.fill_color_rect(
            (0.0, 0.0).into(),
            ResolvedSize { width, height },
            color,
            [width / 2.0; 4],
        );
    }
}

impl VisitorElement for ThemedCaretElement {
    fn debug_name(&self) -> &'static str {
        "ThemedCaret"
    }
}

impl EventElement for ThemedCaretElement {}
impl LayoutElement for ThemedCaretElement {}
impl Rebuildable for ThemedCaretElement {}
