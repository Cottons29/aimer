/// Every page currently available in the Jaime showcase.
///
/// The enum is the small interface between the shared navigation shell and an
/// example module. Adding a page means adding one descriptor here and one
/// branch to [`super::build_example`]; the example's implementation remains in its
/// own module.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExampleId {
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
    BackdropFilter,
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
    CustomTextFieldCaret,
    Markdown,
    CustomMarkdown,
    Counter,
    StatefulList,
    CollapsibleList,
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

pub(super) const EXAMPLES: &[ExampleId] = &[
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
    ExampleId::BackdropFilter,
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
    ExampleId::CustomTextFieldCaret,
    ExampleId::Markdown,
    ExampleId::CustomMarkdown,
    ExampleId::Counter,
    ExampleId::StatefulList,
    ExampleId::CollapsibleList,
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

pub(super) struct ExampleCategory {
    pub(super) label: &'static str,
    pub(super) examples: &'static [ExampleId],
}

pub(super) const EXAMPLE_CATEGORIES: &[ExampleCategory] = &[
    ExampleCategory {
        label: "Controls",
        examples: &[
            ExampleId::Accessibility,
            ExampleId::ChoiceControls,
            ExampleId::RangeControls,
            ExampleId::Forms,
            ExampleId::Pickers,
            ExampleId::Feedback,
            ExampleId::TextArea,
            ExampleId::TextField,
            ExampleId::CustomTextFieldCaret,
            ExampleId::SelectableText,
        ],
    },
    ExampleCategory {
        label: "Navigation",
        examples: &[
            ExampleId::Navigation,
            ExampleId::RoutingContext,
            ExampleId::Floating,
            ExampleId::Modal,
            ExampleId::Routing,
        ],
    },
    ExampleCategory {
        label: "State & data",
        examples: &[
            ExampleId::Animatable,
            ExampleId::DataView,
            ExampleId::Storage,
            ExampleId::Counter,
            ExampleId::StatefulList,
            ExampleId::CollapsibleList,
            ExampleId::AsyncBuilder,
        ],
    },
    ExampleCategory {
        label: "Content",
        examples: &[
            ExampleId::I18n,
            ExampleId::TextProperties,
            ExampleId::Markdown,
            ExampleId::CustomMarkdown,
            ExampleId::Text,
        ],
    },
    ExampleCategory {
        label: "Layout",
        examples: &[
            ExampleId::AnimatedLayout,
            ExampleId::Window,
            ExampleId::Resizable,
            ExampleId::FocusNode,
            ExampleId::Overflow,
            ExampleId::JustifyContent,
            ExampleId::Positioned,
            ExampleId::BorderOutline,
        ],
    },
    ExampleCategory {
        label: "Media & styling",
        examples: &[
            ExampleId::StyleTokens,
            ExampleId::SvgCompletion,
            ExampleId::AssetsMedia,
            ExampleId::GlassLiquid,
            ExampleId::BackdropFilter,
            ExampleId::CustomShape,
            ExampleId::SystemTheme,
            ExampleId::AnimatedTheme,
            ExampleId::CustomAnimatedTheme,
            ExampleId::Svg,
            ExampleId::CustomFont,
            ExampleId::ColorSync,
            ExampleId::AssetImage,
        ],
    },
    ExampleCategory {
        label: "Interaction & runtime",
        examples: &[
            ExampleId::DndCompletion,
            ExampleId::FileDrop,
            ExampleId::DragAndDrop,
            ExampleId::HttpRequest,
            ExampleId::Loading,
            ExampleId::AnimatedList,
            ExampleId::Animation,
            ExampleId::PanicRecovery,
        ],
    },
];

impl ExampleId {
    #[inline]
    pub(super) const fn label(self) -> &'static str {
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
            Self::BackdropFilter => "Backdrop filter",
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
            Self::CustomTextFieldCaret => "Custom TextField caret",
            Self::Markdown => "Markdown",
            Self::CustomMarkdown => "Custom Markdown",
            Self::Counter => "Stateful counter",
            Self::StatefulList => "Stateful list",
            Self::CollapsibleList => "Collapsible list",
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
    pub(super) const fn icon(self) -> &'static str {
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
            Self::BackdropFilter => "▧",
            Self::Storage => "▣",
            Self::AnimatedLayout => "↗",
            Self::TextProperties
            | Self::Text
            | Self::TextArea
            | Self::TextField
            | Self::CustomTextFieldCaret => "T",
            Self::Window => "▣",
            Self::Resizable => "↗",
            Self::FileDrop => "⇩",
            Self::Floating | Self::Modal => "◌",
            Self::FocusNode => "◎",
            Self::Markdown | Self::CustomMarkdown => "▤",
            Self::Counter | Self::StatefulList | Self::AnimatedList => "♙",
            Self::CollapsibleList => "⌄",
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
    pub(super) const fn key(self) -> &'static str {
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
            Self::BackdropFilter => "backdrop-filter",
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
            Self::CustomTextFieldCaret => "custom-text-field-caret",
            Self::Markdown => "markdown",
            Self::CustomMarkdown => "custom-markdown",
            Self::Counter => "counter",
            Self::StatefulList => "stateful-list",
            Self::CollapsibleList => "collapsible-list",
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
    pub(super) const fn description(self) -> &'static str {
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
            Self::BackdropFilter => {
                "Blur and adjust background text through a box while keeping its foreground text sharp."
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
            Self::CustomTextFieldCaret => {
                "Replace the builtin insertion cursor with a themed retained widget."
            }
            Self::Markdown => "Render Jaime's bundled Markdown document.",
            Self::CustomMarkdown => "Render custom blocks, inline widgets, and typed syntax.",
            Self::Counter => "Update retained state from button callbacks.",
            Self::StatefulList => "Add and remove items while preserving list state.",
            Self::CollapsibleList => "Keep a header visible while toggling a retained body.",
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

