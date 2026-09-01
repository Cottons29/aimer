use aimer::macros::widget;
use aimer::style::*;
use aimer::*;

use crate::accessibility_example::accessibility_example;
use crate::animatable_example::animatable_example;
use crate::animated::MyAnimatedList;
use crate::animated_layout_example::animated_layout_example;
use crate::animated_theme::AnimatedThemeExample;
use crate::assets_media_example::assets_media_example;
use crate::async_builder::async_builder_example;
use crate::color_sync::ColorSync;
use crate::custom_animated_theme::custom_animated_theme_example;
use crate::custom_font::custom_font_example;
use crate::custom_shape_example::custom_shape_example;
use crate::data_view_example::data_view_example;
use crate::dnd_completion_example::dnd_completion_example;
use crate::drag_and_drop::DragBoard;
use crate::feedback_example::feedback_example;
use crate::file_drop_zone::FileDropShowcase;
use crate::floating::FloatingShowcase;
use crate::focus_node_example::FocusNodeExample;
use crate::form_example::form_example;
use crate::glass_liquid_example::glass_liquid_example;
use crate::http_request_button::http_request_button_example;
use crate::i18n_example::i18n_example;
use crate::justify_content_example::justify_content_example;
use crate::loading_animation::loading_animation_example;
use crate::markdown_example::{custom_markdown_viewer, jaime_markdown_viewer};
use crate::modal::modal_example;
use crate::navigation_example::navigation_example;
use crate::overflow_behavior_example::overflow_behavior_example;
use crate::panic_recovery::panic_recovery_example;
use crate::picker_example::picker_example;
use crate::range_controls_example::RangeControlsExample;
use crate::resizable_example::ResizableShowcase;
use crate::routing::state_router_widget;
use crate::routing_context_example::routing_context_example;
use crate::selectable_text::selectable_text_example;
use crate::selection_controls_example::selection_controls_example;
use crate::stateful::CounterWidget;
use crate::stateful_2::MyList;
use crate::storage_example::storage_example;
use crate::style_tokens_example::style_tokens_example;
use crate::svg_example::svg_example as svg_completion_example;
use crate::svg_test::svg_example;
use crate::system_theme::SystemThemeExample;
use crate::test_animation::TestFadingAnimation;
use crate::text_area_example::TextAreaExample;
use crate::text_field_example::TextFieldExample;
use crate::text_properties_example::text_properties_example;
use crate::theme;
use crate::window_example::WindowShowcase;

const SIDEBAR_WIDTH: f32 = 292.0;

/// Every page currently available in the Jaime showcase.
///
/// The enum is the small interface between the shared navigation shell and an
/// example module. Adding a page means adding one descriptor here and one
/// branch to [`build_example`]; the example's implementation remains in its
/// own module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExampleId {
    Accessibility,
    ChoiceControls,
    RangeControls,
    Forms,
    Pickers,
    Feedback,
    Navigation,
    RoutingContext,
    Animatable,
    DataView,
    DndCompletion,
    StyleTokens,
    I18n,
    SvgCompletion,
    AssetsMedia,
    GlassLiquid,
    Storage,
    CustomShape,
    AnimatedLayout,
    TextProperties,
    Window,
    Resizable,
    FileDrop,
    Floating,
    FocusNode,
    TextArea,
    TextField,
    Markdown,
    CustomMarkdown,
    Counter,
    StatefulList,
    AsyncBuilder,
    SelectableText,
    SystemTheme,
    AnimatedTheme,
    CustomAnimatedTheme,
    DragAndDrop,
    HttpRequest,
    Loading,
    Overflow,
    JustifyContent,
    Svg,
    CustomFont,
    PanicRecovery,
    Modal,
    ColorSync,
    AnimatedList,
    Animation,
    Routing,
    Text,
    Positioned,
    BorderOutline,
    AssetImage,
}

