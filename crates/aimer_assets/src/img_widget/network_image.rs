use std::cell::{Cell, UnsafeCell};
use crate::img_widget::image_widget::RawImageWidget;
use crate::img_widget::source::ImageSource;
use aimer_attribute::Dimension;
use aimer_attribute::size::Size;
use std::collections::HashMap;
use aimer_style::BoxFit;
use aimer_utils::debug;
use aimer_widget::base::BuildContext;
use aimer_widget::{Constructor, Element, LayoutCache, Widget};

#[derive(Constructor)]
pub struct NetworkImage {
    #[constructor(first, into)]
    pub url: String,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default)]
    pub fit: BoxFit,
    #[constructor(default)]
    pub header: Option<HashMap<String, String>>,
    #[constructor(default)]
    pub error_widget: Option<Box<dyn Widget>>,
    #[constructor(default)]
    pub loading_widget: Option<Box<dyn Widget>>,
    #[constructor(default)]
    pub delay: Option<u64>,
    #[constructor(default = 1.0, into)]
    pub scale: f32

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
            scale: self.scale
        })
    }

    fn debug_name(&self) -> &'static str {
        "NetworkImage"
    }
}
