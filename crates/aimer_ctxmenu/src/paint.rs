//! Drawing the panel, in whichever shape it was opened.
//!
//! Both shapes are the same three passes — panel, highlight, label — over the
//! rectangles [`ContextMenuLayout`] produced; only the metrics differ, and
//! those the style already answers. The one thing worth spelling out is the
//! coordinate change: every rectangle a menu deals in is *absolute logical*,
//! because that is the space a pointer event arrives in, while the canvas draws
//! in element-local physical pixels. The two spaces meet in [`placer`] and
//! nowhere else.

use aimer_attribute::{Bounds, ResolvedSize, Vec2d};
use aimer_style::TextStyle;
use aimer_widget::base::{BuildContext, Color};

use crate::layout::{ContextMenuLayout, ContextMenuStyle};
use crate::menu::ContextMenu;

/// The dark, nearly opaque panel behind the labels.
pub const PANEL_COLOR: Color = Color::Rgba(58, 58, 60, 242);

/// The labels.
pub const LABEL_COLOR: Color = Color::Rgba(255, 255, 255, 255);

/// The labels of rows that cannot be chosen.
pub const DISABLED_LABEL_COLOR: Color = Color::Rgba(255, 255, 255, 102);

/// The hairline between two rows of a pill.
pub const SEPARATOR_COLOR: Color = Color::Rgba(255, 255, 255, 38);

/// The wash under the row a pointer is pressing or hovering.
pub const HIGHLIGHT_COLOR: Color = Color::Rgba(255, 255, 255, 36);

/// Paints one frame of `menu`, reporting whether it stays up.
///
/// It retires the moment there is nothing to hang off — a tracked anchor that
/// has gone, or a menu opened with no rows at all.
pub(crate) fn paint(menu: &ContextMenu, ctx: &BuildContext) -> bool {
    let items = menu.items.borrow().clone();
    if items.is_empty() {
        return false;
    }

    let scale = if ctx.scale > 0.0 { ctx.scale } else { 1.0 };
    let style = menu.style();
    let text_style = TextStyle::default();
    let font_size = style.font_size() * scale;

    let widths = items
        .iter()
        .map(|item| {
            ctx.canvas.measure_text_styled(
                item.label(),
                font_size,
                text_style.font_family,
                text_style.font_style,
                text_style.font_weight.numeric(),
            ) / scale
        })
        .collect::<Vec<_>>();

    let (viewport_width, viewport_height) = menu.viewport();
    if !menu.place(&widths, viewport_width, viewport_height) {
        return false;
    }
    let Some(layout) = menu.layout() else {
        return false;
    };

    let place = placer(ctx, scale);
    let (panel_pos, panel_size) = place(layout.bounds);
    ctx.canvas
        .fill_color_rect(panel_pos, panel_size, PANEL_COLOR, [style.radius() * scale; 4]);

    let metrics = ctx.canvas.measure_text_metrics_styled(
        items[0].label(),
        font_size,
        0.0,
        text_style.font_family,
        text_style.font_style,
        text_style.font_weight.numeric(),
    );
    let text_height = metrics.ascent - metrics.descent;
    // A pressed row outranks a hovered one: a finger holding a row down is
    // hovering it too, and only one wash may be drawn.
    let lit = menu.pressed.get().or(menu.hovered.get());

    for (index, (item, bounds)) in items.iter().zip(&layout.items).enumerate() {
        let (item_pos, item_size) = place(*bounds);
        if lit == Some(index) {
            ctx.canvas.fill_color_rect(
                item_pos,
                item_size,
                HIGHLIGHT_COLOR,
                [row_radius(&layout, index, scale); 4],
            );
        }
        if style == ContextMenuStyle::Pill && index > 0 {
            let (pos, size) = place(Bounds::new(
                bounds.x,
                bounds.y + 10.0,
                1.0,
                bounds.height - 20.0,
            ));
            ctx.canvas
                .fill_color_rect(pos, size, SEPARATOR_COLOR, [0.0; 4]);
        }
        let baseline = item_pos.y + (item_size.height - text_height) * 0.5 + metrics.ascent;
        ctx.canvas.draw_text_styled(
            item.label(),
            Vec2d {
                x: item_pos.x + style.item_padding() * scale,
                y: baseline,
            },
            font_size,
            if item.is_enabled() {
                LABEL_COLOR
            } else {
                DISABLED_LABEL_COLOR
            },
            text_style.font_family,
            text_style.font_style,
            text_style.font_weight.numeric(),
        );
    }

    true
}

/// The corner radius of a row's highlight.
///
/// A pill's end rows are as round as the pill, or the wash would square off its
/// ends; a list's rows are square, because the panel's padding already keeps
/// them clear of its corners.
#[inline]
fn row_radius(layout: &ContextMenuLayout, index: usize, scale: f32) -> f32 {
    match layout.style {
        ContextMenuStyle::Pill if index == 0 || index + 1 == layout.items.len() => {
            layout.style.radius() * scale
        }
        _ => 0.0,
    }
}

/// Turns absolute logical coordinates into the element-local physical ones the
/// canvas draws in.
fn placer(ctx: &BuildContext, scale: f32) -> impl Fn(Bounds) -> (Vec2d, ResolvedSize) {
    let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
    move |bounds: Bounds| {
        (
            Vec2d {
                x: bounds.x * scale - abs_x,
                y: bounds.y * scale - abs_y,
            },
            ResolvedSize {
                width: bounds.width * scale,
                height: bounds.height * scale,
            },
        )
    }
}
