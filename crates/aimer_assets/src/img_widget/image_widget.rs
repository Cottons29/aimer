use std::cell::{Cell, UnsafeCell};
use std::path::PathBuf;

use aimer_attribute::Dimension;
use aimer_container::ZeroSizedBox;
use aimer_macro::{EventElement, Rebuildable};
use aimer_style::BoxFit;
use aimer_widget::base::{BuildContext, Color, Colors, ResolvedSize, Size, Vec2d};
use aimer_widget::{Drawable, Element, LayoutCache, LayoutElement, VisitorElement, Widget};

use crate::ImageResult::Success;
use crate::img_widget::source::ImageSource;
use crate::{ImageProvider, ImageResult};

pub struct Image {
    pub path: PathBuf,
    pub width: Dimension,
    pub height: Dimension,
    pub fit: BoxFit,
    pub scale: f32,
}

impl Image {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            width: Dimension::default(),
            height: Dimension::default(),
            fit: BoxFit::default(),
            scale: 1.0,
        }
    }

    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Dimension>) -> Self {
        self.height = height.into();
        self
    }

    pub fn fit(mut self, fit: BoxFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.scale = scale.into();
        self
    }
}

impl Widget for Image {
    fn to_element(&self, _: &BuildContext) -> Box<dyn Element> {
        Box::new(RawImageWidget {
            source: ImageSource::File(self.path.clone()),
            size: Size { width: self.width, height: self.height },
            cache: LayoutCache::new(),
            fit: self.fit,
            keep_aspect_ratio: self.fit != BoxFit::Fill,
            loading_element: None,
            error_element: None,
            original_size: Cell::new(None),
            cached_id: UnsafeCell::new(None),
            scale: self.scale,
        })
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
    pub loading_element: Option<Box<dyn Element>>,
    pub error_element: Option<Box<dyn Element>>,
    pub cached_id: UnsafeCell<Option<ImageResult>>,
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
        if let Some(cached) = self
            .cache
            .get_computed(ctx.box_constraint, scale_bits)
        {
            return cached;
        }

        let result = self
            .size
            .resolve(&ctx.parent_size, ctx.scale);

        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);

        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.size
            .resolve(&ctx.parent_size, ctx.scale)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
    }
}

