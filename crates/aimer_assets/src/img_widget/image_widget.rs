use std::cell::{Cell, UnsafeCell};
use std::path::PathBuf;

use aimer_attribute::Dimension;
use aimer_container::ZeroSizedBox;
use aimer_macro::{EventElement, Rebuildable};
use aimer_style::BoxFit;
use aimer_widget::base::{BuildContext, Color, Colors, ResolvedSize, Size, Vec2d};
use aimer_widget::{
    AnyElement, Drawable, Element, LayoutCache, LayoutElement, VisitorElement, Widget,
};

use crate::ImageResult::Success;
use crate::img_widget::source::ImageSource;
use crate::{ImageProvider, ImageResult};

#[derive(Clone, Copy, Debug, PartialEq)]
struct ImagePaintGeometry {
    pos: Vec2d,
    size: ResolvedSize,
    use_cover: bool,
}

fn image_paint_geometry(
    target: ResolvedSize,
    intrinsic: Option<(u32, u32)>,
    fit: BoxFit,
    scale_factor: f32,
) -> Option<ImagePaintGeometry> {
    let (iw, ih) = intrinsic?;
    if iw == 0 || ih == 0 {
        return None;
    }

    let target_w = target.width.max(0.0);
    let target_h = target.height.max(0.0);
    let iw = iw as f32;
    let ih = ih as f32;
    let scale_x = target_w / iw;
    let scale_y = target_h / ih;
    let (final_w, final_h, use_cover) = match fit {
        BoxFit::Contain | BoxFit::ScaleDown | BoxFit::None => {
            let mut scale = scale_x.min(scale_y);
            if let BoxFit::ScaleDown = fit {
                scale = scale.min(1.0);
            }
            (iw * scale * scale_factor, ih * scale * scale_factor, false)
        }
        BoxFit::FitWidth => {
            let final_w = target_w * scale_factor;
            (final_w, final_w * (ih / iw), false)
        }
        BoxFit::FitHeight => {
            let final_h = target_h * scale_factor;
            (final_h * (iw / ih), final_h, false)
        }
        BoxFit::Cover => {
            let scale = scale_x.max(scale_y);
            (iw * scale * scale_factor, ih * scale * scale_factor, true)
        }
        BoxFit::Fill => {
            let scale = scale_x.min(scale_y);
            (iw * scale * scale_factor, ih * scale * scale_factor, false)
        }
    };

    Some(ImagePaintGeometry {
        pos: Vec2d {
            x: (target_w - final_w) * 0.5,
            y: (target_h - final_h) * 0.5,
        },
        size: ResolvedSize {
            width: final_w,
            height: final_h,
        },
        use_cover,
    })
}

/// Displays an image read from a file-system path.
///
/// The file is loaded and decoded asynchronously and cached by path. While it
/// is loading, this widget draws no content. If reading or decoding fails, it
/// draws a magenta-and-black error pattern; unlike
/// [`AssetImage`](crate::AssetImage) and [`NetworkImage`](crate::NetworkImage),
/// `Image` does not provide custom fallback builders.
///
/// # Example
///
/// ```
/// use aimer_assets::Image;
/// use aimer_style::BoxFit;
///
/// let image = Image::new("images/photo.png").width(320.0)
///                                           .height(180.0)
///                                           .fit(BoxFit::Cover);
/// ```
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_assets::Image", schema_only)]
pub struct Image {
    #[portable_skip]
    pub path: PathBuf,
    pub width: Dimension,
    pub height: Dimension,
    #[portable_skip]
    pub fit: BoxFit,
    pub scale: f32,
}

