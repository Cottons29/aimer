use crate::aimer_widget;
use crate::blog::{BlogStore, LoadState, request_blog_detail};
use crate::router::AppRouter;
use crate::utils::app_padding;
use aimer::console::info;
use aimer::router::{NavigatorController, NavigatorInstance};
use aimer::style::TextAlign::MidCenter;
use aimer::{
    AnyWidget, BoxAlignment, BuildContext, Button, Color, Column, Container, MarkdownViewer,
    ProviderContext, ProviderHandle, Row, ScrollAxis, Scrollable, SizedBox, StatelessWidget, Svg,
    SvgAsset, SvgDocument, Text, Widget, ZeroSizedBox, widget,
};

#[widget(Stateless)]
#[derive(Clone)]
pub struct BlogDetailPage {
    id: String,
}

impl BlogDetailPage {
    pub fn boxing(id: String, _: &BuildContext) -> Box<dyn Widget> {
        Box::new(Self { id })
    }
}

impl StatelessWidget for BlogDetailPage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let store = ctx.watch::<BlogStore>();
        let state = store
            .details
            .get(&self.id)
            .cloned()
            .unwrap_or_default();
        if matches!(state, LoadState::Idle) {
            request_blog_detail(ProviderHandle::<BlogStore>::of(ctx), self.id.clone());
        }
        let navigator = NavigatorController::<AppRouter>::of(ctx);
        let content = match state {
            LoadState::Idle | LoadState::Loading => {
                crate::screen::blog::status_text("Loading blog…", Color::BLACK)
            }
            LoadState::Error(error) => crate::screen::blog::status_text(&error, Color::RED),
            LoadState::Ready(markdown) => MarkdownViewer::new()
                .markdown(markdown)
                .scrollable(false)
                .boxed(),
        };

        Container::new()
            .color(Color::WHITE)
            .child(
                Scrollable::new()
                    .axis(ScrollAxis::Vertical)
                    .child(
                        Container::new()
                            .padding(app_padding(ctx))
                            .child(
                                Column::new()
                                    .horizontal_alignment(BoxAlignment::Start)
                                    .children([
                                        back_button(navigator),
                                        SizedBox::new().height(24).boxed(),
                                        Row::new()
                                            .children([
                                                Container::new()
                                                    .width(200)
                                                    .color(Color::YELLOW)
                                                    .child(ZeroSizedBox)
                                                    .boxed(),
                                                content,
                                            ])
                                            .boxed(),
                                        SizedBox::new().height(48).boxed(),
                                    ]),
                            ),
                    ),
            )
            .boxed()
    }
}

// fn upload_time()

fn back_button(navigator: NavigatorInstance<AppRouter>) -> AnyWidget {
    let document = SvgDocument::from_svg(include_bytes!("../../assets/back-svgrepo-com.svg"))
        .expect("the bundled cat SVG should be valid");

    Button::new()
        .on_press(move || {
            if navigator.can_pop() { navigator.pop() } else { navigator.push(AppRouter::Blog) }
        })
        .child(
            Row::new().children([
                // Svg::new()
                Svg::new(document)
                    .width(16)
                    .height(16)
                    .boxed(),
                SizedBox::new()
                    .height(8)
                    .width(8)
                    .boxed(),
                Text::new("Back to blogs")
                    .text_align(MidCenter)
                    .boxed(),
            ]),
        )
        .boxed()
}

// back-svgrepo-com.svg
