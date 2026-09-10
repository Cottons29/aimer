/// Owns the one caret child for a mounted text field.
///
/// `TextFieldState` rebuilds its raw widget as configuration changes. Keeping
/// this slot in the state means a custom caret factory runs once, while the
/// resulting element is handed to each candidate tree through a
/// [`ChildBuilder`]. The mode bit allows `builtin_caret` to replace a custom
/// caret without exposing the caret to the field's editing state.
///
/// [`ChildBuilder`]: aimer_widget::ChildBuilder
pub(crate) struct CaretSlot {
    child: RefCell<Option<ChildBuilder>>,
    custom: Cell<Option<bool>>,
    builtin_color: Rc<Cell<Color>>,
}

impl CaretSlot {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            child: RefCell::new(None),
            custom: Cell::new(None),
            builtin_color: Rc::new(Cell::new(Color::default())),
        }
    }

    #[inline]
    pub(crate) fn build(
        &self,
        builder: Option<&CaretBuilder>,
        context: CaretContext,
        color: Colors,
        ctx: &BuildContext,
    ) -> AnyElement {
        let custom = builder.is_some();
        self.builtin_color.set(color.into());
        let mut child = self.child.borrow_mut();
        if self.custom.get() != Some(custom) {
            let widget = builder
                .map(|builder| builder.build(context.clone()))
                .unwrap_or_else(|| {
                    DefaultCaret::with_color_cell(context, self.builtin_color.clone()).boxed()
                });
            let retained = ChildBuilder::from_widget(widget);
            let element = retained.build(ctx);
            *child = Some(retained);
            self.custom.set(Some(custom));
            element
        } else {
            child
                .as_ref()
                .expect("a caret slot's mode must have a matching child")
                .build(ctx)
        }
    }
}

/// Retains the field's interaction element and its independently built caret
/// widget as sibling children.
///
/// The field computes the caret rectangle while it paints text. The host then
/// translates the retained caret element to that rectangle without exposing the
/// caret to pointer hit testing or focus traversal.
pub(crate) struct RawTextFieldHost {
    pub(crate) field: RawTextField,
    pub(crate) caret: AnyElement,
}

impl RawTextFieldHost {
    #[inline]
    pub(crate) fn new(field: RawTextField, caret: AnyElement) -> Self {
        Self { field, caret }
    }
}

impl Drawable for RawTextFieldHost {
    fn draw(&self, ctx: &BuildContext) {
        self.field.draw(ctx);

        let context = self.field.caret_context();
        if !context.is_focused() || !context.is_available() {
            return;
        }

        let geometry = context.geometry();
        let scale = if ctx.scale > 0.0 { ctx.scale } else { 1.0 };
        let offset = Vec2d {
            x: geometry.x * scale,
            y: geometry.y * scale,
        };
        let size = ResolvedSize {
            width: geometry.width * scale,
            height: geometry.height * scale,
        };

        ctx.canvas.save();
        ctx.canvas.translate(offset);

        let mut caret_ctx = ctx.clone();
        caret_ctx.parent_size = size;
        caret_ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: size.width,
            max_height: size.height,
        };
        caret_ctx.visible_rect = ctx.visible_rect.map(|(x, y, width, height)| {
            (x - offset.x, y - offset.y, width, height)
        });
        self.caret.draw(&caret_ctx);

        ctx.canvas.restore();
    }
}

impl VisitorElement for RawTextFieldHost {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.field);
        visitor(&self.caret);
    }

    fn debug_name(&self) -> &'static str {
        "TextField"
    }
}

impl EventElement for RawTextFieldHost {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.field);
    }

    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.field);
        visitor(&self.caret);
    }

    fn focus_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.field);
    }

    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.field);
    }
}

impl LayoutElement for RawTextFieldHost {
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.field.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.field.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.field.content_size(ctx)
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.field.pos_start_end()
    }
}

impl Rebuildable for RawTextFieldHost {}

#[cfg(test)]
mod tests {
    use super::*;
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_cupid::utilities::Rgba8;
    use crate::input_field::caret::{CaretBlink, CaretGeometry};
    use crate::input_field::raw_fields::test_support::{dummy_build_context, field_config};

    struct ProbeCaret(Rc<Cell<(f32, f32)>>);

    impl aimer_widget::PortableWidget for ProbeCaret {}

    impl Widget for ProbeCaret {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            ProbeCaretElement(self.0).boxed()
        }
    }

    struct ProbeCaretElement(Rc<Cell<(f32, f32)>>);

    impl Drawable for ProbeCaretElement {
        fn draw(&self, ctx: &BuildContext) {
            self.0
                .set((ctx.parent_size.width, ctx.parent_size.height));
        }
    }

    impl VisitorElement for ProbeCaretElement {
        fn debug_name(&self) -> &'static str {
            "ProbeCaret"
        }
    }

    impl EventElement for ProbeCaretElement {}
    impl LayoutElement for ProbeCaretElement {}
    impl Rebuildable for ProbeCaretElement {}

    #[test]
    fn retained_builtin_caret_tracks_updated_cursor_color() {
        let slot = CaretSlot::new();
        let context = dummy_build_context(240.0, 60.0);
        let caret_context = CaretContext::new(CaretBlink::new());
        caret_context.publish(
            CaretGeometry {
                width: 1.5,
                height: 16.0,
                ..CaretGeometry::default()
            },
            0,
            true,
            true,
            false,
        );

        let caret = slot.build(
            None,
            caret_context.clone(),
            Colors::Red,
            &context,
        );
        caret.draw(&context);
        let first_color = context
            .canvas
            .get_inner_canvas()
            .take_draw_list()
            .commands()
            .iter()
            .rev()
            .find_map(|command| match command {
                DrawCommand::FillRect { color, .. } => Some(color.to_rgba8().0),
                _ => None,
            });
        assert_eq!(first_color, Some(Rgba8::from_argb(Colors::Red.as_u32()).0));

        let caret = slot.build(
            None,
            caret_context,
            Colors::Blue,
            &context,
        );
        caret.draw(&context);
        let second_color = context
            .canvas
            .get_inner_canvas()
            .take_draw_list()
            .commands()
            .iter()
            .rev()
            .find_map(|command| match command {
                DrawCommand::FillRect { color, .. } => Some(color.to_rgba8().0),
                _ => None,
            });
        assert_eq!(second_color, Some(Rgba8::from_argb(Colors::Blue.as_u32()).0));
    }

    #[test]
    fn host_positions_the_retained_caret_at_the_published_geometry() {
        let field = RawTextField::new(
            field_config(TextEditingController::new()),
            CaretBlink::new(),
            FocusNode::new(),
        );
        let _ = field.on_event(&ElementEvent::FocusGained);
        let caret_context = field.caret_context();
        let size = Rc::new(Cell::new((0.0, 0.0)));
        let context = dummy_build_context(240.0, 60.0);
        let caret = ProbeCaret(size.clone()).to_element(&context);
        let host = RawTextFieldHost::new(field, caret);

        host.draw(&context);

        let (width, height) = size.get();
        assert_eq!(width, 1.5);
        assert!(height > 0.0);
        assert!(caret_context.is_focused());
        assert_eq!(caret_context.offset(), 0);
        assert_eq!(caret_context.geometry().x, 4.0);
        assert!(caret_context.geometry().y > 4.0);
        assert_eq!(caret_context.geometry().width, 1.5);
        assert!(caret_context.geometry().height > 0.0);
    }
}
