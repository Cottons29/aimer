use crate::{ ImageProvider};
use attribute::Dimension;
use std::cell::Cell;
use widget::base::{BuildContext, Color, Colors, ResolvedSize, Size, Vec2d};
use widget::style::BoxFit;
use widget::{Constructor, Drawable, Element, LayoutCache, Widget};

#[derive(Constructor)]
pub struct Image<P: ImageProvider> {
    pub source: P,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default)]
    pub fit: BoxFit,
    #[constructor(default = true)]
    pub keep_aspect_ratio: bool,
}

impl<P: ImageProvider + 'static> Widget for Image<P> {
    fn to_element(&self, _: &BuildContext) -> Box<dyn Element> {
        Box::new(RawImageWidget {
            source: self.source.clone(),
            size: Size { width: self.width, height: self.height },
            cache: LayoutCache::new(),
            fit: self.fit,
            keep_aspect_ratio: self.keep_aspect_ratio,
            original_size: Cell::new(None),
            cached_id: Cell::new(None),
        })
    }

    fn debug_name(&self) -> &'static str {
        "Image"
    }
}

pub struct RawImageWidget<P: ImageProvider> {
    pub source: P,
    pub size: Size,
    pub cache: LayoutCache,
    pub fit: BoxFit,
    pub keep_aspect_ratio: bool,
    pub original_size: Cell<Option<Size>>,
    cached_id: Cell<Option<Result<u32, &'static str>>>,
}

impl<P: ImageProvider> Element for RawImageWidget<P> {
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

    fn invalidate_layout(&self) {
        self.cache.invalidate();
    }

    fn debug_name(&self) -> &'static str {
        "RawImageElement"
    }
}

impl<P: ImageProvider> Drawable for RawImageWidget<P> {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let image_result = if let Some(result) = self.cached_id.get() {
            if result == Err("Loading") {
                let r = self.source.get_image(&ctx);
                if r != Err("Loading") {
                    self.cached_id.set(Some(r));
                }
                r
            } else {
                result
            }
        } else {
            let result = self.source.get_image(&ctx);
            if result != Err("Loading") {
                self.cached_id.set(Some(result));
            }
            result
        };

        match image_result {
            Ok(id) => {
                if self.keep_aspect_ratio {
                    if let Some((iw, ih)) = ctx.canvas.get_image_size(id) {
                        let iw = iw as f32;
                        let ih = ih as f32;
                        if iw > 0.0 && ih > 0.0 {
                            let target_w = size.width.max(0.0);
                            let target_h = size.height.max(0.0);
                            let mut draw_pos = Vec2d { x: 0.0, y: 0.0 };
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
                                    let w = iw * scale;
                                    let h = ih * scale;
                                    (w, h, true, false)
                                }
                                BoxFit::FitWidth => {
                                    let w = target_w;
                                    let h = if iw > 0.0 { w * (ih / iw) } else { target_h };
                                    (w, h, true, false)
                                }
                                BoxFit::FitHeight => {
                                    let h = target_h;
                                    let w = if ih > 0.0 { h * (iw / ih) } else { target_w };
                                    (w, h, true, false)
                                }
                                BoxFit::Cover => {
                                    let scale = scale_x.max(scale_y);
                                    let w = iw * scale;
                                    let h = ih * scale;
                                    (w, h, true, true)
                                }
                                BoxFit::Fill => {
                                    // With keep_aspect_ratio=true, prefer Contain semantics to avoid distortion
                                    let scale = scale_x.min(scale_y);
                                    let w = iw * scale;
                                    let h = ih * scale;
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
                                ctx.canvas.set_clip(Vec2d { x: 0.0, y: 0.0 }, size);
                                ctx.canvas.draw_image(id, draw_pos, draw_size);
                                ctx.canvas.clear_clip();
                            } else {
                                ctx.canvas.draw_image(id, draw_pos, draw_size);
                            }
                        } else {
                            // Fallback: invalid intrinsic size
                            ctx.canvas.draw_image(id, Vec2d::default(), size)
                        }
                    } else {
                        // Fallback when intrinsic size is unknown
                        ctx.canvas.draw_image(id, Vec2d::default(), size)
                    }
                } else {
                    // Not preserving aspect ratio: fill allocated box
                    ctx.canvas.draw_image(id, Vec2d::default(), size)
                }
            },
            Err(_) => {
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
                            ctx.canvas.fill_color_rect(pos, rect_size, color, 0.0);
                        }
                    }
                }
            }
        }
    }
}