const EXAMPLES: &[ExampleId] = &[
    ExampleId::Accessibility,
    ExampleId::ChoiceControls,
    ExampleId::RangeControls,
    ExampleId::Forms,
    ExampleId::Pickers,
    ExampleId::Feedback,
    ExampleId::Navigation,
    ExampleId::RoutingContext,
    ExampleId::Animatable,
    ExampleId::DataView,
    ExampleId::DndCompletion,
    ExampleId::StyleTokens,
    ExampleId::I18n,
    ExampleId::SvgCompletion,
    ExampleId::AssetsMedia,
    ExampleId::GlassLiquid,
    ExampleId::Storage,
    ExampleId::CustomShape,
    ExampleId::AnimatedLayout,
    ExampleId::TextProperties,
    ExampleId::Window,
    ExampleId::Resizable,
    ExampleId::FileDrop,
    ExampleId::Floating,
    ExampleId::FocusNode,
    ExampleId::TextArea,
    ExampleId::TextField,
    ExampleId::Markdown,
    ExampleId::CustomMarkdown,
    ExampleId::Counter,
    ExampleId::StatefulList,
    ExampleId::AsyncBuilder,
    ExampleId::SelectableText,
    ExampleId::SystemTheme,
    ExampleId::AnimatedTheme,
    ExampleId::CustomAnimatedTheme,
    ExampleId::DragAndDrop,
    ExampleId::HttpRequest,
    ExampleId::Loading,
    ExampleId::Overflow,
    ExampleId::JustifyContent,
    ExampleId::Svg,
    ExampleId::CustomFont,
    ExampleId::PanicRecovery,
    ExampleId::Modal,
    ExampleId::ColorSync,
    ExampleId::AnimatedList,
    ExampleId::Animation,
    ExampleId::Routing,
    ExampleId::Text,
    ExampleId::Positioned,
    ExampleId::BorderOutline,
    ExampleId::AssetImage,
];

