//! Recording and presentation operations on the current retained node.

use super::*;

impl CurrentBuildContext {
    /// Returns the element whose local commands this context records.
    #[inline]
    pub const fn element_id(&self) -> RenderNodeId {
        self.element
    }

    /// Updates one direct child's retained presentation without re-recording
    /// either command list.
    #[doc(hidden)]
    pub fn set_child_presentation_at(
        &self,
        child_index: usize,
        transform: Mat3,
        opacity: f32,
    ) -> bool {
        let child = self
            .draw_cmd
            .borrow()
            .node(self.element)
            .and_then(|node| node.children.get(child_index).copied());
        let Some(child) = child else {
            return false;
        };
        RenderTree {
            draw_cmd: self.draw_cmd.clone(),
        }
        .set_presentation(child, transform, opacity)
        .is_ok()
    }

    /// Reserves this element's retained list for one transactional recording.
    #[doc(hidden)]
    pub fn begin_recording(&self) -> Result<DrawListWriter, RenderTreeError> {
        let draw_list = self
            .draw_cmd
            .borrow()
            .node(self.element)
            .ok_or(RenderTreeError::UnknownNode(self.element))?
            .draw_list
            .clone();
        {
            let mut retained = draw_list.borrow_mut();
            if retained.recording {
                return Err(RenderTreeError::RecorderAlreadyOpen(self.element));
            }
            retained.recording = true;
        }
        Ok(DrawListWriter {
            owner: self.element,
            draw_cmd: self.draw_cmd.clone(),
            draw_list,
            finished: false,
        })
    }
}

