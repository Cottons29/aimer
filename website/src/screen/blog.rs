use std::rc::Rc;

use crate::blog::{BlogStore, BlogSummary, LoadState, request_blog_detail, request_blog_list};
use crate::router::AppRouter;
use crate::utils::{app_padding, mobile_title};
use aimer::console::{error, info};
use aimer::router::NavigatorController;
use aimer::style::{BoxDecoration, FontWeight, LayoutSpacing, Spacing, TextOverflow, TextStyle};
use aimer::{BuildContext, Widget, widget, *};

#[widget(Stateless)]
#[derive(Clone)]
pub struct BlogListPage;

impl BlogListPage {
    pub fn boxing(_: &BuildContext) -> Box<dyn Widget> {
        Box::new(Self)
    }
}
impl StatelessWidget for BlogListPage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let store = ctx.watch::<BlogStore>();
        if matches!(store.list, LoadState::Idle) {
            request_blog_list(ProviderHandle::<BlogStore>::of(ctx));
        }

        let content = match store.list {
            LoadState::Idle | LoadState::Loading => status_text("Loading blogs…", Color::BLACK),
            LoadState::Error(error) => {
                error!("{}", error);
                status_text(&error, Color::RED)
            }
            LoadState::Ready(blogs) if blogs.is_empty() => {
                status_text("No blogs have been published yet.", Color::BLACK)
            }
            LoadState::Ready(blogs) => {
                let navigator = NavigatorController::<AppRouter>::of(ctx);
                Column::new()
                    .horizontal_alignment(BoxAlignment::Start)
                    .children(
                        blogs
                            .into_iter()
                            .map(|blog| blog_row(blog, navigator.clone()))
                            .collect::<Vec<_>>(),
                    )
                    .boxed()
            }
        };

        page("Blog", content, ctx)
    }
}

fn page(title: &str, content: AnyWidget, ctx: &BuildContext) -> AnyWidget {
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
                                .children(vec![
                                    SizedBox::new().height(24).boxed(),
                                    Text::new(title)
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(mobile_title(ctx))
                                                .color(Color::BLACK)
                                                .font_weight(FontWeight::Bolder),
                                        )
                                        .boxed(),
                                    SizedBox::new().height(24).boxed(),
                                    content,
                                    SizedBox::new().height(48).boxed(),
                                ]),
                        ),
                ),
        )
        .boxed()
}

fn blog_row(blog: BlogSummary, navigator: Rc<NavigatorController<AppRouter>>) -> Box<dyn Widget> {
    let route_id = blog.id.clone();
    Button::new()
        .decoration(
            BoxDecoration::new()
                .background_color(Color::GRAY.with_alpha(0.2))
                .border_radius(8),
        )
        .on_press(move || navigator.push(AppRouter::BlogDetail { id: route_id.clone() }))
        .child(
            Container::new()
                .padding(LayoutSpacing::all(Spacing::Px(16)))
                .child(
                    Column::new()
                        .horizontal_alignment(BoxAlignment::Start)
                        .children(vec![
                            Text::new(display_upload_time(&blog.upload_time))
                                .text_style(
                                    TextStyle::new()
                                        .font_size(14)
                                        .color(Color::BLACK.with_opacity(160)),
                                )
                                .boxed(),
                            SizedBox::new().height(6).boxed(),
                            Text::new(blog.title)
                                .text_style(
                                    TextStyle::new()
                                        .font_size(24)
                                        .font_weight(FontWeight::Bold)
                                        .color(Color::BLACK)
                                        .text_overflow(TextOverflow::Wrap),
                                )
                                .boxed(),
                        ]),
                ),
        )
        .boxed()
}

pub(crate) fn status_text(message: &str, color: Color) -> Box<dyn Widget> {
    Text::new(message.to_owned())
        .text_style(
            TextStyle::new()
                .font_size(18)
                .color(color)
                .text_overflow(TextOverflow::Wrap),
        )
        .boxed()
}

fn display_upload_time(upload_time: &str) -> String {
    match (upload_time.get(0..10), upload_time.get(11..16)) {
        (Some(date), Some(time)) => format!("{date} {time} UTC"),
        _ => upload_time.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_time_is_presented_as_a_readable_utc_date() {
        assert_eq!(display_upload_time("2026-07-18T02:22:00Z"), "2026-07-18 02:22 UTC");
    }
}