impl ExampleId {
    #[inline]
    const fn label(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility semantics",
            Self::ChoiceControls => "Choice controls",
            Self::RangeControls => "Range controls",
            Self::Forms => "Forms and validation",
            Self::Pickers => "Pickers",
            Self::Feedback => "Feedback and overlays",
            Self::Navigation => "Navigation widgets",
            Self::RoutingContext => "Route-child context",
            Self::Animatable => "Derived Animatable values",
            Self::DataView => "Data views",
            Self::DndCompletion => "DnD completion",
            Self::StyleTokens => "Design tokens",
            Self::I18n => "Localization",
            Self::SvgCompletion => "SVG completion",
            Self::AssetsMedia => "Assets and media",
            Self::GlassLiquid => "Glass/Liquid",
            Self::Storage => "Durable storage",
            Self::CustomShape => "Custom shapes",
            Self::AnimatedLayout => "Animated layout",
            Self::TextProperties => "Text properties",
            Self::Window => "Window showcase",
            Self::Resizable => "Resizable",
            Self::FileDrop => "File drop zone",
            Self::Floating => "Floating panels",
            Self::FocusNode => "FocusNode",
            Self::TextArea => "TextArea",
            Self::TextField => "TextField",
            Self::Markdown => "Markdown",
            Self::CustomMarkdown => "Custom Markdown",
            Self::Counter => "Stateful counter",
            Self::StatefulList => "Stateful list",
            Self::AsyncBuilder => "AsyncBuilder",
            Self::SelectableText => "Selectable text",
            Self::SystemTheme => "System theme",
            Self::AnimatedTheme => "Animated theme",
            Self::CustomAnimatedTheme => "Custom animated theme",
            Self::DragAndDrop => "Drag and drop",
            Self::HttpRequest => "HTTP request",
            Self::Loading => "Loading animation",
            Self::Overflow => "Overflow behavior",
            Self::JustifyContent => "JustifyContent",
            Self::Svg => "SVG animation",
            Self::CustomFont => "Custom font",
            Self::PanicRecovery => "Panic recovery",
            Self::Modal => "Modal",
            Self::ColorSync => "Color sync",
            Self::AnimatedList => "Animated list",
            Self::Animation => "Platform animation",
            Self::Routing => "Routing",
            Self::Text => "Text basics",
            Self::Positioned => "Positioned",
            Self::BorderOutline => "Border and outline",
            Self::AssetImage => "Asset image",
        }
    }

    #[inline]
    const fn icon(self) -> &'static str {
        match self {
            Self::Accessibility => "♿",
            Self::ChoiceControls => "☑",
            Self::RangeControls => "↔",
            Self::Forms => "⌨",
            Self::Pickers => "▣",
            Self::Feedback => "!",
            Self::Navigation | Self::RoutingContext => "⌘",
            Self::Animatable => "◈",
            Self::DataView => "▤",
            Self::DndCompletion => "✥",
            Self::StyleTokens => "◐",
            Self::I18n => "文",
            Self::SvgCompletion | Self::CustomShape => "◇",
            Self::AssetsMedia => "◎",
            Self::GlassLiquid => "◌",
            Self::Storage => "▣",
            Self::AnimatedLayout => "↗",
            Self::TextProperties | Self::Text | Self::TextArea | Self::TextField => "T",
            Self::Window => "▣",
            Self::Resizable => "↗",
            Self::FileDrop => "⇩",
            Self::Floating | Self::Modal => "◌",
            Self::FocusNode => "◎",
            Self::Markdown | Self::CustomMarkdown => "▤",
            Self::Counter | Self::StatefulList | Self::AnimatedList => "♙",
            Self::Animation => "◈",
            Self::AsyncBuilder | Self::HttpRequest => "⇄",
            Self::SelectableText => "⌁",
            Self::SystemTheme | Self::AnimatedTheme | Self::CustomAnimatedTheme => "◐",
            Self::DragAndDrop => "✥",
            Self::Loading => "◔",
            Self::Overflow => "↔",
            Self::JustifyContent => "☷",
            Self::Svg | Self::AssetImage => "◇",
            Self::CustomFont => "Aa",
            Self::PanicRecovery => "!",

            Self::ColorSync => "●",
            Self::Routing => "⌘",
            Self::Positioned => "⊞",
            Self::BorderOutline => "□",
        }
    }

    #[inline]
    const fn key(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::ChoiceControls => "choice-controls",
            Self::RangeControls => "range-controls",
            Self::Forms => "forms",
            Self::Pickers => "pickers",
            Self::Feedback => "feedback",
            Self::Navigation => "navigation",
            Self::RoutingContext => "routing-context",
            Self::Animatable => "animatable",
            Self::DataView => "data-view",
            Self::DndCompletion => "dnd-completion",
            Self::StyleTokens => "style-tokens",
            Self::I18n => "i18n",
            Self::SvgCompletion => "svg-completion",
            Self::AssetsMedia => "assets-media",
            Self::GlassLiquid => "glass-liquid",
            Self::Storage => "storage",
            Self::CustomShape => "custom-shape",
            Self::AnimatedLayout => "animated-layout",
            Self::TextProperties => "text-properties",
            Self::Window => "window",
            Self::Resizable => "resizable",
            Self::FileDrop => "file-drop",
            Self::Floating => "floating",
            Self::FocusNode => "focus-node",
            Self::TextArea => "text-area",
            Self::TextField => "text-field",
            Self::Markdown => "markdown",
            Self::CustomMarkdown => "custom-markdown",
            Self::Counter => "counter",
            Self::StatefulList => "stateful-list",
            Self::AsyncBuilder => "async-builder",
            Self::SelectableText => "selectable-text",
            Self::SystemTheme => "system-theme",
            Self::AnimatedTheme => "animated-theme",
            Self::CustomAnimatedTheme => "custom-animated-theme",
            Self::DragAndDrop => "drag-and-drop",
            Self::HttpRequest => "http-request",
            Self::Loading => "loading",
            Self::Overflow => "overflow",
            Self::JustifyContent => "justify-content",
            Self::Svg => "svg",
            Self::CustomFont => "custom-font",
            Self::PanicRecovery => "panic-recovery",
            Self::Modal => "modal",

            Self::ColorSync => "color-sync",
            Self::AnimatedList => "animated-list",
            Self::Animation => "animation",
            Self::Routing => "routing",
            Self::Text => "text",
            Self::Positioned => "positioned",
            Self::BorderOutline => "border-outline",
            Self::AssetImage => "asset-image",
        }
    }

    #[inline]
    const fn description(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "Inspect a platform-neutral semantic tree with actions and focus order."
            }
            Self::ChoiceControls => {
                "Exercise controlled checkbox, switch, radio, select, and autocomplete models."
            }
            Self::RangeControls => {
                "Compare a stepped slider with a distinct two-thumb range slider."
            }
            Self::Forms => {
                "Validate controlled text fields with explicit submit and focus-on-error state."
            }
            Self::Pickers => "Explore bounded calendar, date-time, color, and cancellation models.",
            Self::Feedback => {
                "Drive deterministic tooltip, toast, progress, spinner, and overlay state."
            }
            Self::Navigation => {
                "Navigate tabs and route-backed pages with keyboard-friendly state."
            }
            Self::RoutingContext => {
                "Keep providers available across direct and Shell/Outlet route children."
            }
            Self::Animatable => {
                "Interpolate derived struct, tuple, and explicit enum animation values."
            }
            Self::DataView => {
                "Inspect stable-key collections, table sorting, tree expansion, and loading states."
            }
            Self::DndCompletion => {
                "Exercise bounded auto-scroll, stable-key reorder, cancellation, and file-drop fallback."
            }
            Self::StyleTokens => {
                "Compare semantic theme variants, component states, density, contrast, and motion."
            }
            Self::I18n => {
                "Format translations, plurals, numbers, dates, times, and RTL navigation deterministically."
            }
            Self::SvgCompletion => {
                "Inspect SVG fit policies, gradients, deferred features, diagnostics, and fallback behavior."
            }
            Self::AssetsMedia => {
                "Track asset lifecycle and cache state beside safe optional-media fallbacks."
            }
            Self::GlassLiquid => {
                "Compare Glass and Liquid surfaces with reduced-motion and GPU fallback policies."
            }
            Self::Storage => {
                "Exercise namespaced preferences through the deterministic memory storage adapter."
            }
            Self::CustomShape => {
                "Render bounded curves with fill, stroke, clipping, animation, and hit-test metadata."
            }
            Self::AnimatedLayout => {
                "Transition Flex geometry with bounded duration and reduced-motion policy."
            }
            Self::TextProperties => {
                "Explore typography, wrapping, spacing, indentation, and rich text."
            }
            Self::Window => "A complete dark window page with a persistent library sidebar.",
            Self::Resizable => "Drag edges and corners to change a widget's resolved size.",
            Self::FileDrop => "Drop image files or arbitrary files into separate targets.",
            Self::Floating => "Open anchored menus, tooltips, and overflow-aware panels.",
            Self::FocusNode => "Move keyboard focus between explicit focus targets.",
            Self::TextArea => "Edit multiline text with wrapping and bounded growth.",
            Self::TextField => "Edit a single line and submit it with Return.",
            Self::Markdown => "Render Jaime's bundled Markdown document.",
            Self::CustomMarkdown => "Render custom blocks, inline widgets, and typed syntax.",
            Self::Counter => "Update retained state from button callbacks.",
            Self::StatefulList => "Add and remove items while preserving list state.",
            Self::AsyncBuilder => "Load asynchronous content through an AsyncSnapshot.",
            Self::SelectableText => "Select text across nested widgets and rich spans.",
            Self::SystemTheme => "Follow the system appearance or choose a theme explicitly.",
            Self::AnimatedTheme => "Animate the built-in theme while retaining widget state.",
            Self::CustomAnimatedTheme => "Animate a custom theme with derived values.",
            Self::DragAndDrop => "Move typed cards between drop targets.",
            Self::HttpRequest => "Run an asynchronous request from a button.",
            Self::Loading => "Rotate a bundled SVG while an operation is in progress.",
            Self::Overflow => "Compare hidden, wrapped, and visible overflow.",
            Self::JustifyContent => "Compare the six main-axis distribution modes.",
            Self::Svg => "Animate a bundled SVG document into view.",
            Self::CustomFont => "Register and render an embedded font family.",
            Self::PanicRecovery => {
                "Show the framework's recovered error surface(watch the console)."
            }
            Self::Modal => "Present a modal barrier and animated dialog on demand.",

            Self::ColorSync => "Display a row of synchronized color samples.",
            Self::AnimatedList => "Insert and dismiss list rows with transitions.",
            Self::Animation => "Switch between platform screenshots with a fading transition.",
            Self::Routing => "Navigate through the generated route tree.",
            Self::Text => "A compact baseline text and wrapping example.",
            Self::Positioned => "Place overlapping children at explicit coordinates.",
            Self::BorderOutline => "Compare a border and an outline around a field.",
            Self::AssetImage => "Load and decorate a bundled raster asset.",
        }
    }
}

