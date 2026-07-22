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
        AnyWidget::new_projected(self, project_widget, project_widget_mut)
    }

    /// Returns the text content if this is a text widget.
    /// Used by the reconciliation system to update text elements in-place.
    fn text_content(&self) -> Option<&str> {
        None
    }
}

fn project_widget<W: Widget + 'static>(value: &W) -> &(dyn Widget + 'static) {
    value
}

fn project_widget_mut<W: Widget + 'static>(value: &mut W) -> &mut (dyn Widget + 'static) {
    value
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

    use aimer_rubick::INLINE_CAPACITY;

    use super::*;

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
        let heap = StorageWidget([0; INLINE_CAPACITY + 1]).boxed();

        assert!(inline.is_inline());
        assert!(heap.is_heap());

        let owners = std::hint::black_box([inline, heap]);
        assert_eq!(owners[0].debug_name(), "StorageWidget");
        assert_eq!(owners[1].debug_name(), "StorageWidget");
    }

    struct DroppingWidget<const N: usize> {
        drops: Rc<Cell<usize>>,
        _bytes: [u8; N],
    }

    impl<const N: usize> Drop for DroppingWidget<N> {
        fn drop(&mut self) {
            self.drops
                .set(self.drops.get() + 1);
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
                _bytes: [0; INLINE_CAPACITY],
            }
            .boxed();

            assert!(inline.is_inline());
            assert!(heap.is_heap());
        }

        assert_eq!(drops.get(), 2);
    }
}
