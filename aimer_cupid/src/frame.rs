//! The hand-off payload between the thread that builds a frame and the thread
//! that rasterizes it.

use crate::draw_cmd::DrawList;

/// A finished, immutable frame: everything the renderer needs to encode and
/// present, and nothing else.
///
/// Building a frame walks the widget tree, which is full of `Rc` and `RefCell`
/// and therefore bound to the thread that owns it. A `Frame` deliberately
/// carries none of that — only the recorded draw commands and the surface
/// dimensions they were laid out for — which is what makes it `Send` and lets a
/// raster thread consume it while the UI thread moves on to the next frame.
///
/// The dimensions travel with the frame rather than being read back from the GPU
/// context at present time. A resize between build and present would otherwise
/// encode a frame laid out for the old size against the new one.
///
/// # Examples
///
/// ```
/// use aimer_cupid::draw_cmd::DrawList;
/// use aimer_cupid::frame::Frame;
///
/// let frame = Frame::new(DrawList::new(), 800, 600);
///
/// assert_eq!(frame.width, 800);
/// assert_eq!(frame.height, 600);
/// assert!(frame.is_empty());
///
/// // The whole payload can be moved to another thread.
/// std::thread::spawn(move || frame.draw_list.commands().len())
///     .join()
///     .unwrap();
/// ```
pub struct Frame {
    /// The commands recorded for this frame, owned outright.
    pub draw_list: DrawList,
    /// Surface width, in physical pixels, the frame was built for.
    pub width: u32,
    /// Surface height, in physical pixels, the frame was built for.
    pub height: u32,
}

impl Frame {
    /// Wraps a recorded draw list together with the surface size it targets.
    #[inline]
    pub fn new(draw_list: DrawList, width: u32, height: u32) -> Self {
        Self {
            draw_list,
            width,
            height,
        }
    }

    /// Returns `true` when the frame records no commands.
    ///
    /// An empty frame still has to be presented: skipping it would leave the
    /// previous frame's contents on screen.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.draw_list.commands().is_empty()
    }

    /// Consumes the frame and returns the draw list, so the caller can hand the
    /// buffer back to the canvas for reuse.
    #[inline]
    pub fn into_draw_list(self) -> DrawList {
        self.draw_list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::{Color, Rect};

    #[test]
    fn frame_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Frame>();
    }

    #[test]
    fn frame_keeps_the_size_it_was_built_for() {
        let frame = Frame::new(DrawList::new(), 1280, 720);

        assert_eq!((frame.width, frame.height), (1280, 720));
    }

    #[test]
    fn an_empty_frame_is_reported_as_empty() {
        let mut list = DrawList::new();

        assert!(Frame::new(DrawList::new(), 1, 1).is_empty());

        list.fill_rect(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::red(),
            [0.0; 4],
            [0.0; 4],
            Color::transparent(),
        );

        assert!(!Frame::new(list, 1, 1).is_empty());
    }
}