impl<P: ImageProvider> Drawable for RawImageWidget<P> {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if aimer_widget::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx
                    .canvas
                    .get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = Vec2d { x: end_x / scale, y: end_y / scale };
                let cp = ctx.cursor_pos;
                let is_hovered =
                    cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y;
                if is_hovered
                    && let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write()
                {
                    *hovered = Some((self.debug_name(), l_start, l_end));
                }
            }
        }
        let size = self.computed_size(ctx);
        let image_result = if let Some(result) = unsafe { &*self.cached_id.get() } {
            if result == &ImageResult::Loading {
                let r = self.source.get_image(ctx);
                if r != ImageResult::Loading {
                    unsafe { *self.cached_id.get() = Some(r.clone()) };
                }
                r
            } else {
                result.clone()
            }
        } else {
            let result = self.source.get_image(ctx);
            if result != ImageResult::Loading {
                unsafe { *self.cached_id.get() = Some(result.clone()) };
            }
            result
        };

        match image_result {
            Success(id) => {
                if self.keep_aspect_ratio {
                    if let Some((iw, ih)) = ctx.canvas.get_image_size(id) {
                        let iw = iw as f32;
                        let ih = ih as f32;
                        if iw > 0.0 && ih > 0.0 {
                            let target_w = size.width.max(0.0);
                            let target_h = size.height.max(0.0);
                            let mut draw_pos = Vec2d { x: 0.0, y: 0.0 };
                            #[allow(unused_assignments)]
                            let mut draw_size = size;

                            let scale_x = if iw > 0.0 { target_w / iw } else { 1.0 };
                            let scale_y = if ih > 0.0 { target_h / ih } else { 1.0 };
                            let (final_w, final_h, center, use_cover) = match self.fit {
                                BoxFit::Contain | BoxFit::ScaleDown | BoxFit::None => {
                                    // Scale down if necessary (ScaleDown), otherwise contain.
                                    let mut scale = scale_x.min(scale_y);
                                    if let BoxFit::ScaleDown = self.fit {
                                        scale = scale.min(1.0);
                                    }
                                    let w = iw * scale * self.scale;
                                    let h = ih * scale * self.scale;
                                    (w, h, true, false)
                                }
                                BoxFit::FitWidth => {
                                    let w = target_w * self.scale;
                                    let h = if iw > 0.0 {
                                        w * (ih / iw)
                                    } else {
                                        target_h * self.scale
                                    };
                                    (w, h, true, false)
                                }
                                BoxFit::FitHeight => {
                                    let h = target_h * self.scale;
                                    let w = if ih > 0.0 {
                                        h * (iw / ih)
                                    } else {
                                        target_w * self.scale
                                    };
                                    (w, h, true, false)
                                }
                                BoxFit::Cover => {
                                    let scale = scale_x.max(scale_y);
                                    let w = iw * scale * self.scale;
                                    let h = ih * scale * self.scale;
                                    (w, h, true, true)
                                }
                                BoxFit::Fill => {
                                    // With keep_aspect_ratio=true, prefer Contain semantics to
                                    // avoid distortion
                                    let scale = scale_x.min(scale_y);
                                    let w = iw * scale * self.scale;
                                    let h = ih * scale * self.scale;
                                    (w, h, true, false)
                                }
                            };

                            // Center if requested
                            if center {
                                draw_pos.x = (target_w - final_w) * 0.5;
                                draw_pos.y = (target_h - final_h) * 0.5;
                            }
                            draw_size = ResolvedSize { width: final_w, height: final_h };

                            if use_cover {
                                // Clip to target box to emulate cover cropping
                                ctx.canvas
                                    .set_clip(Vec2d { x: 0.0, y: 0.0 }, size);
                                ctx.canvas
                                    .draw_image(id, draw_pos, draw_size);
                                ctx.canvas.clear_clip();
                            } else {
                                ctx.canvas
                                    .draw_image(id, draw_pos, draw_size);
                            }
                        } else {
                            // Fallback: invalid intrinsic size
                            let final_w = size.width * self.scale;
                            let final_h = size.height * self.scale;
                            let draw_pos = Vec2d {
                                x: (size.width - final_w) * 0.5,
                                y: (size.height - final_h) * 0.5,
                            };
                            let draw_size = ResolvedSize { width: final_w, height: final_h };
                            ctx.canvas
                                .draw_image(id, draw_pos, draw_size)
                        }
                    } else {
                        // Fallback when intrinsic size is unknown
                        let final_w = size.width * self.scale;
                        let final_h = size.height * self.scale;
                        let draw_pos = Vec2d {
                            x: (size.width - final_w) * 0.5,
                            y: (size.height - final_h) * 0.5,
                        };
                        let draw_size = ResolvedSize { width: final_w, height: final_h };
                        ctx.canvas
                            .draw_image(id, draw_pos, draw_size)
                    }
                } else {
                    // Not preserving aspect ratio: fill allocated box
                    let final_w = size.width * self.scale;
                    let final_h = size.height * self.scale;
                    let draw_pos =
                        Vec2d { x: (size.width - final_w) * 0.5, y: (size.height - final_h) * 0.5 };
                    let draw_size = ResolvedSize { width: final_w, height: final_h };
                    ctx.canvas
                        .draw_image(id, draw_pos, draw_size)
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

                        let pos = Vec2d { x: col as f32 * grid_size, y: row as f32 * grid_size };

                        let rect_size = ResolvedSize {
                            width: grid_size.min(size.width - pos.x),
                            height: grid_size.min(size.height - pos.y),
                        };

                        if rect_size.width > 0.0 && rect_size.height > 0.0 {
                            ctx.canvas
                                .fill_color_rect(pos, rect_size, color, [0.0; 4]);
                        }
                    }
                }
            }
        }
    }
}
