use crate::aimer_widget;
use crate::blog_store::{BlogDetail, BlogStore, LoadState, request_blog_detail};
use crate::components::BlogBackButton;
use crate::router::AppRouter;
use crate::utils::{app_padding, is_mobile};
use aimer::console::info;
use aimer::router::NavigatorController;
use aimer::style::{FontWeight, LayoutSpacing, TextOverflow, TextStyle, Theme, ThemeData};
use aimer::{
    BoxAlignment, BuildContext, Color, Column, Container, Expanded, Key, MarkdownTheme,
    MarkdownViewer, ProviderContext, ProviderHandle, Row, ScrollAxis, Scrollable, SizedBox,
    StatelessWidget, Text, Widget, ZeroSizedBox, widget,
};

#[derive(Debug, PartialEq, Eq)]
enum DetailLayout {
    Horizontal,
    Vertical,
}

fn detail_layout(mobile: bool) -> DetailLayout {
    if mobile {
        DetailLayout::Vertical
    } else {
        DetailLayout::Horizontal
    }
}

fn metadata_fields(detail: &BlogDetail) -> [(&'static str, String); 3] {
    [
        (
            "Published",
            crate::screen::blog::display_upload_time(&detail.upload_time),
        ),
        ("Author", detail.author.clone()),
        ("Tags", detail.tags.join(", ")),
    ]
}

fn metadata_sidebar(detail: &BlogDetail, theme: &ThemeData) -> Box<dyn Widget> {
    let mut children = Vec::new();
    for (index, (label, value)) in metadata_fields(detail)
        .into_iter()
        .enumerate()
    {
        children.push(
            Text::new(label)
                .text_style(
                    TextStyle::new()
                        .font_size(15)
                        .font_weight(FontWeight::Bold)
                        .color(
                            theme
                                .on_background_color
                                .darken(0.2)
                                .with_alpha(0.4),
                        ),
                )
                .boxed(),
        );
        children.push(
            SizedBox::new()
                .height(8)
                .boxed(),
        );
        children.push(
            Text::new(value)
                .text_style(
                    TextStyle::new()
                        .font_size(17)
                        .color(theme.on_background_color)
                        .text_overflow(TextOverflow::Wrap),
                )
                .boxed(),
        );
        if index < 2 {
            children.push(
                SizedBox::new()
                    .height(16)
                    .boxed(),
            );
            children.push(
                Container::new()
                    .height(1)
                    .color(
                        theme
                            .on_background_color
                            .with_alpha(0.35),
                    )
                    .child(ZeroSizedBox)
                    .boxed(),
            );
            children.push(
                SizedBox::new()
                    .height(16)
                    .boxed(),
            );
        }
    }
    Column::new()
        .horizontal_alignment(BoxAlignment::Start)
        .children(children)
        .boxed()
}

fn themed_markdown(theme: &ThemeData) -> MarkdownTheme {
    let mut markdown = MarkdownTheme::default();
    markdown.body = markdown
        .body
        .color(theme.on_background_color);
    markdown.headings = markdown
        .headings
        .map(|style| style.color(theme.on_background_color));
    markdown.blockquote = markdown.blockquote.color(
        theme
            .on_background_color
            .with_opacity(180),
    );
    markdown.code_block = markdown
        .code_block
        .color(theme.on_surface_color);
    markdown.inline_code = markdown
        .inline_code
        .color(theme.on_surface_color)
        .background_color(
            theme
                .surface_color
                .darken(0.1)
                .with_alpha(0.4),
        );
    markdown.link = markdown
        .link
        .color(theme.primary_color);
    markdown.link_hover_color = theme
        .primary_color
        .lighten(0.2);
    markdown.code_background = theme
        .surface_color
        .darken(0.1)
        .with_alpha(0.4);
    markdown.quote_background = theme
        .surface_color
        .darken(0.1)
        .with_alpha(0.4);
    markdown.rule_color = theme
        .on_background_color
        .with_opacity(72);
    markdown.table_header_background = theme.surface_color;
    markdown.table_cell_background = theme.background_color;
    markdown
}

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
        let theme = ThemeData::of(ctx);
        let store = ctx.watch::<BlogStore>();
        let state = store
            .details
            .get(&self.id)
            .cloned()
            .unwrap_or_default();
        if matches!(state, LoadState::Idle) {
            request_blog_detail(ctx, ProviderHandle::<BlogStore>::of(ctx), self.id.clone());
        }
        let navigator = NavigatorController::<AppRouter>::of(ctx);
        let (content, metadata, blog_id) = match state {
            LoadState::Idle | LoadState::Loading => (
                crate::screen::blog::status_text("Loading blog…", theme.on_background_color),
                None,
                None,
            ),
            LoadState::Error(error) => (
                crate::screen::blog::status_text(&error, Color::RED),
                None,
                None,
            ),
            LoadState::Ready(detail) => {
                // info!("Markdown: {}", detail.markdown);
                let metadata = metadata_sidebar(&detail, &theme);
                let markdown_theme = themed_markdown(&theme);
                let content = MarkdownViewer::new()
                    .markdown(detail.markdown)
                    .theme(markdown_theme)
                    .scrollable(false)
                    .boxed();
                (content, Some(metadata), Some(detail.id.clone()))
            }
        };

        let key = match blog_id {
            Some(blog_id) => Key::from(blog_id),
            None => Key::unique(),
        };

        let back_button = Container::new()
            .height(40)
            .width(120)
            .child(BlogBackButton::new().on_click(move || {
                if navigator.can_pop() {
                    navigator.pop()
                } else {
                    navigator.push(AppRouter::Blog)
                }
            }))
            .boxed();
        let mut sidebar_children = vec![back_button];
        if let Some(metadata) = metadata {
            sidebar_children.push(
                SizedBox::new()
                    .height(32)
                    .boxed(),
            );
            sidebar_children.push(metadata);
        }

        let sidebar = Column::new()
            .horizontal_alignment(BoxAlignment::Start)
            .children(sidebar_children);

        let detail = match detail_layout(is_mobile(ctx)) {
            DetailLayout::Horizontal => Row::new()
                .vertical_alignment(BoxAlignment::Start)
                .children([
                    Expanded::new()
                        .flex(1.2)
                        .child(
                            Container::new()
                                .padding(LayoutSpacing::new().right(16))
                                .child(sidebar),
                        )
                        .boxed(),
                    Expanded::new()
                        .flex(4.0)
                        .child(content)
                        .boxed(),
                ])
                .boxed(),
            DetailLayout::Vertical => Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .children([
                    sidebar.boxed(),
                    SizedBox::new()
                        .height(32)
                        .boxed(),
                    content,
                ])
                .boxed(),
        };



        Container::new()
            .color(theme.background_color)
            .child(
                Scrollable::new()
                    .key(key)
                    .axis(ScrollAxis::Vertical)
                    .child(
                        Container::new()
                            .padding(app_padding(ctx))
                            .child(
                                Column::new()
                                    .horizontal_alignment(BoxAlignment::Start)
                                    .children([
                                        detail,
                                        SizedBox::new()
                                            .height(48)
                                            .boxed(),
                                    ]),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_layout_is_horizontal_on_desktop_and_vertical_on_mobile() {
        assert_eq!(detail_layout(false), DetailLayout::Horizontal);
        assert_eq!(detail_layout(true), DetailLayout::Vertical);
    }

    #[test]
    fn markdown_theme_uses_website_semantic_colors() {
        let theme = ThemeData::dark();
        let markdown = themed_markdown(&theme);

        assert!(markdown.body.color == theme.on_background_color);
        assert!(
            markdown
                .headings
                .iter()
                .all(|style| style.color == theme.on_background_color)
        );
        assert_eq!(
            markdown.code_background,
            theme
                .surface_color
                .darken(0.1)
                .with_alpha(0.4)
        );
        assert_eq!(markdown.quote_background, markdown.code_background);
        assert_eq!(markdown.table_cell_background, theme.background_color);
        assert_eq!(markdown.inline_code.color, Some(theme.on_surface_color));
        assert_eq!(markdown.link.color, Some(theme.primary_color));
    }

    #[test]
    fn sidebar_contains_publication_time_author_and_tags() {
        let fields = metadata_fields(&BlogDetail {
            id: "first-post".to_owned(),
            upload_time: "2026-07-18T02:22:00Z".to_owned(),
            title: "First post".to_owned(),
            author: "Aimer Team".to_owned(),
            tags: vec!["Rust".to_owned(), "GUI".to_owned()],
            markdown: "# First post".to_owned(),
        });

        assert_eq!(
            fields,
            [
                ("Published", "2026-07-18 02:22 UTC".to_owned()),
                ("Author", "Aimer Team".to_owned()),
                ("Tags", "Rust, GUI".to_owned()),
            ]
        );
    }
}
