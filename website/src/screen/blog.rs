use crate::blog_store::{BlogStore, BlogSummary, LoadState, request_blog_list};
use crate::router::AppRouter;
use crate::utils::{app_padding, is_mobile};
use aimer::console::error;
use aimer::router::NavigatorController;
use aimer::style::{
    FontStyle, FontWeight, TextDecoration, TextDecorationLine, TextDecorationStyle, TextOverflow,
    TextStyle, Theme, ThemeData,
};
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
        let theme = ThemeData::of(ctx);
        let store = ctx.watch::<BlogStore>();
        if matches!(store.list, LoadState::Idle) {
            request_blog_list(ctx, ProviderHandle::<BlogStore>::of(ctx));
        }

        let content = match &store.list {
            LoadState::Idle | LoadState::Loading => status_text(
                "Loading blogs…",
                theme
                    .on_background_color
                    .with_opacity(150),
            ),
            LoadState::Error(error) => {
                error!("{}", error);
                status_text(error, Color::RED)
            }
            LoadState::Ready(blogs) if blogs.is_empty() => status_text(
                "No blogs have been published yet.",
                theme
                    .on_background_color
                    .with_opacity(150),
            ),
            LoadState::Ready(blogs) => {
                let navigator = NavigatorController::<AppRouter>::of(ctx);
                blog_archive(blogs.clone(), navigator, is_mobile(ctx), &theme)
            }
        };

        Container::new()
            .color(theme.background_color)
            .child(
                Scrollable::new()
                    .axis(ScrollAxis::Vertical)
                    .child(
                        Container::new()
                            .padding(app_padding(ctx))
                            .child(
                                Column::new()
                                    .horizontal_alignment(BoxAlignment::Start)
                                    .overflow(OverflowBehavior::Wrap)
                                    .vertical_alignment(BoxAlignment::Start)
                                    .children([
                                        SizedBox::new().height(32).boxed(),
                                        Text::new("This is the main blog page for announcing the latest updates and news about the project. This page will share framework updates, implementation notes, and guides from the Aimer project.")
                                            // .text_align(TextAlign::MidCenter)
                                            .text_style(TextStyle::new()
                                                .text_overflow(TextOverflow::Wrap)
                                                .font_size(20)
                                                .color(theme.on_background_color)).boxed(),
                                        SizedBox::new().height(24).boxed(),

                                        Text::new("“Updates and guides for the Aimer„")
                                            .text_style(TextStyle::new()
                                                .font_size(if is_mobile(ctx) {18} else {20})
                                                .color(theme.on_background_color.with_opacity(180))
                                                .font_weight(FontWeight::Normal)
                                                .text_decoration(TextDecoration::new()
                                                    .line(TextDecorationLine::ITALIC | TextDecorationLine::UNDERLINE)
                                                    .style(TextDecorationStyle::Dashed)))
                                            .boxed(),

                                        SizedBox::new().height(70).boxed(),
                                        content,
                                        SizedBox::new().height(48).boxed(),
                                    ]),
                            ),
                    ),
            )
    }
}

fn blog_archive(
    blogs: Vec<BlogSummary>,
    navigator: NavigatorController<AppRouter>,
    mobile: bool,
    theme: &ThemeData,
) -> Box<dyn Widget> {
    let (heading_style, _) = archive_text_styles(theme);
    let mut current_year = None;
    let mut children = Vec::new();

    for blog in blogs {
        let year = archive_year(&blog.upload_time);
        if current_year.as_deref() != Some(year) {
            if current_year.is_some() {
                children.push(
                    SizedBox::new()
                        .height(40)
                        .boxed(),
                );
            }
            children.push(
                Text::new(archive_heading(&blog.upload_time))
                    .text_style(
                        heading_style
                            .font_size(if mobile { 40 } else { 48 })
                            .text_decoration(TextDecoration::Underline),
                    )
                    .boxed(),
            );
            children.push(
                SizedBox::new()
                    .height(if mobile { 28 } else { 40 })
                    .boxed(),
            );
            current_year = Some(year.to_owned());
        }
        children.push(blog_row(blog, navigator.clone(), mobile, theme));
        children.push(
            SizedBox::new()
                .height(if mobile { 28 } else { 36 })
                .boxed(),
        );
    }

    Column::new()
        .horizontal_alignment(BoxAlignment::Start)
        .children(children)
        .boxed()
}

