use attribute::Dimension;
use std::cell::Cell;
use widget::base::{BuildContext, ResolvedSize, Size, Vec2d};
use widget::{Constructor, Drawable, Element, LayoutCache, Widget, WidgetConstructor};

#[derive(Debug, Clone)]
pub enum ImageSource {
    Id(u32),
    Path(String),
}
#[derive(Constructor)]
pub struct Image {
    pub source: ImageSource,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
}

impl Widget for Image {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawImageWidget {
            source: self.source.clone(),
            size: Size { width: self.width, height: self.height },
            cache: LayoutCache::new(),
            cached_id: Cell::new(None),
        })
    }

    fn debug_name(&self) -> &'static str {
        "Image"
    }
}

pub struct RawImageWidget {
    pub source: ImageSource,
    pub size: Size,
    pub cache: LayoutCache,
    cached_id: Cell<Option<u32>>,
}

impl RawImageWidget {
    pub fn new(image_id: u32, size: Size) -> Self {
        Self {
            source: ImageSource::Id(image_id),
            size,
            cache: LayoutCache::new(),
            cached_id: Cell::new(Some(image_id)),
        }
    }

    pub fn from_path(path: impl Into<String>, size: Size) -> Self {
        Self { source: ImageSource::Path(path.into()), size, cache: LayoutCache::new(), cached_id: Cell::new(None) }
    }
}

impl Element for RawImageWidget {
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
        "RawImageWidget"
    }
}

impl Drawable for RawImageWidget {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let image_id = if let Some(id) = self.cached_id.get() {
            id
        } else {
            let id = match &self.source {
                ImageSource::Id(id) => *id,
                ImageSource::Path(path) => ctx.canvas.load_image(path),
            };
            self.cached_id.set(Some(id));
            id
        };
        ctx.canvas.draw_image(image_id, Vec2d::default(), size);
    }
}
