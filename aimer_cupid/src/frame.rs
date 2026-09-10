//! The hand-off payload between the thread that builds a frame and the thread
//! that rasterizes it.

use crate::draw_cmd::DrawList;
use crate::damage_region::DamageSet;

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

/// Renderer metadata that travels with a finished frame.
#[doc(hidden)]
pub struct FrameRenderMetadata {
    device_scale_bits: u32,
    surface_identity: u64,
    renderer_generation: u64,
    context_generation: u64,
    resource_generation: u64,
    damage: DamageSet,
}

/// An owned frame plus the target and damage information needed by a raster
/// thread to choose between a full and a persistent-target repaint.
#[doc(hidden)]
pub struct FramePacket {
    frame: Frame,
    metadata: FrameRenderMetadata,
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

impl FrameRenderMetadata {
    /// Creates metadata for a conservative full repaint.
    #[inline]
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            device_scale_bits: 1.0_f32.to_bits(),
            surface_identity: 0,
            renderer_generation: 0,
            context_generation: 0,
            resource_generation: 0,
            damage: DamageSet::full(width, height),
        }
    }

    /// Creates metadata for a frame whose damage was collected by the widget
    /// renderer.
    #[inline]
    pub fn new(
        device_scale: f32,
        surface_identity: u64,
        renderer_generation: u64,
        context_generation: u64,
        resource_generation: u64,
        damage: DamageSet,
    ) -> Self {
        Self {
            device_scale_bits: device_scale.to_bits(),
            surface_identity,
            renderer_generation,
            context_generation,
            resource_generation,
            damage,
        }
    }

    /// Returns the device scale captured when the frame was built.
    #[inline]
    pub fn device_scale(&self) -> f32 {
        f32::from_bits(self.device_scale_bits)
    }

    /// Returns the identity of the presentation surface.
    #[inline]
    pub fn surface_identity(&self) -> u64 {
        self.surface_identity
    }

    /// Returns the renderer generation that produced the frame.
    #[inline]
    pub fn renderer_generation(&self) -> u64 {
        self.renderer_generation
    }

    /// Returns the GPU context generation that produced the frame.
    #[inline]
    pub fn context_generation(&self) -> u64 {
        self.context_generation
    }

    /// Returns the resource generation associated with the frame target.
    #[inline]
    pub fn resource_generation(&self) -> u64 {
        self.resource_generation
    }

    /// Returns the normalized target damage.
    #[inline]
    pub fn damage(&self) -> &DamageSet {
        &self.damage
    }
}

impl FramePacket {
    /// Wraps a frame with explicit renderer metadata.
    #[inline]
    pub fn new(frame: Frame, metadata: FrameRenderMetadata) -> Self {
        Self { frame, metadata }
    }

    /// Wraps a frame in the compatibility full-repaint policy.
    #[inline]
    pub fn full(frame: Frame) -> Self {
        let metadata = FrameRenderMetadata::full(frame.width, frame.height);
        Self { frame, metadata }
    }

    /// Borrows the immutable frame payload.
    #[inline]
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Borrows the renderer metadata.
    #[inline]
    pub fn metadata(&self) -> &FrameRenderMetadata {
        &self.metadata
    }

    /// Consumes the packet and returns its frame payload.
    #[inline]
    pub fn into_frame(self) -> Frame {
        self.frame
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
        assert_send::<FramePacket>();
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

    #[test]
    fn a_compatibility_packet_requests_a_full_repaint() {
        let packet = FramePacket::full(Frame::new(DrawList::new(), 12, 8));

        assert!(packet.metadata().damage().is_full());
        assert_eq!(packet.metadata().damage().target_size(), (12, 8));
    }
}