impl Image {
    /// Creates an image that reads from `path`.
    ///
    /// Width and height default to [`Dimension::Auto`], [`BoxFit::None`] is
    /// used, and the drawing scale is `1.0`. The path is interpreted by the
    /// host file system and is not an `aimer.toml` asset key.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            width: Dimension::default(),
            height: Dimension::default(),
            fit: BoxFit::default(),
            scale: 1.0,
        }
    }

    /// Sets the width of the widget's layout box.
    ///
    /// The default is [`Dimension::Auto`].
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the widget's layout box.
    ///
    /// The default is [`Dimension::Auto`].
    pub fn height(mut self, height: impl Into<Dimension>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets how the image is fitted into its layout box.
    ///
    /// The default is [`BoxFit::None`]. Every mode except [`BoxFit::Fill`]
    /// preserves the image's aspect ratio; `Fill` stretches it to the box.
    pub fn fit(mut self, fit: BoxFit) -> Self {
        self.fit = fit;
        self
    }

    /// Multiplies the final painted image size around the center of its layout
    /// box.
    ///
    /// This does not change the widget's layout size. The default is `1.0`;
    /// values are stored without validation, so callers should provide a
    /// finite, non-negative value.
    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.scale = scale.into();
        self
    }
}

impl Widget for Image {
    fn to_element(self, _: &BuildContext) -> AnyElement {
        RawImageWidget {
            source: ImageSource::File(self.path.clone()),
            size: Size {
                width: self.width,
                height: self.height,
            },
            cache: LayoutCache::new(),
            fit: self.fit,
            keep_aspect_ratio: self.fit != BoxFit::Fill,
            loading_element: None,
            error_element: None,
            original_size: Cell::new(None),
            cached_id: UnsafeCell::new(None),
            cached_texture_epoch: Cell::new(0),
            scale: self.scale,
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Image"
    }
}

#[derive(Rebuildable, EventElement)]
pub struct RawImageWidget<P: ImageProvider> {
    pub source: P,
    pub size: Size,
    pub cache: LayoutCache,
    pub fit: BoxFit,
    pub keep_aspect_ratio: bool,
    pub original_size: Cell<Option<Size>>,
    pub loading_element: Option<AnyElement>,
    pub error_element: Option<AnyElement>,
    pub cached_id: UnsafeCell<Option<ImageResult>>,
    /// Last renderer image-cache generation observed by this retained widget.
    /// A changed generation makes the provider revalidate its cached image ID
    /// before the widget records another draw command.
    pub cached_texture_epoch: Cell<u64>,
    pub scale: f32,
}

impl<P: ImageProvider> VisitorElement for RawImageWidget<P> {
    fn debug_name(&self) -> &'static str {
        "RawImageElement"
    }
}

impl<P: ImageProvider> LayoutElement for RawImageWidget<P> {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let result = self.size.resolve(&ctx.parent_size, ctx.scale);

        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);

        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.size.resolve(&ctx.parent_size, ctx.scale)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
    }
}

impl<P: ImageProvider> RawImageWidget<P> {
    fn cache_result(&self, result: ImageResult, ctx: &BuildContext) -> ImageResult {
        let was_loading = unsafe {
            matches!(&*self.cached_id.get(), Some(ImageResult::Loading))
        };
        if was_loading && result != ImageResult::Loading {
            // An async provider can finish after an AnimatedSwitcher has
            // stopped animating. Requesting a frame alone is insufficient in
            // that case because retained paint may have no damage to replay.
            aimer_widget::mark_paint_damage_full();
        }
        unsafe { *self.cached_id.get() = Some(result.clone()) };
        self.cached_texture_epoch
            .set(ctx.canvas.texture_cache_epoch());
        result
    }
}

impl<P: ImageProvider> Drawable for RawImageWidget<P> {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let image_result = if let Some(result) = unsafe { &*self.cached_id.get() } {
            let cache_invalidated = match result {
                Success(_) => {
                    let epoch = ctx.canvas.texture_cache_epoch();
                    let invalidated = epoch != self.cached_texture_epoch.get();
                    self.cached_texture_epoch.set(epoch);
                    invalidated
                }
                _ => false,
            };
            if cache_invalidated {
                unsafe { *self.cached_id.get() = None };
                let r = self.source.get_image(ctx);
                self.cache_result(r, ctx)
            } else if result == &ImageResult::Loading {
                let r = self.source.get_image(ctx);
                self.cache_result(r, ctx)
            } else {
                result.clone()
            }
        } else {
            let result = self.source.get_image(ctx);
            self.cache_result(result, ctx)
        };

