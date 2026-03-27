use std::collections::HashMap;
use std::sync::Arc;
use attribute::Dimension;
use widget::{Element, Widget, WidgetConstructor, State, StatefulWidget, StatefulElement};
use widget::base::BuildContext;
use widget::style::BoxFit;
use crate::single_frame::source::ImageSource;
use crate::single_frame::image_widget::Image;
use crate::ImageProvider;

#[derive(WidgetConstructor)]
pub struct NetworkImage<W: Widget + Send + Sync + Clone + 'static> {
    #[constructor(first, into)]
    pub url: String,
    #[constructor(default, into)]
    pub width: Dimension,
    #[constructor(default, into)]
    pub height: Dimension,
    #[constructor(default)]
    pub fit: BoxFit,
    #[constructor(default = true)]
    pub keep_aspect_ratio: bool,
    #[constructor(default)]
    pub header: HashMap<String, String>,
    #[constructor(default)]
    pub error_widget: Option<W>,
    #[constructor(default)]
    pub loading_widget: Option<W>,
    #[constructor(default)]
    pub delay: Option<u64>,
}

impl<W: Widget + Send + Sync + Clone + 'static> StatefulWidget for NetworkImage<W> {
    type State = NetworkImageState<W>;

    fn create_state(&self) -> Self::State {
        NetworkImageState {
            url: self.url.clone(),
            width: self.width,
            height: self.height,
            fit: self.fit,
            keep_aspect_ratio: self.keep_aspect_ratio,
            header: self.header.clone(),
            error_widget: self.error_widget.as_ref().map(|w| Arc::new((*w).clone())),
            loading_widget: self.loading_widget.as_ref().map(|w| Arc::new((*w).clone())),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct NetworkImageState<W> {
    url: String,
    width: Dimension,
    height: Dimension,
    fit: BoxFit,
    keep_aspect_ratio: bool,
    header: HashMap<String, String>,
    error_widget: Option<Arc<W>>,
    loading_widget: Option<Arc<W>>,
    _marker: std::marker::PhantomData<W>,
}

impl<W: Widget + Send + Sync + Clone + 'static> State<NetworkImage<W>> for NetworkImageState<W> {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let source = if self.header.is_empty() {
            ImageSource::Network(self.url.clone())
        } else {
            ImageSource::NetworkWithHeaders(self.url.clone(), self.header.clone())
        };

        match source.get_image(ctx) {
            Ok(_) => Box::new(Image {
                source,
                width: self.width,
                height: self.height,
                fit: self.fit,
                keep_aspect_ratio: self.keep_aspect_ratio,
            }) as Box<dyn Widget>,
            Err("Loading") => {
                if let Some(ref loading) = self.loading_widget {
                    Box::new(ArcWidgetWrapper(loading.clone())) as Box<dyn Widget>
                } else {
                    Box::new(Image {
                        source,
                        width: self.width,
                        height: self.height,
                        fit: self.fit,
                        keep_aspect_ratio: self.keep_aspect_ratio,
                    }) as Box<dyn Widget>
                }
            }
            Err(_) => {
                if let Some(ref error) = self.error_widget {
                    Box::new(ArcWidgetWrapper(error.clone())) as Box<dyn Widget>
                } else {
                    Box::new(Image {
                        source,
                        width: self.width,
                        height: self.height,
                        fit: self.fit,
                        keep_aspect_ratio: self.keep_aspect_ratio,
                    }) as Box<dyn Widget>
                }
            }
        }
    }
}

struct ArcWidgetWrapper<W>(Arc<W>);
impl<W: Widget + 'static> Widget for ArcWidgetWrapper<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        self.0.to_element(ctx)
    }
    fn debug_name(&self) -> &'static str { "ArcWidgetWrapper" }
}

impl<W: Widget + Send + Sync + Clone + 'static> Widget for NetworkImage<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let (element, _) = StatefulElement::new(self, ctx);
        Box::new(element)
    }

    fn debug_name(&self) -> &'static str {
        "NetworkImage"
    }
}