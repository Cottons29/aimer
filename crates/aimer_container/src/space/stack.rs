use aimer_attribute::BoxConstraint;
use aimer_macro::{EventElement, LayoutElement, Rebuildable};
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyWidget, Drawable, Element, LayoutElement, VisitorElement, Widget};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
    #[default]
    Normal,
    Reverse,
    Inherit,
}
/// Paints children on top of one another in the same constrained area.
///
/// Every child receives the stack's content size and constraints. Before
/// painting, children are sorted by their [`Widget`] element layer; the default
/// [`StackDirection::Normal`] paints lower layers first, while
/// [`StackDirection::Reverse`] reverses that order. `Inherit` currently behaves
/// like `Normal`.
///
/// `Stack::new()` is an empty, valid widget. [`Stack::children`] replaces the
/// collection with homogeneous values, while [`Stack::add_child`] appends and
/// boxes values so different concrete widget types can be mixed.
///
/// # Example
///
/// ```rust
/// use aimer_container::{Align, Alignment, SizedBox, Stack};
///
/// let stack = Stack::new()
///     .add_child(SizedBox::new().width(200).height(120))
///     .add_child(
///         Align::new()
///             .alignment(Alignment::MidCenter)
///             .child(SizedBox::new().width(40).height(40)),
///     );
/// ```
pub struct Stack<W = AnyWidget> {
    pub children: Vec<W>,
    pub direction: StackDirection,
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Stack {
    /// Creates an empty stack in [`StackDirection::Normal`] painting order.
    ///
    /// The empty stack is already a valid [`Widget`].
    pub fn new() -> Self {
        Self { children: Vec::new(), direction: StackDirection::default() }
    }

    /// Replaces all children with a homogeneous collection.
    ///
    /// This is not an append operation. The returned [`Stack`] adopts the
    /// iterator's item type; callers that need it to satisfy the current
    /// concrete [`Widget`] implementation should supply erased [`AnyWidget`]
    /// values, or use [`Stack::add_child`] instead.
    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Stack<W> {
        Stack {
            children: children
                .into_iter()
                .collect(),
            direction: self.direction,
        }
    }

    /// Appends a child, boxing it into the stack's erased collection.
    ///
    /// Existing children are retained, and successive calls may use different
    /// concrete widget types.
    pub fn add_child(mut self, child: impl Widget + 'static) -> Self {
        self.children
            .push(Box::new(child));
        self
    }

    /// Sets the layer-sorted painting order.
    ///
    /// The default is [`StackDirection::Normal`]. Reverse order affects
    /// painting only; it does not change layout constraints or child storage.
    pub fn direction(mut self, direction: StackDirection) -> Self {
        self.direction = direction;
        self
    }
}

impl Widget for Stack {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self
            .children
            .iter()
            .map(|c| c.to_element(ctx))
            .collect();
        Box::new(RawStackElement { children, direction: self.direction })
    }
}

#[derive(Rebuildable, LayoutElement, EventElement)]
pub struct RawStackElement {
    pub children: Vec<Box<dyn Element>>,
    pub direction: StackDirection,
}

impl Drawable for RawStackElement {
    fn draw(&self, ctx: &BuildContext) {
        let content_size = self.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content_size,
            canvas: ctx
                .canvas
                .clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content_size.width,
                max_height: content_size.height,
            },
            visible_rect: ctx.visible_rect,
            window: ctx
                .window
                .clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx
                .async_handle
                .clone(),
            inherited_states: ctx
                .inherited_states
                .clone(),
        };

        let mut sorted_children: Vec<_> = self
            .children
            .iter()
            .collect();

        sorted_children.sort_by_key(|child| child.layer());

        if self.direction == StackDirection::Reverse {
            for child in sorted_children
                .iter()
                .rev()
            {
                child.draw(&child_ctx);
            }
        } else {
            for child in sorted_children {
                child.draw(&child_ctx);
            }
        }
    }
}

impl VisitorElement for RawStackElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }

    fn debug_name(&self) -> &'static str {
        "RawStackElement"
    }
}