/// The two-pane interactive example browser used by Jaime.
///
/// The left pane owns only selection and navigation. The selected example is
/// rebuilt in the keyed right viewport, so an example module can focus on its
/// own widget without knowing anything about the catalogue shell.
#[widget(Stateful)]
pub struct ExampleShowcase {}

impl ExampleShowcase {
    /// Creates the example browser with the first registered page selected.
    #[inline]
    pub fn new() -> Self {
        Self {}
    }
}

pub struct ExampleShowcaseState {
    selected: ExampleId,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for ExampleShowcase {
    type State = ExampleShowcaseState;

    fn create_state(self) -> Self::State {
        ExampleShowcaseState {
            selected: EXAMPLES[0],
            updater: StateUpdater::empty(),
        }
    }
}

impl State<ExampleShowcase> for ExampleShowcaseState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let app_theme = ThemeData::copied(ctx);

        Container::new()
            // .width(Dimension::Percent(100.0))
            // .height(Dimension::Percent(100.0))
            .box_decoration(
                BoxDecoration::new()
                    .background_color(app_theme.background_color)
                    .border_radius(24),
            )
            .child(Column::new().children(vec![
                // title_bar(),
                Expanded::new()
                    .child(
                        Row::new().children(vec![
                            sidebar(self.selected, self.updater.clone(), app_theme),
                            SizedBox::new()
                                .width(Dimension::Px(1.0))
                                .color(theme::divider(&app_theme))
                                .boxed(),
                            Expanded::new()
                                .child(content(self.selected, app_theme))
                                .boxed(),
                        ]),
                    )
                    .boxed(),
            ]))
    }
}

