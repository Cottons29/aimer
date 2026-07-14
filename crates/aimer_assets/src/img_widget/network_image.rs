use crate::img_widget::image_widget::RawImageWidget;
use crate::img_widget::source::ImageSource;
use aimer_attribute::Dimension;
use aimer_attribute::size::Size;
use aimer_style::BoxFit;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, LayoutCache, Widget};
use std::cell::{Cell, UnsafeCell};
use std::collections::HashMap;

pub struct NetworkImage {
    pub url: String,
    pub width: Dimension,
    pub height: Dimension,
    pub fit: BoxFit,
    pub header: Option<HashMap<String, String>>,
    pub error_widget: Option<Box<dyn Widget>>,
    pub loading_widget: Option<Box<dyn Widget>>,
    pub delay: Option<u64>,
    pub scale: f32,
}

impl NetworkImage {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            width: Dimension::default(),
            height: Dimension::default(),
            fit: BoxFit::default(),
            header: None,
            error_widget: None,
            loading_widget: None,
            delay: None,
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

    pub fn header(mut self, header: HashMap<String, String>) -> Self {
        self.header = Some(header);
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

    pub fn delay(mut self, delay: u64) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.scale = scale.into();
        self
    }
}

impl Widget for NetworkImage {
    #[track_caller]
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let source = match self.header.as_ref() {
            Some(header) => ImageSource::NetworkWithHeaders(self.url.clone(), header.clone()),
            None => ImageSource::Network(self.url.clone()),
        };

        // debug!("creating network image widget with url: {}", self.url);

        Box::new(RawImageWidget {
            source,
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
        "NetworkImage"
    }
}
