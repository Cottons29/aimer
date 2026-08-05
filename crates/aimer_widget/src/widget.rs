use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use aimer_attribute::size::ResolvedSize;

use crate::base::BuildContext;
use crate::{AnyElement, AnyWidget};

mod recovery;
pub mod stateful;
pub mod stateless;

pub trait Widget {
    fn key(&self) -> Option<crate::key::Key> {
        None
    }
    fn to_element(&self, ctx: &BuildContext) -> AnyElement;

    fn debug_name(&self) -> &'static str {
        "Unknown"
    }

    /// Erases this widget into an inline-or-heap [`AnyWidget`].
    ///
    /// Values fitting the configured `Rubick` size and alignment are embedded
    /// directly in the returned owner. Other values use one heap allocation.
    /// Despite the historical method name, allocation is therefore not
    /// guaranteed. Moving an inline owner also moves this widget.
    fn boxed(self) -> AnyWidget
    where
        Self: Sized + 'static,
    {
        AnyWidget::erase(self)
    }

    /// Returns the text content if this is a text widget.
    /// Used by the reconciliation system to update text elements in-place.
    fn text_content(&self) -> Option<&str> {
        None
    }
}

// SAFETY: The template is `null::<W>()` coerced to the target, so it carries
// exactly `W`'s vtable and a null data address.
unsafe impl<W: Widget + 'static> aimer_rubick::ErasedFrom<W> for dyn Widget {
    const TEMPLATE: *const Self = std::ptr::null::<W>() as *const dyn Widget;
}

impl Widget for AnyWidget {
    fn key(&self) -> Option<crate::key::Key> {
        self.as_ref().key()
    }

    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        self.as_ref().to_element(ctx)
    }

    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }

    fn text_content(&self) -> Option<&str> {
        self.as_ref().text_content()
    }

    /// Returns this owner unchanged.
    ///
    /// An [`AnyWidget`] is already erased, so wrapping it again would store an
    /// owner inside an owner: one extra indirection on every borrow and, since
    /// the inner owner is larger than the inline capacity, one guaranteed
    /// allocation per rebuild.
    #[inline]
    fn boxed(self) -> AnyWidget
    where
        Self: Sized + 'static,
    {
        self
    }
}

impl Widget for Box<dyn Widget> {
    fn key(&self) -> Option<crate::key::Key> {
        self.as_ref().key()
    }
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        self.as_ref().to_element(ctx)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    // fn text_content(&self) -> Option<&str> {
    //     self.as_ref().text_content()
    // }
}

impl Widget for Rc<dyn Widget> {
    fn key(&self) -> Option<crate::key::Key> {
        self.as_ref().key()
    }
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        self.as_ref().to_element(ctx)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    // fn text_content(&self) -> Option<&str> {
    //     self.as_ref().text_content()
    // }
}

/// Draw a colored bounding box + label at the current canvas transform origin.
/// Called during the draw pass when the widget inspector is enabled.
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub(crate) fn draw_inspector_box(ctx: &BuildContext, size: ResolvedSize, name: &'static str) {
    use aimer_color::prelude::Color;

    let w = size.width;
    let h = size.height;
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    // Bounding box stroke
    let stroke_color = Color::Rgba(0, 120, 255, 200);
    ctx.canvas.stroke_rect(
        (0.0_f32, 0.0_f32).into(),
        ResolvedSize {
            width: w,
            height: h,
        },
        stroke_color,
        1.5,
        [0.0; 4],
    );

    // Label
    let font_size = 10.0_f32;
    let label = format!("{} {:.0}×{:.0}", name, w, h);
    let label_w = (label.len() as f32) * font_size * 0.55 + 4.0;
    let label_h = font_size + 4.0;

    let bg_color = Color::Rgba(0, 0, 0, 180);
    ctx.canvas.fill_color_rect(
        (0.0_f32, 0.0_f32).into(),
        ResolvedSize {
            width: label_w,
            height: label_h,
        },
        bg_color,
        [0.0; 4],
    );

    let text_color = Color::Rgba(255, 255, 255, 255);
    ctx.canvas.draw_text(
        &label,
        (2.0_f32, font_size).into(),
        font_size,
        text_color,
        400,
    );
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// Payload bytes an [`AnyWidget`] holds without allocating.
    const WIDGET_CAPACITY: usize = AnyWidget::INLINE_CAPACITY;

    struct StorageWidget<const N: usize>([u8; N]);

    impl<const N: usize> Widget for StorageWidget<N> {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            panic!("storage contract test does not build an element")
        }

        fn debug_name(&self) -> &'static str {
            "StorageWidget"
        }
    }

    #[test]
    fn erased_widgets_select_inline_or_heap_storage_and_dispatch_after_moves() {
        let inline = StorageWidget([]).boxed();
        let heap = StorageWidget([0; WIDGET_CAPACITY + 1]).boxed();

        assert!(inline.is_inline());
        assert!(heap.is_heap());

        let owners = std::hint::black_box([inline, heap]);
        assert_eq!(owners[0].debug_name(), "StorageWidget");
        assert_eq!(owners[1].debug_name(), "StorageWidget");
    }

    #[test]
    fn an_erased_widget_costs_its_capacity_plus_one_word() {
        assert_eq!(WIDGET_CAPACITY / size_of::<usize>(), 8);
        assert_eq!(size_of::<AnyWidget>(), 9 * size_of::<usize>());
        assert_eq!(size_of::<AnyElement>(), 2 * size_of::<usize>());
    }

    #[test]
    fn container_sized_widgets_are_erased_without_allocating() {
        let container = StorageWidget([0; WIDGET_CAPACITY]).boxed();

        assert!(
            container.is_inline(),
            "a container sized widget must fit the erased owner"
        );
        assert!(
            container.is_direct(),
            "erasing must not store projection adapters"
        );
    }

    #[test]
    fn erasing_an_already_erased_widget_returns_the_same_owner() {
        let inline = StorageWidget([0; WIDGET_CAPACITY]).boxed();
        let inline = inline.boxed();

        assert!(
            inline.is_inline(),
            "re-erasing must not wrap the owner in another owner"
        );
        assert_eq!(inline.debug_name(), "StorageWidget");

        // A heap payload keeps its allocation across owner moves, so its
        // address proves that no second erasure took place.
        let heap = StorageWidget([0; WIDGET_CAPACITY + 1]).boxed();
        let address = heap.as_ref() as *const dyn Widget as *const u8;
        let heap = heap.boxed();

        assert!(heap.is_heap());
        assert_eq!(
            heap.as_ref() as *const dyn Widget as *const u8,
            address,
            "the payload must not be re-erased into a new allocation"
        );
    }

    struct DroppingWidget<const N: usize> {
        drops: Rc<Cell<usize>>,
        _bytes: [u8; N],
    }

    impl<const N: usize> Drop for DroppingWidget<N> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    impl<const N: usize> Widget for DroppingWidget<N> {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            panic!("drop contract test does not build an element")
        }
    }

    #[test]
    fn erased_widgets_drop_inline_and_heap_values_exactly_once() {
        let drops = Rc::new(Cell::new(0));
        {
            let inline = DroppingWidget {
                drops: Rc::clone(&drops),
                _bytes: [],
            }
            .boxed();
            let heap = DroppingWidget {
                drops: Rc::clone(&drops),
                _bytes: [0; WIDGET_CAPACITY],
            }
            .boxed();

            assert!(inline.is_inline());
            assert!(heap.is_heap());
        }

        assert_eq!(drops.get(), 2);
    }
}
