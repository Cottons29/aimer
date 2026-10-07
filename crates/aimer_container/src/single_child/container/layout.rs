use super::*;

impl<T: Element> LayoutElement for RawContainer<T> {
    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        // These bounds follow paint transforms such as scroll offsets, which
        // can change without a layout-generation update. Keep this target
        // unbounded in the cached event index; dispatch checks the live bounds.
        None
    }

    #[inline]
    fn is_layout_stable(&self) -> bool {
        self.can_delegate_paint_islands() && self.child.is_layout_stable()
    }

    fn size(&self) -> Option<Size> {
        Some(Size {
            width: self.width,
            height: self.height,
        })
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.refresh_decoration_layout();
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;
        let p_w = ctx.box_constraint.max_width;
        let p_h = ctx.box_constraint.max_height;
        let threshold = 1_000_000.0f32;

        let m_left = self.margin.left.value(p_w, scale);
        let m_right = self.margin.right.value(p_w, scale);
        let m_top = self.margin.top.value(p_h, scale);
        let m_bottom = self.margin.bottom.value(p_h, scale);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => p_w * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => p_w - (m_left + m_right),
        };

        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => p_h * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => p_h - (m_top + m_bottom),
        };

        // When Auto dimension is unbounded (e.g. inside scrollable), derive size from
        // child
        let width_unbounded = matches!(self.width, Dimension::Auto) && box_width > threshold;
        let height_unbounded = matches!(self.height, Dimension::Auto) && box_height > threshold;

        let result = if width_unbounded || height_unbounded {
            let capped_w = box_width.min(threshold);
            let capped_h = box_height.min(threshold);

            let p_left = self.padding.left.value(capped_w, scale);
            let p_right = self.padding.right.value(capped_w, scale);
            let p_top = self.padding.top.value(capped_h, scale);
            let p_bottom = self.padding.bottom.value(capped_h, scale);

            let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
                match dim {
                    Dimension::Px(w) => w * scale,
                    Dimension::Percent(p) => parent_val * (p / 100.0),
                    Dimension::Auto => 0.0,
                }
            };
            let bl = get_stroke(self.decoration().border.left.stroke, capped_w).max(0.0);
            let br = get_stroke(self.decoration().border.right.stroke, capped_w).max(0.0);
            let bt = get_stroke(self.decoration().border.top.stroke, capped_h).max(0.0);
            let bb = get_stroke(self.decoration().border.bottom.stroke, capped_h).max(0.0);

            let mut child_ctx = ctx.clone();
            child_ctx.box_constraint.max_width = if width_unbounded {
                f32::MAX
            } else {
                (box_width - p_left - bl - p_right - br).max(0.0)
            };
            child_ctx.box_constraint.max_height = if height_unbounded {
                f32::MAX
            } else {
                (box_height - p_top - bt - p_bottom - bb).max(0.0)
            };
            let child_size = self.child.computed_size(&child_ctx);

            let final_w = if width_unbounded {
                child_size.width + p_left + p_right + bl + br + m_left + m_right
            } else {
                box_width + m_left + m_right
            };
            let final_h = if height_unbounded {
                child_size.height + p_top + p_bottom + bt + bb + m_top + m_bottom
            } else {
                box_height + m_top + m_bottom
            };

            ResolvedSize {
                width: final_w.max(0.0),
                height: final_h.max(0.0),
            }
        } else {
            ResolvedSize {
                width: (box_width + m_left + m_right).max(0.0),
                height: (box_height + m_top + m_bottom).max(0.0),
            }
        };
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.refresh_decoration_layout();
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_content(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;
        let p_w = ctx.box_constraint.max_width;
        let p_h = ctx.box_constraint.max_height;
        let threshold = 1_000_000.0f32;

        let m_left = self.margin.left.value(p_w, scale);
        let m_right = self.margin.right.value(p_w, scale);
        let m_top = self.margin.top.value(p_h, scale);
        let m_bottom = self.margin.bottom.value(p_h, scale);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => p_w * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => p_w - (m_left + m_right),
        };
        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => p_h * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => p_h - (m_top + m_bottom),
        };

        let width_unbounded = matches!(self.width, Dimension::Auto) && box_width > threshold;
        let height_unbounded = matches!(self.height, Dimension::Auto) && box_height > threshold;

        let b_w = box_width.max(0.0);
        let b_h = box_height.max(0.0);
        let capped_w = b_w.min(threshold);
        let capped_h = b_h.min(threshold);

        let p_left = self.padding.left.value(capped_w, scale);
        let p_right = self.padding.right.value(capped_w, scale);
        let p_top = self.padding.top.value(capped_h, scale);
        let p_bottom = self.padding.bottom.value(capped_h, scale);

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let border = self.decoration().border;

        let b_left = get_stroke(border.left.stroke, capped_w).max(0.0);
        let b_right = get_stroke(border.right.stroke, capped_w).max(0.0);
        let b_top = get_stroke(border.top.stroke, capped_h).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, capped_h).max(0.0);

        let result = if width_unbounded || height_unbounded {
            let mut child_ctx = ctx.clone();
            child_ctx.box_constraint.max_width = if width_unbounded {
                f32::MAX
            } else {
                (b_w - p_left - b_left - p_right - b_right).max(0.0)
            };
            child_ctx.box_constraint.max_height = if height_unbounded {
                f32::MAX
            } else {
                (b_h - p_top - b_top - p_bottom - b_bottom).max(0.0)
            };
            let child_size = self.child.computed_size(&child_ctx);

            ResolvedSize {
                width: if width_unbounded {
                    child_size.width
                } else {
                    (b_w - p_left - p_right - b_left - b_right).max(0.0)
                },
                height: if height_unbounded {
                    child_size.height
                } else {
                    (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0)
                },
            }
        } else {
            ResolvedSize {
                width: (b_w - p_left - p_right - b_left - b_right).max(0.0),
                height: (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0),
            }
        };
        self.cache
            .set_content(ctx.box_constraint, scale_bits, result);
        result
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();

        let m_w: f32 = 0.0;
        let m_h: f32 = 0.0;
        let mut p_w: f32 = 0.0;
        let mut p_h: f32 = 0.0;
        let mut b_w: f32 = 0.0;
        let mut b_h: f32 = 0.0;

        // Note: For get_size_from_child, we don't have a parent size to resolve
        // percentages, so we can only accurately add Px values. Percentages
        // will be ignored or should be handled by the layout system during
        // actual resolution.

        if let Spacing::Px(v) = self.padding.left {
            p_w += v as f32;
        }
        if let Spacing::Px(v) = self.padding.right {
            p_w += v as f32;
        }
        if let Spacing::Px(v) = self.padding.top {
            p_h += v as f32;
        }
        if let Spacing::Px(v) = self.padding.bottom {
            p_h += v as f32;
        }

        if let Dimension::Px(v) = self.decoration().border.left.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.decoration().border.right.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.decoration().border.top.stroke {
            b_h += v;
        }
        if let Dimension::Px(v) = self.decoration().border.bottom.stroke {
            b_h += v;
        }

        if let Dimension::Px(w) = self.width {
            size.width = Dimension::Px(w + m_w);
        } else {
            size.width = match size.width {
                Dimension::Px(v) => Dimension::Px(v + m_w + p_w + b_w),
                other => other,
            };
        }

        if let Dimension::Px(h) = self.height {
            size.height = Dimension::Px(h + m_h);
        } else {
            size.height = match size.height {
                Dimension::Px(v) => Dimension::Px(v + m_h + p_h + b_h),
                other => other,
            };
        }

        Some(size)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.pos_start_end()
    }
}

