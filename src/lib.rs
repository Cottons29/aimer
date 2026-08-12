pub use aimer_assets::img_widget::image_widget::Image;
pub use aimer_assets::img_widget::source::ImageSource;
pub use aimer_assets::{
    FontError, FontFamily, FontRegistration, FontRegistry, FontStyle, FontWeight, *,
};
pub use aimer_attribute::dimension::Dimension;
pub use aimer_attribute::position::Vec2d;
pub use aimer_attribute::size::{ResolvedSize, Size};
pub use aimer_color::prelude::*;
pub use aimer_container::*;
pub use aimer_ctxmenu::{
    ContextMenu, ContextMenuDismiss, ContextMenuItem, ContextMenuRows, ContextMenuShape,
    ContextMenuStyle,
};
pub use aimer_dnd::{
    DragAxis, DragOverlay, DragPayload, DragSession, DragStartMode, DragTarget, DragTargetState,
    Draggable, DropZone, FileDrop,
};
pub use aimer_events as events;
pub use aimer_events::element::ElementEvent;
pub use aimer_flex::*;
pub use aimer_focus as focus;
pub use aimer_focus::{
    FocusBehavior, FocusCallback, FocusCandidate, FocusManager, FocusNode, FocusTrap, FocusTrapId,
    FocusTransition, active_focus_trap,
};
pub use aimer_grid::*;
pub use aimer_input::button::Button;
pub use aimer_input::callback::{AsyncCallback, RawInnerCallback};
pub use aimer_input::input::*;
pub use aimer_input::*;
pub use aimer_macro::{Router, StatefulWidget, StatelessWidget, Theme, key, main};
#[cfg(feature = "markdown")]
pub use aimer_markdown::{
    Alignment as MarkdownAlignment, Block as MarkdownBlock, Document as MarkdownDocument,
    ImageResolver as MarkdownImageResolver, Inline as MarkdownInline,
    LinkHandler as MarkdownLinkHandler, MarkdownError, MarkdownImage, MarkdownTheme,
    MarkdownViewer,
};
pub use aimer_modal::{
    Anchor, AnchorHandle, Floating, FloatingAlign, FloatingPlacement, FloatingSide, Modal,
    ModalAnimation, ModalController, ModalHandle, ModalHost, ModalId, OverflowPolicy,
    OverlayLayer, OverlayLayerHandle, PlacementSpec, resolve_placement,
};
#[cfg(feature = "provider")]
pub use aimer_provider::{
    NotifierProvider, Provider, ProviderContext, ProviderHandle, StoreProvider,
};
pub use aimer_quiver as quiver;
pub use aimer_quiver::frame_stats;
pub use aimer_quiver::{
    self, AimerApp, FIRST_FRAME_RENDERED_EVENT, HeadlessAimerApp, HeadlessOptions,
    set_first_frame_rendered_callback,
};
pub use aimer_rubick::{self, ErasedFrom, Rubick};
pub use aimer_scroll::*;
pub use aimer_space::*;
#[cfg(feature = "svg")]
pub use aimer_svg::{
    RawSvg, Svg, SvgCallback, SvgColor, SvgDiagnostic, SvgDocument, SvgError, SvgFillRule, SvgHit,
    SvgLimits, SvgLoadState, SvgLoader, SvgNodeId, SvgNodeMetadata, SvgPath, SvgSelector,
    SvgSource, SvgStyle, SvgTransform,
};
pub use aimer_text::{RichText, SelectionArea, SpanStyle, Text, TextButton, TextSpan};
pub use aimer_venus as venus;
pub use aimer_venus::{TaskScope, Venus, yield_if_over_budget, yield_now};
pub use aimer_widget::base::BuildContext;
pub use aimer_widget::{self, Key, State, StatefulWidget, StatelessWidget, Widget, *};


pub use aimer_native as native;

pub mod widget {
    pub use aimer_widget::base::BuildContext;
    pub use aimer_widget::{State, StatefulWidget, StatelessWidget, Widget, *};
}

pub mod animation {
    pub use aimer_animation::*;
}

// Macro re-export
pub mod macros {
    pub use aimer_macro::{Router, StatefulWidget, StatelessWidget, Theme, key, main, widget};
}

// Styling re-export
pub mod style {
    pub use aimer_animation::primitives::curve::Curve;
    pub use aimer_animation::{AnimationEffect, AnimationStatus};
    pub use aimer_color::prelude::{Color, Colors};
    pub use aimer_flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
    pub use aimer_style::*;
}

// utils re-export
pub mod console {
    pub use aimer_utils::*;
}

// wasm dependencies
pub use aimer_provider as provider;
pub use aimer_router as router;
pub use wasm_bindgen;
