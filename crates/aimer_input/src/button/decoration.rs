use super::*;

pub(super) struct ButtonDecoration {
    hover: Rc<Cell<bool>>,
    pressed: Rc<Cell<bool>>,
    // Resolve fallback lightening/darkening once per configuration build.
    // Selecting a pointer state does not clone decorations or allocate.
    decorations: [BoxDecoration; 4],
}

impl ButtonDecoration {
    pub(super) fn new<W: Widget + 'static>(state: &ButtonState<W>) -> Self {
        Self {
            hover: state.is_hover.clone(),
            pressed: state.is_pressed.clone(),
            decorations: [
                state.decoration_for(false, false),
                state.decoration_for(true, false),
                state.decoration_for(false, true),
                state.decoration_for(true, true),
            ],
        }
    }

    #[inline]
    fn selected(&self) -> usize {
        usize::from(self.hover.get()) | (usize::from(self.pressed.get()) << 1)
    }

    pub(super) fn set_hover(&self, hover: bool) {
        self.set_flag(&self.hover, hover);
    }

    pub(super) fn set_pressed(&self, pressed: bool) {
        self.set_flag(&self.pressed, pressed);
    }

    fn set_flag(&self, flag: &Cell<bool>, value: bool) {
        if flag.get() == value {
            return;
        }
        let before = &self.decorations[self.selected()];
        flag.set(value);
        let after = &self.decorations[self.selected()];
        if before == after {
            return;
        }
        let before_widths = [before.border.top.stroke, before.border.right.stroke,
            before.border.bottom.stroke, before.border.left.stroke];
        let after_widths = [after.border.top.stroke, after.border.right.stroke,
            after.border.bottom.stroke, after.border.left.stroke];
        if before_widths != after_widths {
            // Border insets can alter ancestor measurements and event bounds.
            // The mounted elements stay in place; refresh their cached geometry.
            aimer_widget::notify_element_tree_changed();
        } else if before.border_radius != after.border_radius
            || before.outline != after.outline
            || before.box_shadow != after.box_shadow
        {
            aimer_widget::notify_retained_render_structure_changed();
        }
        aimer_events::window::request_animation_frame();
    }
}

impl aimer_container::RetainedBoxDecoration for ButtonDecoration {
    #[inline]
    fn decoration(&self) -> &BoxDecoration {
        &self.decorations[self.selected()]
    }

    #[inline]
    fn revision(&self) -> u64 {
        self.selected() as u64
    }
}