fn sidebar(
    selected: ExampleId,
    updater: StateUpdater<ExampleShowcaseState>,
    app_theme: ThemeData,
) -> AnyWidget {
    // let mut childrn = vec![
    //     SizedBox::new()
    //         .height(if cfg!(target_os = "macos") { 38 } else { 18 })
    //         .boxed(),
    // ];
    //
    // childrn.append(
    //     &mut EXAMPLES
    //         .iter()
    //         .copied()
    //         .map(|example| example_button(example, selected == example, updater.clone(), app_theme))
    //         .collect::<Vec<AnyWidget>>(),
    // );

    let list = Column::new()
        .gaps(LayoutSpacing::new().bottom(3))
        .list(
            EXAMPLES
                .into_iter()
                .enumerate(),
        )
        .builder(move |(index, example)| {
            // LayoutSpacing::new().top(38)
            Container::new()
                .margin(case!(
                    cfg!(target_os = "macos") && *index == 0,
                    LayoutSpacing::new().top(38)
                ))
                .box_child(example_button(
                    **example,
                    selected == **example,
                    updater.clone(),
                    app_theme,
                ))
        });
    // .children(childrn);

    Container::new()
        .width(Dimension::Px(SIDEBAR_WIDTH))
        .height(Dimension::Percent(100.0))
        .padding(
            LayoutSpacing::new()
                .left(18)
                .right(14)
                .bottom(16),
        )
        .color(app_theme.surface_color)
        .child(
            Scrollable::new()
                .vertical_scroll_bar(None)
                .axis(ScrollAxis::Vertical)
                .child(list),
        )
        .boxed()
}

fn navigation_button(
    icon: &'static str,
    label: &'static str,
    selected: bool,
    example: ExampleId,
    updater: StateUpdater<ExampleShowcaseState>,
    app_theme: ThemeData,
) -> AnyWidget {
    example_button_with_label(icon, label, selected, example, updater, app_theme)
}