fn blog_row(
    blog: BlogSummary,
    navigator: NavigatorController<AppRouter>,
    mobile: bool,
    theme: &ThemeData,
) -> AnyWidget {
    let route_id = blog.id.clone();
    let (style, hover_style) = blog_link_styles(theme);
    let (_, date_style) = archive_text_styles(theme);
    let date = Text::new(display_archive_date(&blog.upload_time))
        .text_style(date_style)
        .boxed();
    let link = TextButton::new(blog.title)
        .style(style)
        .hover_style(hover_style)
        .on_press(move || {
            navigator.push(AppRouter::BlogDetail {
                id: route_id.clone(),
            })
        })
        .boxed();

    if mobile {
        Column::new()
            .horizontal_alignment(BoxAlignment::Start)
            .children(vec![
                date,
                SizedBox::new()
                    .height(8)
                    .boxed(),
                link,
            ])
            .boxed()
    } else {
        Row::new()
            .vertical_alignment(BoxAlignment::Start)
            .children(vec![
                Container::new()
                    .width(140)
                    .child(date)
                    .boxed(),
                Expanded::new()
                    .child(link)
                    .boxed(),
            ])
            .boxed()
    }
}

fn archive_year(upload_time: &str) -> &str {
    upload_time
        .get(0..4)
        .filter(|year| {
            year.bytes()
                .all(|byte| byte.is_ascii_digit())
        })
        .unwrap_or("")
}

fn archive_heading(upload_time: &str) -> String {
    match archive_year(upload_time) {
        "" => "Posts".to_owned(),
        year => format!("Posts in {year}"),
    }
}

fn display_archive_date(upload_time: &str) -> String {
    let Some(month) = upload_time.get(5..7) else {
        return upload_time.to_owned();
    };
    let Some(day) = upload_time.get(8..10) else {
        return upload_time.to_owned();
    };
    let month = match month {
        "01" => "January",
        "02" => "February",
        "03" => "March",
        "04" => "April",
        "05" => "May",
        "06" => "June",
        "07" => "July",
        "08" => "August",
        "09" => "September",
        "10" => "October",
        "11" => "November",
        "12" => "December",
        _ => return upload_time.to_owned(),
    };
    let Ok(day) = day.parse::<u8>() else {
        return upload_time.to_owned();
    };
    if !(1..=31).contains(&day) {
        return upload_time.to_owned();
    }
    format!("{month} {day}")
}

fn archive_text_styles(theme: &ThemeData) -> (TextStyle, TextStyle) {
    let heading_style = TextStyle::new()
        .font_size(54)
        .font_weight(FontWeight::Bolder)
        .color(theme.on_background_color)
        .text_overflow(TextOverflow::Wrap);
    let date_style = TextStyle::new()
        .font_size(24)
        .font_weight(FontWeight::Bold)
        .color(
            theme
                .on_background_color
                .with_opacity(120),
        )
        .text_overflow(TextOverflow::Wrap);
    (heading_style, date_style)
}

fn blog_link_styles(theme: &ThemeData) -> (TextStyle, TextStyle) {
    let style = TextStyle::new()
        .font_size(24)
        .font_weight(FontWeight::Bold)
        .color(theme.on_background_color)
        .text_overflow(TextOverflow::Wrap);
    let hover_style = style
        .font_style(FontStyle::Italic)
        .text_decoration(
            TextDecoration::new().line(TextDecorationLine::ITALIC | TextDecorationLine::UNDERLINE),
        );

    (style, hover_style)
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

pub(crate) fn display_upload_time(upload_time: &str) -> String {
    match (upload_time.get(0..10), upload_time.get(11..16)) {
        (Some(date), Some(time)) => format!("{date} {time} UTC"),
        _ => upload_time.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use aimer::style::{FontStyle, TextDecorationLine};

    use super::*;

    #[test]
    fn archive_copy_uses_the_upload_year_and_readable_month_date() {
        assert_eq!(archive_heading("2026-07-18T02:22:00Z"), "Posts in 2026");
        assert_eq!(display_archive_date("2026-07-18T02:22:00Z"), "July 18");
    }

    #[test]
    fn malformed_archive_dates_fall_back_without_panicking() {
        assert_eq!(archive_heading("not-a-date"), "Posts");
        assert_eq!(display_archive_date("not-a-date"), "not-a-date");
        assert_eq!(
            display_archive_date("2026-13-18T02:22:00Z"),
            "2026-13-18T02:22:00Z"
        );
    }

    #[test]
    fn archive_text_uses_muted_theme_color_and_links_use_theme_foreground() {
        let theme = ThemeData::dark();
        let (heading_style, date_style) = archive_text_styles(&theme);
        let (link_style, _) = blog_link_styles(&theme);

        assert!(heading_style.color == theme.on_background_color);
        assert!(
            date_style.color
                == theme
                    .on_background_color
                    .with_opacity(120)
        );
        assert!(link_style.color == theme.on_background_color);
    }

    #[test]
    fn upload_time_is_presented_as_a_readable_utc_date() {
        assert_eq!(
            display_upload_time("2026-07-18T02:22:00Z"),
            "2026-07-18 02:22 UTC"
        );
    }
}
