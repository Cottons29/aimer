use super::*;

impl<T: Element> RawContainer<T> {
    pub(super) fn render(&self, ctx: &BuildContext) {
        ctx.canvas.save();

        let constraint = ctx.box_constraint;

        let parent_width = constraint.max_width;
        let parent_height = constraint.max_height;
        let scale = ctx.scale;

        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => parent_width * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => parent_width - m_left - m_right,
        };

        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => parent_height * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => parent_height - m_top - m_bottom,
        };

        let box_width = box_width.max(0.0);
        let box_height = box_height.max(0.0);

        // Use computed_size to get correct dimensions (handles unbounded/scrollable
        // case)
        let computed = self.computed_size(ctx);
        let (m_left_v, m_top_v, m_right_v, m_bottom_v) = self.margin(ctx);
        let draw_width = (computed.width - m_left_v - m_right_v).max(0.0);
        let draw_height = (computed.height - m_top_v - m_bottom_v).max(0.0);

        // Record the on-screen (logical) bounds every frame. `dispatch_event`
        // uses `pos_start_end` to decide whether an event lands on this element;
        // without live bounds an opaque container could never occlude an event
        // at a specific position (see `on_event`). Bounds start after the margin
        // translate and span the actually-drawn size (`draw_width`/`draw_height`).
        let (start_x, start_y) = ctx.canvas.get_transform_translation();
        self.bounds.save(
            scale,
            start_x + m_left,
            start_y + m_top,
            draw_width,
            draw_height,
        );

        // Translate to the decorated box before painting. Margin is layout
        // space outside the complete decoration; it must not be filled by the
        // background, border, or outline.
        ctx.canvas.translate(Vec2d {
            x: m_left,
            y: m_top,
        });

        let p_left = self.padding.left.value(box_width, scale);
        let p_top = self.padding.top.value(box_height, scale);
        let _p_right = self.padding.right.value(box_width, scale);
        let _p_bottom = self.padding.bottom.value(box_height, scale);

        let border = self.decoration().border;
        let radii = self
            .decoration()
            .border_radius
            .resolve(box_width, box_height, scale);

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };
        let b_left = get_stroke(border.left.stroke, box_width).max(0.0);
        let b_right = get_stroke(border.right.stroke, box_width).max(0.0);
        let b_top = get_stroke(border.top.stroke, box_height).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, box_height).max(0.0);

        // Draw decoration (background, border, outline)

        // Clip to inset rect (inside borders)
        let clip_x = b_left;
        let clip_y = b_top;
        let clip_w = (box_width - b_right - clip_x).max(0.0);
        let clip_h = (box_height - b_bottom - clip_y).max(0.0);

        let inner_radii = [
            (radii[0] - b_top.max(b_left)).max(0.0),     // top-left
            (radii[1] - b_top.max(b_right)).max(0.0),    // top-right
            (radii[2] - b_bottom.max(b_right)).max(0.0), // bottom-right
            (radii[3] - b_bottom.max(b_left)).max(0.0),  /* bottom-left */
        ];

        ctx.canvas.translate(Vec2d {
            x: p_left + b_left,
            y: p_top + b_top,
        });

        let mut child_ctx = ctx.clone();
        let content_w = (box_width - p_left - b_left - _p_right - b_right).max(0.0);
        let content_h = (box_height - p_top - b_top - _p_bottom - b_bottom).max(0.0);
        child_ctx.box_constraint.max_width = content_w;
        child_ctx.box_constraint.max_height = content_h;
        child_ctx.parent_size = ResolvedSize {
            width: content_w,
            height: content_h,
        };

        // The child is drawn after translating the canvas by the margin and the
        // padding + border inset, so the visibility rect (used for scroll
        // culling) must be shifted by the same offset. Otherwise children of a
        // padded/margined container are culled too early and disappear before
        // they actually leave the viewport.
        let inset_x = m_left + p_left + b_left;
        let inset_y = m_top + p_top + b_top;
        child_ctx.visible_rect = ctx
            .visible_rect
            .map(|(vx, vy, vw, vh)| (vx - inset_x, vy - inset_y, vw, vh));

        // The child may intentionally paint outside its measured content into
        // the padding area, so use the complete clipping region rather than
        // the child's nominal content bounds. That keeps unknown child bounds
        // conservative while still skipping the whole subtree when the clip
        // itself is outside the ancestor viewport.
        if ctx.is_rect_visible(
            m_left + clip_x,
            m_top + clip_y,
            clip_w,
            clip_h,
        ) {
            self.child.update(&child_ctx);
        }
        ctx.canvas.restore();
    }
}