fn example_button(
    example: ExampleId,
    selected: bool,
    updater: StateUpdater<ExampleShowcaseState>,
    app_theme: ThemeData,
) -> AnyWidget {
    example_button_with_label(
        example.icon(),
        example.label(),
        selected,
        example,
        updater,
        app_theme,
    )
}

fn example_button_with_label(
    icon: &'static str,
    label: &'static str,
    selected: bool,
    example: ExampleId,
    updater: StateUpdater<ExampleShowcaseState>,
    app_theme: ThemeData,
) -> AnyWidget {
    let background = if selected {
        app_theme.primary_color
    } else {
        Color::Transparent
    };
    let foreground = if selected {
        app_theme.on_primary_color
    } else {
        app_theme.on_surface_color
    };

    Button::new()
        .on_press(move || {
            updater.set_state(move |state| state.selected = example);
        })
        .decoration(
            BoxDecoration::new()
                .background_color(background)
                .border_radius(10),
        )
        .hover_decoration(
            BoxDecoration::new()
                .background_color(if selected {
                    app_theme.primary_color
                } else {
                    theme::raised_surface(&app_theme)
                })
                .border_radius(10),
        )
        .press_decoration(
            BoxDecoration::new()
                .background_color(
                    app_theme
                        .primary_color
                        .darken(0.08),
                )
                .border_radius(10),
        )
        .box_child(
            Container::new()
                .height(39)
                .padding(
                    LayoutSpacing::new()
                        .left(11)
                        .right(9),
                )
                .child(
                    Row::new()
                        .vertical_alignment(BoxAlignment::Center)
                        .gaps(LayoutSpacing::all(Spacing::Px(10)))
                        .children(vec![
                            Container::new()
                                .width(Dimension::Px(24.0))
                                .child(
                                    Text::new(icon)
                                        .text_align(TextAlign::MidCenter)
                                        .text_style(
                                            TextStyle::new()
                                                .font_size(18)
                                                .color(foreground),
                                        ),
                                )
                                .boxed(),
                            Text::new(label)
                                .text_style(
                                    TextStyle::new()
                                        .font_size(15)
                                        .font_weight(if selected {
                                            FontWeight::Bold
                                        } else {
                                            FontWeight::Normal
                                        })
                                        .color(foreground),
                                )
                                .boxed(),
                        ]),
                ),
        )
}

fn content(selected: ExampleId, app_theme: ThemeData) -> AnyWidget {
    Container::new()
        .height(Dimension::Percent(100.0))
        .padding(
            LayoutSpacing::new()
                .top(20)
                .right(24)
                .bottom(22)
                .left(24),
        )
        .color(app_theme.background_color)
        .child(Column::new().children(vec![
            SizedBox::new().height(18).boxed(),
            Text::new(selected.label())
                .text_style(
                    TextStyle::new()
                        .font_size(30)
                        .font_weight(FontWeight::Bold)
                        .color(app_theme.on_background_color),
                )
                .boxed(),
            SizedBox::new().height(7).boxed(),
            Text::new(selected.description())
                .text_style(TextStyle::new().font_size(15).color(theme::muted_text(&app_theme)))
                .boxed(),
            SizedBox::new().height(18).boxed(),
            Expanded::new()
                .child(
                    Container::new()
                        .width(Dimension::Percent(100.0))
                        .padding(LayoutSpacing::all(Spacing::Px(24)))
                        .box_decoration(
                            BoxDecoration::new()
                                .background_color(theme::raised_surface(&app_theme))
                                .border_radius(14),
                        )
                        .child(build_example(selected, app_theme)),
                )
                .boxed(),
        ]))
        .boxed()
}

