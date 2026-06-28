use std::cell::{Cell, UnsafeCell};
use crate::img_widget::image_widget::RawImageWidget;
use crate::img_widget::source::ImageSource;
use aimer_attribute::Dimension;
use aimer_attribute::size::Size;
use aimer_style::BoxFit;
use aimer_widget::base::BuildContext;
use aimer_widget::{Constructor, Element, LayoutCache, Widget};

/// Displays an image bundled with the app and registered under `[assets]` in
/// `aimer.toml`.
///
/// The `key` is the path declared in the manifest (relative to the project
/// root, e.g. `"assets/logo.png"`); it is resolved per platform at runtime
/// through [`ImageSource::Asset`]: from the APK on Android, the app bundle on
/// iOS/macOS, the project directory during desktop development, and via `fetch`
/// from the site root on web.
#[derive(Constructor)]
pub struct AssetImage {
    #[constructor(first, into)]
    pub key: String,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default)]
    pub fit: BoxFit,
    #[constructor(default)]
    pub error_widget: Option<Box<dyn Widget>>,
    #[constructor(default)]
    pub loading_widget: Option<Box<dyn Widget>>,
    #[constructor(default = 1.0, into)]
    pub scale: f32,
}

impl Widget for AssetImage {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawImageWidget {
            source: ImageSource::Asset(self.key.clone()),
            size: Size::new(self.width, self.height),
            fit: self.fit,
            keep_aspect_ratio: self.fit != BoxFit::Fill,
            error_element: self.error_widget.as_ref().map(|w| w.to_element(ctx)),
            loading_element: self.loading_widget.as_ref().map(|w| w.to_element(ctx)),
            cache: LayoutCache::new(),
            original_size: Cell::new(None),
            cached_id: UnsafeCell::new(None),
            scale: self.scale,
        })
    }

    fn debug_name(&self) -> &'static str {
        "AssetImage"
    }
}
