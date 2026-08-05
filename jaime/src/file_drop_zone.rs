//! Two drop zones competing for files dragged in from the desktop.
//!
//! The left zone takes images only; the right one takes anything. Only the zone
//! under the cursor highlights and only that one receives the files, and a
//! multi-file drag arrives as a single batch — the platform reports it one file
//! at a time, with no marker saying the batch has ended.
//!
//! # Platforms
//!
//! winit's web backend emits no file-drag events, so this demo runs but never
//! reacts when `jaime` is built for the browser. Run it natively.

use std::path::{Path, PathBuf};

use aimer::style::*;
use aimer::*;

const IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// Starts the file drop showcase.
pub fn start_file_drop_zone_example() {
    AimerApp::start(FileDropShowcase::new().boxed())
}

#[widget(Stateful)]
pub struct FileDropShowcase {}

impl Default for FileDropShowcase {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDropShowcase {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct FileDropShowcaseState {
    /// The last batch each zone received, and how many batches it has seen.
    images: (Vec<PathBuf>, usize),
    anything: (Vec<PathBuf>, usize),
    updater: StateUpdater<Self>,
}

impl StatefulWidget for FileDropShowcase {
    type State = FileDropShowcaseState;

    fn create_state(&self) -> Self::State {
        FileDropShowcaseState {
            images: (Vec::new(), 0),
            anything: (Vec::new(), 0),
            updater: StateUpdater::empty(),
        }
    }
}

impl State<FileDropShowcase> for FileDropShowcaseState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::Rgb(17, 24, 39))
            .padding(LayoutSpacing::all(Spacing::Px(32)))
            .child(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Start)
                    .children([
                        heading("Drop zones: drag files in from Finder"),
                        subheading(
                            "The left zone takes images only. Drop several at once: \
                             they arrive as one batch.",
                        ),
                        SizedBox::new().height(24).boxed(),
                        Expanded::new()
                            .child(
                                Column::new()
                                    .vertical_alignment(BoxAlignment::Start)
                                    .gaps(LayoutSpacing::all(Spacing::Px(16)))
                                    .children([
                                        Expanded::new().child(self.image_zone()),
                                        Expanded::new().child(self.anything_zone()),
                                    ]),
                            )
                            .boxed(),
                    ]),
            )
    }
}

impl FileDropShowcaseState {
    /// A zone restricted to images: anything else passes straight through it.
    fn image_zone(&self) -> AnyWidget {
        let updater = self.updater.clone();
        let received = self.images.clone();

        DropZone::new()
            .extensions(IMAGE_EXTENSIONS)
            .on_drop(move |paths: Vec<PathBuf>| {
                updater.set_state(move |state| {
                    state.images = (paths, state.images.1 + 1);
                });
            })
            .child(move |state: DragTargetState| {
                zone_body(
                    "Images only",
                    Color::Rgb(59, 130, 246),
                    state,
                    &received.0,
                    received.1,
                )
            })
            .boxed()
    }

    /// A zone with no filter at all.
    fn anything_zone(&self) -> AnyWidget {
        let updater = self.updater.clone();
        let received = self.anything.clone();

        DropZone::new()
            .on_drop(move |paths: Vec<PathBuf>| {
                updater.set_state(move |state| {
                    state.anything = (paths, state.anything.1 + 1);
                });
            })
            .child(move |state: DragTargetState| {
                zone_body(
                    "Anything",
                    Color::Rgb(34, 197, 94),
                    state,
                    &received.0,
                    received.1,
                )
            })
            .boxed()
    }
}

/// One zone: a dashed-looking panel that lights up while files hover over it
/// and lists what it last received.
fn zone_body(
    title: &'static str,
    accent: Color,
    state: DragTargetState,
    paths: &[PathBuf],
    batches: usize,
) -> AnyWidget {
    let border = if state.is_hovered {
        accent
    } else {
        Color::Rgb(55, 65, 81)
    };

    let mut rows = vec![
        zone_heading(title, accent),
        zone_summary(paths.len(), batches),
    ];
    rows.extend(paths.iter().take(8).map(|path| path_row(path.as_path())));

    Container::new()
        // .width(Dimension::Px(320.0))
        // .height(Dimension::Percent(100.0))
        .padding(LayoutSpacing::all(Spacing::Px(14)))
        .box_decoration(
            BoxDecoration::new()
                .background_color(Color::Rgb(31, 41, 55))
                .border_radius(14)
                .border(BoxBorder::all(
                    BorderSlice::new()
                        .style(BorderStyle::Solid)
                        .stroke(Stroke::Px(2.0))
                        .color(border),
                )),
        )
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .gaps(LayoutSpacing::new().bottom(6))
                .children(rows),
        )
        .boxed()
}

fn zone_heading(title: &'static str, accent: Color) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(28.0))
        .child(
            Text::new(title)
                .text_align(TextAlign::MidLeft)
                .text_style(TextStyle::new().font_size(17).color(accent)),
        )
        .boxed()
}

/// The two numbers that make the coalescing visible: five files in one batch is
/// not the same as five batches of one.
fn zone_summary(files: usize, batches: usize) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(22.0))
        .child(
            Text::new(format!("{files} file(s) in {batches} batch(es)"))
                .text_align(TextAlign::MidLeft)
                .text_style(
                    TextStyle::new()
                        .font_size(14)
                        .color(Color::Rgb(148, 163, 184)),
                ),
        )
        .boxed()
}

fn path_row(path: &Path) -> AnyWidget {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unnamed>")
        .to_owned();

    Container::new()
        .height(Dimension::Px(20.0))
        .child(
            Text::new(name)
                .text_align(TextAlign::MidLeft)
                .text_style(TextStyle::new().font_size(13).color(Color::WHITE)),
        )
        .boxed()
}

fn heading(text: &'static str) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(34.0))
        .child(
            Text::new(text)
                .text_align(TextAlign::MidLeft)
                .text_style(TextStyle::new().font_size(24).color(Color::WHITE)),
        )
        .boxed()
}

fn subheading(text: &'static str) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(24.0))
        .child(
            Text::new(text)
                .text_align(TextAlign::MidLeft)
                .text_style(
                    TextStyle::new()
                        .font_size(15)
                        .color(Color::Rgb(148, 163, 184)),
                ),
        )
        .boxed()
}