fn build_example(example: ExampleId, app_theme: ThemeData) -> AnyWidget {
    match example {
        ExampleId::Accessibility => accessibility_example().boxed(),
        ExampleId::ChoiceControls => selection_controls_example().boxed(),
        ExampleId::RangeControls => RangeControlsExample::new().boxed(),
        ExampleId::Forms => form_example().boxed(),
        ExampleId::Pickers => picker_example().boxed(),
        ExampleId::Feedback => feedback_example().boxed(),
        ExampleId::Navigation => navigation_example().boxed(),
        ExampleId::RoutingContext => routing_context_example().boxed(),
        ExampleId::Animatable => animatable_example().boxed(),
        ExampleId::DataView => data_view_example().boxed(),
        ExampleId::DndCompletion => dnd_completion_example().boxed(),
        ExampleId::StyleTokens => style_tokens_example().boxed(),
        ExampleId::I18n => i18n_example().boxed(),
        ExampleId::SvgCompletion => svg_completion_example().boxed(),
        ExampleId::AssetsMedia => assets_media_example().boxed(),
        ExampleId::GlassLiquid => glass_liquid_example().boxed(),
        ExampleId::Storage => storage_example().boxed(),
        ExampleId::CustomShape => custom_shape_example().boxed(),
        ExampleId::AnimatedLayout => animated_layout_example().boxed(),
        ExampleId::TextProperties => text_properties_example().boxed(),
        ExampleId::Window => WindowShowcase::new().boxed(),
        ExampleId::Resizable => ResizableShowcase::new().boxed(),
        ExampleId::FileDrop => FileDropShowcase::new().boxed(),
        ExampleId::Floating => FloatingShowcase::new().boxed(),
        ExampleId::FocusNode => FocusNodeExample::new().boxed(),
        ExampleId::TextArea => TextAreaExample::new().boxed(),
        ExampleId::TextField => TextFieldExample::new().boxed(),
        ExampleId::Markdown => jaime_markdown_viewer().boxed(),
        ExampleId::CustomMarkdown => custom_markdown_viewer().boxed(),
        ExampleId::Counter => CounterWidget::new(1).boxed(),
        ExampleId::StatefulList => MyList::new().boxed(),
        ExampleId::AsyncBuilder => async_builder_example().boxed(),
        ExampleId::SelectableText => selectable_text_example().boxed(),
        ExampleId::SystemTheme => SystemThemeExample::new().boxed(),
        ExampleId::AnimatedTheme => AnimatedThemeExample::new().boxed(),
        ExampleId::CustomAnimatedTheme => custom_animated_theme_example().boxed(),
        ExampleId::DragAndDrop => DragBoard::new().boxed(),
        ExampleId::HttpRequest => http_request_button_example().boxed(),
        ExampleId::Loading => loading_animation_example().boxed(),
        ExampleId::Overflow => overflow_behavior_example().boxed(),
        ExampleId::JustifyContent => justify_content_example().boxed(),
        ExampleId::Svg => svg_example().boxed(),
        ExampleId::CustomFont => custom_font_example().boxed(),
        ExampleId::PanicRecovery => panic_recovery_example().boxed(),
        ExampleId::Modal => modal_example().boxed(),
        ExampleId::ColorSync => ColorSync.boxed(),
        ExampleId::AnimatedList => MyAnimatedList.boxed(),
        ExampleId::Animation => TestFadingAnimation.boxed(),
        ExampleId::Routing => state_router_widget().boxed(),
        ExampleId::Text => text_example(app_theme),
        ExampleId::Positioned => positioned_example(app_theme),
        ExampleId::BorderOutline => border_outline_example(app_theme),
        ExampleId::AssetImage => asset_image_example(app_theme),
    }
}

fn text_example(app_theme: ThemeData) -> AnyWidget {
    demo_card(
        "Text",
        "A baseline text widget with wrapping and multiple writing systems.",
        Text::new("Hello, Aimer!\nEnglish · Français · 中文 · សួស្តី")
            .text_style(
                TextStyle::new()
                    .font_size(24)
                    .color(app_theme.on_surface_color),
            )
            .boxed(),
        app_theme,
    )
}

