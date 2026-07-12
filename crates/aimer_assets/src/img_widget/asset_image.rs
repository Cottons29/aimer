use std::cell::{Cell, UnsafeCell};
use crate::img_widget::image_widget::RawImageWidget;
use crate::img_widget::source::ImageSource;
use aimer_attribute::Dimension;
use aimer_attribute::size::Size;
use aimer_style::BoxFit;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};

/// Displays an image bundled with the app and registered under `[assets]` in
/// `aimer.toml`.
///
/// The `key` is the path declared in the manifest (relative to the project
/// root, e.g. `"assets/logo.png"`); it is resolved per platform at runtime
/// through [`ImageSource::Asset`]: from the APK on Android, the app bundle on
/// iOS/macOS, the project directory during desktop development, and via `fetch`
/// from the site root on web.
pub struct AssetImage {
    pub key: String,
    pub width: Dimension,
    pub height: Dimension,
    pub fit: BoxFit,
    pub error_widget: Option<Box<dyn Widget>>,
    pub loading_widget: Option<Box<dyn Widget>>,
    pub scale: f32,
}

impl AssetImage {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            width: Dimension::default(),
            height: Dimension::default(),
            fit: BoxFit::default(),
            error_widget: None,
            loading_widget: None,
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

    pub fn error_widget(mut self, error_widget: impl Widget + 'static) -> Self {
        self.error_widget = Some(Box::new(error_widget));
        self
    }

    pub fn loading_widget(mut self, loading_widget: impl Widget + 'static) -> Self {
        self.loading_widget = Some(Box::new(loading_widget));
        self
    }

    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.scale = scale.into();
        self
    }
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