        match image_result {
            Success(id) => {
                if self.keep_aspect_ratio {
                    let Some(geometry) = image_paint_geometry(
                        size,
                        ctx.canvas.get_image_size(id),
                        self.fit,
                        self.scale,
                    ) else {
                        // A preserving fit cannot safely paint until the
                        // renderer exposes the source dimensions. Filling the
                        // layout box here makes a wide or tall image visibly
                        // jump to the wrong aspect ratio for one frame.
                        return;
                    };

                    if geometry.use_cover {
                        // Clip to target box to emulate cover cropping.
                        ctx.canvas.set_clip(Vec2d { x: 0.0, y: 0.0 }, size);
                        ctx.canvas.draw_image(id, geometry.pos, geometry.size);
                        ctx.canvas.clear_clip();
                    } else {
                        ctx.canvas.draw_image(id, geometry.pos, geometry.size);
                    }
                } else {
                    // Not preserving aspect ratio: fill allocated box
                    let final_w = size.width * self.scale;
                    let final_h = size.height * self.scale;
                    let draw_pos = Vec2d {
                        x: (size.width - final_w) * 0.5,
                        y: (size.height - final_h) * 0.5,
                    };
                    let draw_size = ResolvedSize {
                        width: final_w,
                        height: final_h,
                    };
                    ctx.canvas.draw_image(id, draw_pos, draw_size)
                }
            }

            ImageResult::Loading => {
                self.loading_element
                    .as_ref()
                    .unwrap_or(&ZeroSizedBox.to_element(ctx))
                    .draw(ctx);
            }

            ImageResult::Error(_) => {
                if let Some(error_element) = &self.error_element {
                    error_element.draw(ctx);
                    return;
                }
                let grid_size = 32.0;
                let rows = (size.height / grid_size).ceil() as i32;
                let cols = (size.width / grid_size).ceil() as i32;

                for row in 0..rows {
                    for col in 0..cols {
                        let color = if (row + col) % 2 == 0 {
                            Color::Basic(Colors::Magenta)
                        } else {
                            Color::Basic(Colors::Black)
                        };

                        let pos = Vec2d {
                            x: col as f32 * grid_size,
                            y: row as f32 * grid_size,
                        };

                        let rect_size = ResolvedSize {
                            width: grid_size.min(size.width - pos.x),
                            height: grid_size.min(size.height - pos.y),
                        };

                        if rect_size.width > 0.0 && rect_size.height > 0.0 {
                            ctx.canvas.fill_color_rect(pos, rect_size, color, [0.0; 4]);
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        let image_is_bounded = match self.fit {
            // Cover explicitly clips to the image's layout box, and Fill
            // paints exactly that box.
            BoxFit::Cover | BoxFit::Fill => true,
            // These modes fit inside the layout box when the caller does not
            // enlarge the final painted image.
            BoxFit::Contain | BoxFit::None | BoxFit::ScaleDown => {
                self.scale.is_finite() && self.scale >= 0.0 && self.scale <= 1.0
            }
            // Width-only and height-only fitting may extend beyond the other
            // axis of the layout box.
            BoxFit::FitWidth | BoxFit::FitHeight => false,
        };

        image_is_bounded
            && self
                .loading_element
                .as_ref()
                .map_or(true, |element| element.is_paint_bounded())
            && self
                .error_element
                .as_ref()
                .map_or(true, |element| element.is_paint_bounded())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Debug)]
    struct LoadingThenError {
        calls: Rc<Cell<usize>>,
    }

    impl ImageProvider for LoadingThenError {
        fn get_image(&self, _ctx: &BuildContext) -> ImageResult {
            let call = self.calls.get();
            self.calls.set(call + 1);
            if call == 0 {
                ImageResult::Loading
            } else {
                ImageResult::Error("ready".to_owned())
            }
        }
    }

    #[derive(Clone, Debug)]
    struct CountingSuccess {
        calls: Rc<Cell<usize>>,
    }

    impl ImageProvider for CountingSuccess {
        fn get_image(&self, ctx: &BuildContext) -> ImageResult {
            let call = self.calls.get();
            self.calls.set(call + 1);
            ctx.canvas.set_texture_size(7, 1, 1);
            ImageResult::Success(7)
        }
    }

    fn context() -> BuildContext<'static> {
        use aimer_attribute::{BoxConstraint, Vec2d};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_widget::base::WindowHandle;

        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> =
            std::sync::OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("image widget test runtime should build")
        });
        let _guard = runtime.enter();
        let inner = Box::leak(Box::new(InnerCanvas::new()));
        let mut context = BuildContext::new(
            Canvas::new(inner),
            ResolvedSize {
                width: 32.0,
                height: 32.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        context.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 32.0,
            max_height: 32.0,
        };
        context
    }

    fn image_with_source<P: ImageProvider>(source: P) -> RawImageWidget<P> {
        RawImageWidget {
            source,
            size: Size::new(Dimension::Px(16.0), Dimension::Px(16.0)),
            cache: LayoutCache::new(),
            fit: BoxFit::Fill,
            keep_aspect_ratio: false,
            original_size: Cell::new(None),
            loading_element: None,
            error_element: None,
            cached_id: UnsafeCell::new(None),
            cached_texture_epoch: Cell::new(0),
            scale: 1.0,
        }
    }

    fn loading_then_error_image(calls: Rc<Cell<usize>>) -> RawImageWidget<LoadingThenError> {
        image_with_source(LoadingThenError { calls })
    }

    #[test]
    fn aspect_preserving_geometry_waits_for_intrinsic_dimensions() {
        let target = ResolvedSize {
            width: 1_000.0,
            height: 450.0,
        };

        assert_eq!(
            image_paint_geometry(target, None, BoxFit::Contain, 1.0),
            None,
            "missing image metadata must not turn the layout box into a stretched image"
        );
    }

    #[test]
    fn aspect_preserving_geometry_contains_a_wide_image() {
        let target = ResolvedSize {
            width: 1_000.0,
            height: 450.0,
        };

        let geometry = image_paint_geometry(
            target,
            Some((2_170, 1_516)),
            BoxFit::Contain,
            1.0,
        )
        .expect("known image metadata produces paint geometry");

        assert!((geometry.pos.x - 177.9353).abs() < 0.01);
        assert!(geometry.pos.y.abs() < 0.01);
        assert!((geometry.size.width - 644.1293).abs() < 0.01);
        assert!((geometry.size.height - 450.0).abs() < 0.01);
        assert!(!geometry.use_cover);
    }

    #[test]
    fn async_image_completion_invalidates_paint_after_loading_frame() {
        let ctx = context();
        let image = loading_then_error_image(Rc::new(Cell::new(0)));

        aimer_widget::begin_paint_frame(32, 32);
        image.draw(&ctx);
        let _ = aimer_widget::take_paint_frame_damage(32, 32);

        aimer_widget::begin_paint_frame(32, 32);
        image.draw(&ctx);
        let damage = aimer_widget::take_paint_frame_damage(32, 32);

        assert!(
            damage.is_full(),
            "a loading image becoming drawable must invalidate its retained paint"
        );
    }

    #[test]
    fn image_provider_is_revalidated_after_another_texture_changes() {
        let ctx = context();
        let calls = Rc::new(Cell::new(0));
        let image = image_with_source(CountingSuccess {
            calls: Rc::clone(&calls),
        });

        aimer_widget::begin_paint_frame(32, 32);
        image.draw(&ctx);
        let _ = aimer_widget::take_paint_frame_damage(32, 32);
        assert_eq!(calls.get(), 1);

        let _ = ctx.canvas.load_image(&[1, 2, 3, 4], 1, 1);

        aimer_widget::begin_paint_frame(32, 32);
        image.draw(&ctx);
        let _ = aimer_widget::take_paint_frame_damage(32, 32);

        assert_eq!(
            calls.get(),
            2,
            "a retained success must be revalidated after the renderer cache changes"
        );
    }
}