fn positioned_example(app_theme: ThemeData) -> AnyWidget {
    demo_card(
        "Positioned",
        "The cards below overlap because their offsets are resolved inside a Stack.",
        Container::new()
            .width(Dimension::Px(520.0))
            .height(Dimension::Px(300.0))
            .box_decoration(
                BoxDecoration::new()
                    .background_color(theme::recessed_surface(&app_theme))
                    .border_radius(12),
            )
            .child(
                Stack::new().children([
                    Positioned::new()
                        .top(34.0)
                        .left(38.0)
                        .child(
                            Container::new()
                                .width(Dimension::Px(260.0))
                                .height(Dimension::Px(150.0))
                                .color(
                                    app_theme
                                        .primary_color
                                        .darken(0.18),
                                )
                                .child(Text::new("Top / left")),
                        )
                        .boxed(),
                    Positioned::new()
                        .top(112.0)
                        .left(172.0)
                        .child(
                            Container::new()
                                .width(Dimension::Px(260.0))
                                .height(Dimension::Px(150.0))
                                .color(app_theme.primary_color)
                                .child(Text::new("Overlapping child")),
                        )
                        .boxed(),
                ]),
            )
            .boxed(),
        app_theme,
    )
}

fn border_outline_example(app_theme: ThemeData) -> AnyWidget {
    demo_card(
        "Border and outline",
        "A border participates in the decorated box; an outline is painted outside it.",
        Container::new()
            .width(400.0)
            .height(400.0)
            .margin(LayoutSpacing::all(Spacing::Px(30)))
            .box_decoration(
                BoxDecoration::new()
                    .background_color(theme::recessed_surface(&app_theme))
                    .border_radius(10)
                    .border(BoxBorder::all(
                        BorderSlice::new()
                            .style(BorderStyle::Solid)
                            .stroke(Stroke::Px(8.0))
                            .color(app_theme.primary_color),
                    ))
                    .outline(BoxOutline::all(
                        BorderSlice::new()
                            .style(BorderStyle::Solid)
                            .stroke(Stroke::Px(8.0))
                            .color(
                                app_theme
                                    .primary_color
                                    .lighten(0.16),
                            ),
                    )),
            )
            .child(
                Text::new("Border inside · outline outside").text_style(
                    TextStyle::new()
                        .font_size(18)
                        .color(app_theme.on_surface_color),
                ),
            )
            .boxed(),
        app_theme,
    )
}

fn asset_image_example(app_theme: ThemeData) -> AnyWidget {
    demo_card(
        "Asset image",
        "A bundled image can be scaled and decorated without a platform-specific API.",
        AssetImage::new("assets/my_image.png")
            .fit(BoxFit::FitWidth)
            .boxed(),
        app_theme,
    )
}

fn demo_card(
    title: &'static str,
    description: &'static str,
    demo: AnyWidget,
    app_theme: ThemeData,
) -> AnyWidget {
    Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(28)))
        .child(Column::new().children(vec![
            Text::new(title)
                .text_style(
                    TextStyle::new()
                        .font_size(26)
                        .font_weight(FontWeight::Bold)
                        .color(app_theme.on_surface_color),
                )
                .boxed(),
            SizedBox::new().height(8).boxed(),
            Text::new(description)
                .text_style(TextStyle::new().font_size(15).color(theme::muted_text(&app_theme)))
                .boxed(),
            SizedBox::new().height(24).boxed(),
            demo,
        ]))
        .boxed()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn showcase_has_unique_registered_examples() {
        assert!(!EXAMPLES.is_empty());
        let keys = EXAMPLES
            .iter()
            .map(|example| example.key())
            .collect::<HashSet<_>>();

        assert_eq!(keys.len(), EXAMPLES.len());
    }

    #[test]
    fn every_registered_example_has_metadata_and_a_constructible_page() {
        for example in EXAMPLES {
            assert!(
                !example
                    .label()
                    .is_empty()
            );
            assert!(
                !example
                    .description()
                    .is_empty()
            );
            let _page = build_example(*example, theme::app_theme());
        }
    }

    #[test]
    fn the_default_selection_is_the_first_registered_example() {
        let state = ExampleShowcase::new().create_state();

        assert_eq!(state.selected, EXAMPLES[0]);
    }

    #[test]
    fn showcase_mounts_with_the_first_example_visible() {
        let mut app = AimerApp::start_headless(ExampleShowcase::new());

        app.pump_frames(2);
    }
}
