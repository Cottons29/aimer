use aimer_attribute::BoxConstraint;
use aimer_macro::{LayoutElement, Rebuildable};
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement, VisitorElement, Widget,
};

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
/// use aimer_container::SizedBox;
/// use aimer_space::{Align, Alignment, Stack};
///
/// let stack = Stack::new().add_child(SizedBox::new().width(200).height(120))
///                         .add_child(Align::new().alignment(Alignment::MidCenter)
///                                                .child(SizedBox::new().width(40).height(40)));
/// ```
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_space::space::Stack", schema_only)]
pub struct Stack<W = AnyWidget> {
    #[portable_children]
    pub children: Vec<W>,
    #[portable_skip]
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
    #[inline]
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            direction: StackDirection::default(),
        }
    }

    /// Replaces all children with a homogeneous collection.
    ///
    /// This is not an append operation. The returned [`Stack`] adopts the
    /// iterator's item type; callers that need it to satisfy the current
    /// concrete [`Widget`] implementation should supply erased [`AnyWidget`]
    /// values, or use [`Stack::add_child`] instead.
    #[inline]
    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Stack<W> {
        Stack {
            children: children.into_iter().collect(),
            direction: self.direction,
        }
    }

    /// Appends a child, boxing it into the stack's erased collection.
    ///
    /// Existing children are retained, and successive calls may use different
    /// concrete widget types.
    #[inline]
    pub fn add_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(child.boxed());
        self
    }

    /// Sets the layer-sorted painting order.
    ///
    /// The default is [`StackDirection::Normal`]. Reverse order affects
    /// painting only; it does not change layout constraints or child storage.
    #[inline]
    pub fn direction(mut self, direction: StackDirection) -> Self {
        self.direction = direction;
        self
    }
}

impl Widget for Stack {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let children = self.children.into_iter().map(|c| c.to_element(ctx)).collect();
        RawStackElement {
            children,
            direction: self.direction,
        }
        .boxed()
    }
}

#[derive(Rebuildable, LayoutElement)]
pub struct RawStackElement {
    pub children: Vec<AnyElement>,
    pub direction: StackDirection,
}

impl Drawable for RawStackElement {
    fn draw(&self, ctx: &BuildContext) {
        let content_size = self.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content_size,
            canvas: ctx.canvas.clone(),
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
            window: ctx.window.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        let mut sorted_children: Vec<_> = self.children.iter().collect();

        sorted_children.sort_by_key(|child| child.layer());

        if self.direction == StackDirection::Reverse {
            for child in sorted_children.iter().rev() {
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

impl EventElement for RawStackElement {
    /// Offer the topmost layer first, matching paint order.
    ///
    /// Position-based dispatch walks the child list in reverse, so visiting
    /// children in ascending layer order makes the highest layer answer a press
    /// before anything painted beneath it. Without this a full-area bottom
    /// layer — a `Scrollable` under a floating `Align`, say — swallows the press
    /// aimed at the button above it.
    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        let mut sorted: Vec<&'a dyn Element> =
            self.children.iter().map(|child| child.as_ref()).collect();
        sorted.sort_by_key(|child| child.layer());
        for child in sorted {
            visitor(child);
        }
    }
}
