use std::cell::{Cell, RefCell, UnsafeCell};
use std::sync::Arc;

use aimer_attribute::{Bounds, CacheBounds, Dimension, ResolvedSize};
use aimer_cupid::svg::{
    SvgFillRule, SvgGeometry, SvgNode, SvgNodeId, SvgNodeStyleOverride, SvgPathCommand, SvgScene,
    SvgViewport,
};
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_events::window::request_animation_frame;
use aimer_utils::callback::{Callback, CallbackExecutor, RawInnerCallback};
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
};

use crate::{SvgDocument, SvgError, SvgLoadState, SvgLoader, SvgSelector, SvgSource, SvgStyle};

pub type SvgCallback = Callback<SvgHit, ()>;

#[derive(Clone, Debug)]
pub struct SvgNodeMetadata {
    pub svg_id: Option<Arc<str>>,
    pub classes: Arc<[Arc<str>]>,
    pub element: aimer_cupid::svg::SvgElementKind,
}

#[derive(Clone, Debug)]
pub struct SvgHit {
    pub node_id: SvgNodeId,
    pub metadata: SvgNodeMetadata,
}

#[derive(Clone)]
struct StyleRule {
    selector: SvgSelector,
    style: SvgStyle,
}

#[derive(Clone)]
struct CallbackRule {
    selector: SvgSelector,
    callback: SvgCallback,
}

#[derive(Clone)]
/// Renders an already parsed [`SvgDocument`].
///
/// With no explicit dimensions, the widget uses the document's intrinsic SVG
/// viewport. Setting only [`Svg::width`] or [`Svg::height`] preserves the viewport
/// aspect ratio; setting both uses the requested dimensions. Selector-based style
/// overrides and pointer interaction are optional and empty by default.
///
/// Styles target `#id`, `.class`, or element-name selectors. A [`SvgStyle`] changes
/// only the properties it contains: fill and stroke colors can be replaced or
/// removed, opacity is overridden, and a transform replaces the selected node's
/// rendered transform. The non-`try_` builders silently ignore invalid selectors;
/// their `try_` counterparts return [`SvgError::InvalidSelector`].
///
/// # Example
///
/// ```
/// use aimer_svg::{Svg, SvgColor, SvgDocument, SvgError, SvgStyle};
///
/// # fn example() -> Result<(), SvgError> {
/// let document = SvgDocument::from_svg(
///     br#"<svg viewBox="0 0 24 24"><path id="mark" d="M2 2h20v20z"/></svg>"#,
/// )?;
/// let svg = Svg::new(document)
///     .width(48.0)
///     .style("#mark", SvgStyle::new().fill(SvgColor::rgba8(0, 128, 255, 255)));
/// # Ok(())
/// # }
/// ```
pub struct Svg {
    document: SvgDocument,
    width: Option<Dimension>,
    height: Option<Dimension>,
    styles: Vec<StyleRule>,
    hover_styles: Vec<StyleRule>,
    pressed_styles: Vec<StyleRule>,
    callbacks: Vec<CallbackRule>,
}

impl Svg {
    /// Creates a widget for a parsed SVG document.
    ///
    /// The intrinsic viewport determines its size, and no style or callback rules
    /// are installed.
    pub fn new(document: SvgDocument) -> Self {
        Self {
            document,
            width: None,
            height: None,
            styles: Vec::new(),
            hover_styles: Vec::new(),
            pressed_styles: Vec::new(),
            callbacks: Vec::new(),
        }
    }

    /// Sets the rendered width.
    ///
    /// When height is not set, it is derived from the document viewport to preserve
    /// the intrinsic aspect ratio.
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Sets the rendered height.
    ///
    /// When width is not set, it is derived from the document viewport to preserve
    /// the intrinsic aspect ratio.
    pub fn height(mut self, height: impl Into<Dimension>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Adds a persistent style override for nodes matching `selector`.
    ///
    /// Supported selectors are `#id`, `.class`, and element names. An invalid
    /// selector is ignored; use [`Svg::try_style`] to receive an error instead.
    /// Rules are retained in insertion order and only properties set in `style`
    /// override the SVG document.
    pub fn style(mut self, selector: impl AsRef<str>, style: impl Into<SvgStyle>) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.styles.push(StyleRule {
                selector,
                style: style.into(),
            });
        }
        self
    }

    /// Adds a persistent style override, returning an error for an invalid selector.
    ///
    /// See [`Svg::style`] for matching and override semantics.
    pub fn try_style(
        mut self,
        selector: impl AsRef<str>,
        style: SvgStyle,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.styles
            .push(StyleRule { selector, style });
        Ok(self)
    }

    /// Adds a style applied while a matching painted node is under the pointer.
    ///
    /// An invalid selector is ignored; use [`Svg::try_hover_style`] to receive an
    /// error. Only properties present in `style` are overridden.
    pub fn hover_style(mut self, selector: impl AsRef<str>, style: SvgStyle) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.hover_styles
                .push(StyleRule { selector, style });
        }
        self
    }

    /// Adds a hover style, returning an error for an invalid selector.
    ///
    /// See [`Svg::hover_style`] for interaction and override semantics.
    pub fn try_hover_style(
        mut self,
        selector: impl AsRef<str>,
        style: SvgStyle,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.hover_styles
            .push(StyleRule { selector, style });
        Ok(self)
    }

    /// Adds a style applied while a matching painted node is pressed.
    ///
    /// An invalid selector is ignored; use [`Svg::try_pressed_style`] to receive an
    /// error. The pressed state ends when the pointer is released or cancelled.
    pub fn pressed_style(mut self, selector: impl AsRef<str>, style: SvgStyle) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.pressed_styles
                .push(StyleRule { selector, style });
        }
        self
    }

    /// Adds a pressed style, returning an error for an invalid selector.
    ///
    /// See [`Svg::pressed_style`] for interaction and override semantics.
    pub fn try_pressed_style(
        mut self,
        selector: impl AsRef<str>,
        style: SvgStyle,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.pressed_styles
            .push(StyleRule { selector, style });
        Ok(self)
    }

    /// Registers a callback for a press completed on a matching painted node.
    ///
    /// The callback receives [`SvgHit`] metadata and runs only when pointer down and
    /// pointer up hit the same node. An invalid selector is ignored; use
    /// [`Svg::try_on_path_press`] to receive an error.
    pub fn on_path_press(
        mut self,
        selector: impl AsRef<str>,
        callback: impl Into<SvgCallback>,
    ) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.callbacks
                .push(CallbackRule {
                    selector,
                    callback: callback.into(),
                });
        }
        self
    }

    /// Registers a press callback, returning an error for an invalid selector.
    ///
    /// See [`Svg::on_path_press`] for hit-testing and press lifecycle semantics.
    pub fn try_on_path_press(
        mut self,
        selector: impl AsRef<str>,
        callback: impl Into<SvgCallback>,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.callbacks
            .push(CallbackRule {
                selector,
                callback: callback.into(),
            });
        Ok(self)
    }
}

impl Widget for Svg {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        RawSvg {
            document: self.document.clone(),
            width: self.width,
            height: self.height,
            styles: self.styles.clone(),
            hover_styles: self.hover_styles.clone(),
            pressed_styles: self.pressed_styles.clone(),
            callbacks: self.callbacks.clone(),
            bounds: CacheBounds::new(),
            hovered: Cell::new(None),
            interaction: RefCell::new(SvgInteraction::default()),
        }
        .boxed()
    }
}

/// Displays an SVG bundled with the app and registered under `[assets]` in
/// `aimer.toml`.
///
/// The asset key follows the same platform lookup rules as `AssetImage`:
/// Android's asset manager, app-bundle resources on Apple platforms, the project
/// directory during desktop development, and a root-relative request on web.
/// Loading and parsing run asynchronously. While loading, the configured
/// [`SvgAsset::loading_widget`] is active; after an I/O or parse error, the
/// configured [`SvgAsset::error_widget`] is active. Without the corresponding
/// fallback, the widget has no active child for that phase.
///
/// Once loaded, sizing, selector styles, transforms, colors, and callbacks have the
/// same semantics as [`Svg`].
///
/// # Example
///
/// ```
/// use aimer_svg::{SvgAsset, SvgColor, SvgStyle};
///
/// let icon = SvgAsset::new("assets/icon.svg")
///     .width(32.0)
///     .height(32.0)
///     .style(".accent", SvgStyle::new().fill(SvgColor::rgba8(0, 128, 255, 255)));
/// ```
pub struct SvgAsset {
    key: Arc<str>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    styles: Vec<StyleRule>,
    hover_styles: Vec<StyleRule>,
    pressed_styles: Vec<StyleRule>,
    callbacks: Vec<CallbackRule>,
    loading_widget: Option<AnyWidget>,
    error_widget: Option<AnyWidget>,
}

impl SvgAsset {
    /// Creates a widget for the registered SVG asset `key`.
    ///
    /// The intrinsic SVG viewport determines the loaded widget's size. No style,
    /// callback, loading, or error widgets are set by default.
    pub fn new(key: impl Into<Arc<str>>) -> Self {
        Self {
            key: key.into(),
            width: None,
            height: None,
            styles: Vec::new(),
            hover_styles: Vec::new(),
            pressed_styles: Vec::new(),
            callbacks: Vec::new(),
            loading_widget: None,
            error_widget: None,
        }
    }

    /// Sets the rendered width after the asset loads.
    ///
    /// When height is not set, it is derived from the SVG viewport to preserve the
    /// intrinsic aspect ratio.
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Sets the rendered height after the asset loads.
    ///
    /// When width is not set, it is derived from the SVG viewport to preserve the
    /// intrinsic aspect ratio.
    pub fn height(mut self, height: impl Into<Dimension>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Adds a persistent style override for nodes matching `selector`.
    ///
    /// Supported selectors are `#id`, `.class`, and element names. An invalid
    /// selector is ignored; use [`SvgAsset::try_style`] to receive an error instead.
    /// Only properties set in [`SvgStyle`] override the loaded document.
    pub fn style(mut self, selector: impl AsRef<str>, style: SvgStyle) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.styles
                .push(StyleRule { selector, style });
        }
        self
    }

    /// Adds a persistent style override, returning an error for an invalid selector.
    ///
    /// See [`SvgAsset::style`] for matching and override semantics.
    pub fn try_style(
        mut self,
        selector: impl AsRef<str>,
        style: SvgStyle,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.styles
            .push(StyleRule { selector, style });
        Ok(self)
    }

    /// Adds a style applied while a matching painted node is under the pointer.
    ///
    /// An invalid selector is ignored; use [`SvgAsset::try_hover_style`] to receive
    /// an error.
    pub fn hover_style(mut self, selector: impl AsRef<str>, style: SvgStyle) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.hover_styles
                .push(StyleRule { selector, style });
        }
        self
    }

    /// Adds a hover style, returning an error for an invalid selector.
    ///
    /// See [`SvgAsset::hover_style`] for interaction semantics.
    pub fn try_hover_style(
        mut self,
        selector: impl AsRef<str>,
        style: SvgStyle,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.hover_styles
            .push(StyleRule { selector, style });
        Ok(self)
    }

    /// Adds a style applied while a matching painted node is pressed.
    ///
    /// An invalid selector is ignored; use [`SvgAsset::try_pressed_style`] to
    /// receive an error. The pressed state ends on pointer release or cancellation.
    pub fn pressed_style(mut self, selector: impl AsRef<str>, style: SvgStyle) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.pressed_styles
                .push(StyleRule { selector, style });
        }
        self
    }

    /// Adds a pressed style, returning an error for an invalid selector.
    ///
    /// See [`SvgAsset::pressed_style`] for interaction semantics.
    pub fn try_pressed_style(
        mut self,
        selector: impl AsRef<str>,
        style: SvgStyle,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.pressed_styles
            .push(StyleRule { selector, style });
        Ok(self)
    }

    /// Registers a callback for a press completed on a matching painted node.
    ///
    /// The callback receives [`SvgHit`] metadata and runs only when pointer down and
    /// pointer up hit the same node. An invalid selector is ignored; use
    /// [`SvgAsset::try_on_path_press`] to receive an error.
    pub fn on_path_press(
        mut self,
        selector: impl AsRef<str>,
        callback: impl Into<SvgCallback>,
    ) -> Self {
        if let Ok(selector) = selector.as_ref().parse() {
            self.callbacks
                .push(CallbackRule {
                    selector,
                    callback: callback.into(),
                });
        }
        self
    }

    /// Registers a press callback, returning an error for an invalid selector.
    ///
    /// See [`SvgAsset::on_path_press`] for hit-testing and press lifecycle semantics.
    pub fn try_on_path_press(
        mut self,
        selector: impl AsRef<str>,
        callback: impl Into<SvgCallback>,
    ) -> Result<Self, SvgError> {
        let selector = selector.as_ref().parse()?;
        self.callbacks
            .push(CallbackRule {
                selector,
                callback: callback.into(),
            });
        Ok(self)
    }

    /// Sets the child active while the asset is being read and parsed.
    ///
    /// Without one, the asset widget has no active child during loading. The child
    /// is built with the same build context as the asset widget.
    pub fn loading_widget(mut self, loading_widget: impl Widget + 'static) -> Self {
        self.loading_widget = Some(loading_widget.boxed());
        self
    }

    /// Sets the child active after asset loading or SVG parsing fails.
    ///
    /// The underlying error text is not passed to the child. Without an error
    /// widget, the asset widget has no active child after failure.
    pub fn error_widget(mut self, error_widget: impl Widget + 'static) -> Self {
        self.error_widget = Some(error_widget.boxed());
        self
    }
}

impl Widget for SvgAsset {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let loader = SvgLoader::new(SvgSource::Asset(self.key.clone()));
        let background_loader = loader.clone();
        let window = ctx.window.clone();

        #[cfg(not(target_arch = "wasm32"))]
        ctx.async_handle
            .spawn(async move {
                background_loader.load().await;
                window.request_redraw();
            });

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            background_loader.load().await;
            window.request_redraw();
        });

        Box::new(RawSvgAsset {
            loader,
            phase: Cell::new(SvgAssetPhase::Loading),
            width: self.width,
            height: self.height,
            styles: self.styles.clone(),
            hover_styles: self.hover_styles.clone(),
            pressed_styles: self.pressed_styles.clone(),
            callbacks: self.callbacks.clone(),
            loading_element: self
                .loading_widget
                .as_ref()
                .map(|widget| widget.to_element(ctx)),
            error_element: self
                .error_widget
                .as_ref()
                .map(|widget| widget.to_element(ctx)),
            svg_element: UnsafeCell::new(None),
        })
    }

    fn debug_name(&self) -> &'static str {
        "SvgAsset"
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SvgAssetPhase {
    Loading,
    Ready,
    Error,
}

struct RawSvgAsset {
    loader: SvgLoader,
    phase: Cell<SvgAssetPhase>,
    width: Option<Dimension>,
    height: Option<Dimension>,
    styles: Vec<StyleRule>,
    hover_styles: Vec<StyleRule>,
    pressed_styles: Vec<StyleRule>,
    callbacks: Vec<CallbackRule>,
    loading_element: Option<Box<dyn Element>>,
    error_element: Option<Box<dyn Element>>,
    svg_element: UnsafeCell<Option<Box<dyn Element>>>,
}

impl RawSvgAsset {
    fn refresh(&self, ctx: &BuildContext) {
        if self.phase.get() != SvgAssetPhase::Loading {
            return;
        }
        match self.loader.state() {
            SvgLoadState::Loading => {}
            SvgLoadState::Ready(document) => {
                let svg = Svg {
                    document,
                    width: self.width,
                    height: self.height,
                    styles: self.styles.clone(),
                    hover_styles: self.hover_styles.clone(),
                    pressed_styles: self.pressed_styles.clone(),
                    callbacks: self.callbacks.clone(),
                };
                // Rendering and element-tree access are single-threaded. The loader
                // only updates its independent synchronized state in the background.
                unsafe {
                    *self.svg_element.get() = Some(svg.to_element(ctx));
                }
                self.phase
                    .set(SvgAssetPhase::Ready);
            }
            SvgLoadState::Error(_) => self
                .phase
                .set(SvgAssetPhase::Error),
        }
    }

    fn active_element(&self) -> Option<&dyn Element> {
        match self.phase.get() {
            SvgAssetPhase::Loading => self
                .loading_element
                .as_deref(),
            SvgAssetPhase::Ready => {
                // The element is initialized before the phase changes to Ready and
                // is thereafter only read on the render thread.
                unsafe { (&*self.svg_element.get()).as_deref() }
            }
            SvgAssetPhase::Error => self.error_element.as_deref(),
        }
    }
}

impl VisitorElement for RawSvgAsset {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        if let Some(element) = self.active_element() {
            visitor(element);
        }
    }

    fn debug_name(&self) -> &'static str {
        "SvgAssetElement"
    }
}

impl LayoutElement for RawSvgAsset {
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.refresh(ctx);
        self.active_element()
            .map(|element| element.layout(ctx))
            .unwrap_or_default()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.refresh(ctx);
        self.active_element()
            .map(|element| element.computed_size(ctx))
            .unwrap_or_default()
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.refresh(ctx);
        self.active_element()
            .map(|element| element.content_size(ctx))
            .unwrap_or_default()
    }

    fn invalidate_layout(&self) {
        if let Some(element) = self.active_element() {
            element.invalidate_layout();
        }
    }
}

impl Drawable for RawSvgAsset {
    fn draw(&self, ctx: &BuildContext) {
        self.refresh(ctx);
        if let Some(element) = self.active_element() {
            element.draw(ctx);
        }
    }
}

impl EventElement for RawSvgAsset {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.active_element()
            .is_some_and(|element| element.on_event(event))
    }

    fn captures_pointer(&self, pointer: u64) -> bool {
        self.active_element()
            .is_some_and(|element| element.captures_pointer(pointer))
    }
}

impl Rebuildable for RawSvgAsset {}

pub struct RawSvg {
    document: SvgDocument,
    width: Option<Dimension>,
    height: Option<Dimension>,
    styles: Vec<StyleRule>,
    hover_styles: Vec<StyleRule>,
    pressed_styles: Vec<StyleRule>,
    callbacks: Vec<CallbackRule>,
    bounds: CacheBounds,
    hovered: Cell<Option<SvgNodeId>>,
    interaction: RefCell<SvgInteraction>,
}

impl RawSvg {
    fn resolved_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let viewport = self.document.scene().viewport;
        let width = self
            .width
            .map(|value| value.resolve(ctx.parent_size.width, ctx.scale));
        let height = self
            .height
            .map(|value| value.resolve(ctx.parent_size.height, ctx.scale));
        let (width, height) = resolved_svg_size(viewport, width, height);
        ResolvedSize {
            width: width.clamp(ctx.box_constraint.min_width, ctx.box_constraint.max_width),
            height: height.clamp(ctx.box_constraint.min_height, ctx.box_constraint.max_height),
        }
    }

    fn active_rules(&self) -> Vec<(SvgSelector, SvgStyle)> {
        let mut rules = self
            .styles
            .iter()
            .map(|rule| (rule.selector.clone(), rule.style))
            .collect::<Vec<_>>();
        if let Some(hovered) = self.hovered.get()
            && let Some(node) = self
                .document
                .scene()
                .node(hovered)
        {
            rules.extend(
                self.hover_styles
                    .iter()
                    .filter(|rule| rule.selector.matches(node))
                    .map(|rule| (rule.selector.clone(), rule.style)),
            );
        }
        if let Some(pressed) = self
            .interaction
            .borrow()
            .pressed
            && let Some(node) = self
                .document
                .scene()
                .node(pressed)
        {
            rules.extend(
                self.pressed_styles
                    .iter()
                    .filter(|rule| rule.selector.matches(node))
                    .map(|rule| (rule.selector.clone(), rule.style)),
            );
        }
        rules
    }

    fn overrides(&self) -> Vec<SvgNodeStyleOverride> {
        overrides_for_rules(self.document.scene(), &self.active_rules())
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<SvgHit> {
        hit_test_scene(
            self.document.scene(),
            self.bounds.get_bounds()?,
            x,
            y,
            &self.overrides(),
        )
    }

    fn set_hovered(&self, hovered: Option<SvgNodeId>) {
        if self.hovered.replace(hovered) != hovered {
            request_animation_frame();
        }
    }

    fn execute_callbacks(&self, hit: SvgHit) {
        let Some(node) = self
            .document
            .scene()
            .node(hit.node_id)
        else {
            return;
        };
        for rule in self
            .callbacks
            .iter()
            .filter(|rule| rule.selector.matches(node))
        {
            if let Some(callback) = rule.callback.get().as_ref() {
                match callback {
                    RawInnerCallback::Empty => {}
                    RawInnerCallback::Sync(function) => function(hit.clone()),
                    RawInnerCallback::Async(function) => {
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            handle.spawn(function(hit.clone()));
                        }
                        #[cfg(target_arch = "wasm32")]
                        wasm_bindgen_futures::spawn_local(function(hit.clone()));
                    }
                }
            }
        }
    }
}

impl VisitorElement for RawSvg {
    fn debug_name(&self) -> &'static str {
        "Svg"
    }
}

impl Rebuildable for RawSvg {}

impl LayoutElement for RawSvg {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.resolved_size(ctx)
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.resolved_size(ctx);
        let (x, y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(ctx.scale, x, y, size.width, size.height);
        size
    }

    fn pos_start_end(&self) -> Option<(aimer_attribute::Vec2d, aimer_attribute::Vec2d)> {
        self.bounds.pos_start_end()
    }
}

impl Drawable for RawSvg {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.resolved_size(ctx);
        let (x, y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(ctx.scale, x, y, size.width, size.height);
        let overrides = self.overrides();
        ctx.canvas.draw_svg(
            self.document.scene().clone(),
            (0.0, 0.0).into(),
            size,
            overrides.into(),
        );
    }
}

impl EventElement for RawSvg {
    fn on_event(&self, event: &ElementEvent) -> bool {
        match event {
            ElementEvent::PointerMove(position, PointerSource::Mouse, _) => {
                self.set_hovered(
                    self.hit_at(position.x, position.y)
                        .map(|hit| hit.node_id),
                );
                false
            }
            ElementEvent::PointerExited(PointerSource::Mouse, _) => {
                self.set_hovered(None);
                self.interaction
                    .borrow_mut()
                    .cancel();
                false
            }
            ElementEvent::PointerDown(position, _, _) => {
                let hit = self.hit_at(position.x, position.y);
                self.interaction
                    .borrow_mut()
                    .pointer_down(
                        hit.as_ref()
                            .map(|hit| hit.node_id),
                    );
                hit.is_some()
            }
            ElementEvent::PointerUp(position, _, _) => {
                let hit = self.hit_at(position.x, position.y);
                let pressed = self
                    .interaction
                    .borrow_mut()
                    .pointer_up(
                        hit.as_ref()
                            .map(|hit| hit.node_id),
                    );
                if pressed.is_some()
                    && let Some(hit) = hit
                {
                    self.execute_callbacks(hit);
                    request_animation_frame();
                    true
                } else {
                    false
                }
            }
            ElementEvent::Cancel => {
                self.interaction
                    .borrow_mut()
                    .cancel();
                false
            }
            _ => false,
        }
    }
}

#[derive(Default)]
pub(crate) struct SvgInteraction {
    pressed: Option<SvgNodeId>,
}

impl SvgInteraction {
    pub(crate) fn pointer_down(&mut self, hit: Option<SvgNodeId>) {
        self.pressed = hit;
    }

    pub(crate) fn pointer_up(&mut self, hit: Option<SvgNodeId>) -> Option<SvgNodeId> {
        let pressed = self.pressed.take();
        if pressed == hit { pressed } else { None }
    }

    fn cancel(&mut self) {
        self.pressed = None;
    }
}

pub(crate) fn resolved_svg_size(
    viewport: SvgViewport,
    width: Option<f32>,
    height: Option<f32>,
) -> (f32, f32) {
    let ratio = viewport.width
        / viewport
            .height
            .max(f32::EPSILON);
    match (width, height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (width, width / ratio.max(f32::EPSILON)),
        (None, Some(height)) => (height * ratio, height),
        (None, None) => (viewport.width, viewport.height),
    }
}

pub(crate) fn overrides_for_rules(
    scene: &SvgScene,
    rules: &[(SvgSelector, SvgStyle)],
) -> Vec<SvgNodeStyleOverride> {
    scene
        .nodes
        .iter()
        .filter_map(|node| {
            let mut result = SvgNodeStyleOverride {
                node_id: node.node_id,
                fill: None,
                stroke: None,
                opacity: None,
                transform: None,
            };
            let mut matched = false;
            for (_, style) in rules
                .iter()
                .filter(|(selector, _)| selector.matches(node))
            {
                matched = true;
                if style.fill.is_some() {
                    result.fill = style.fill;
                }
                if style.stroke.is_some() {
                    result.stroke = style.stroke;
                }
                if style.opacity.is_some() {
                    result.opacity = style.opacity;
                }
                if style.transform.is_some() {
                    result.transform = style.transform;
                }
            }
            matched.then_some(result)
        })
        .collect()
}

pub(crate) fn hit_test_scene(
    scene: &SvgScene,
    bounds: Bounds,
    x: f32,
    y: f32,
    overrides: &[SvgNodeStyleOverride],
) -> Option<SvgHit> {
    if bounds.width <= 0.0
        || bounds.height <= 0.0
        || x < bounds.x
        || y < bounds.y
        || x > bounds.x + bounds.width
        || y > bounds.y + bounds.height
    {
        return None;
    }
    let scene_point = (
        (x - bounds.x) * scene.viewport.width / bounds.width,
        (y - bounds.y) * scene.viewport.height / bounds.height,
    );
    for node in scene
        .nodes
        .iter()
        .rev()
        .filter(|node| node.visible && node.geometry.is_some())
    {
        let node_override = overrides
            .iter()
            .find(|value| value.node_id == node.node_id);
        if node_override
            .and_then(|value| value.opacity)
            .unwrap_or(node.opacity)
            <= 0.0
        {
            continue;
        }
        let transform = node_override
            .and_then(|value| value.transform)
            .unwrap_or(node.transform);
        let Some(inverse) = transform.inverse() else {
            continue;
        };
        let point = inverse.transform_point(scene_point.0, scene_point.1);
        let Some(geometry) = scene.geometry(node) else {
            continue;
        };
        if hits_geometry(node, node_override, geometry, point) {
            return Some(SvgHit {
                node_id: node.node_id,
                metadata: SvgNodeMetadata {
                    svg_id: node.svg_id.clone(),
                    classes: node.classes.clone(),
                    element: node.element,
                },
            });
        }
    }
    None
}

fn hits_geometry(
    node: &SvgNode,
    node_override: Option<&SvgNodeStyleOverride>,
    geometry: &SvgGeometry,
    point: (f32, f32),
) -> bool {
    let contours = flatten_geometry(geometry);
    let fill_visible = match node_override.map(|value| value.fill) {
        Some(Some(None)) => false,
        Some(Some(Some(_))) => true,
        Some(None) | None => node.fill.is_some(),
    };
    let fill_rule = node
        .fill
        .as_ref()
        .map(|fill| fill.rule)
        .unwrap_or(SvgFillRule::NonZero);
    if fill_visible && contains_point(&contours, point, fill_rule) {
        return true;
    }
    let stroke_visible = match node_override.map(|value| value.stroke) {
        Some(Some(None)) => false,
        Some(Some(Some(_))) => true,
        Some(None) | None => node.stroke.is_some(),
    };
    if stroke_visible && let Some(stroke) = &node.stroke {
        let threshold = stroke.width * 0.5;
        return contours
            .iter()
            .any(|contour| {
                contour
                    .windows(2)
                    .any(|segment| {
                        point_segment_distance(point, segment[0], segment[1]) <= threshold
                    })
            });
    }
    false
}

fn flatten_geometry(geometry: &SvgGeometry) -> Vec<Vec<(f32, f32)>> {
    let mut contours = Vec::new();
    let mut contour = Vec::new();
    let mut current = (0.0, 0.0);
    for command in geometry
        .commands
        .iter()
        .copied()
    {
        match command {
            SvgPathCommand::MoveTo { x, y } => {
                if !contour.is_empty() {
                    contours.push(std::mem::take(&mut contour));
                }
                current = (x, y);
                contour.push(current);
            }
            SvgPathCommand::LineTo { x, y } => {
                current = (x, y);
                contour.push(current);
            }
            SvgPathCommand::QuadraticTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                let start = current;
                for step in 1..=16 {
                    let t = step as f32 / 16.0;
                    let inverse = 1.0 - t;
                    contour.push((
                        inverse * inverse * start.0 + 2.0 * inverse * t * control_x + t * t * x,
                        inverse * inverse * start.1 + 2.0 * inverse * t * control_y + t * t * y,
                    ));
                }
                current = (x, y);
            }
            SvgPathCommand::CubicTo {
                control1_x,
                control1_y,
                control2_x,
                control2_y,
                x,
                y,
            } => {
                let start = current;
                for step in 1..=24 {
                    let t = step as f32 / 24.0;
                    let inverse = 1.0 - t;
                    contour.push((
                        inverse.powi(3) * start.0
                            + 3.0 * inverse * inverse * t * control1_x
                            + 3.0 * inverse * t * t * control2_x
                            + t.powi(3) * x,
                        inverse.powi(3) * start.1
                            + 3.0 * inverse * inverse * t * control1_y
                            + 3.0 * inverse * t * t * control2_y
                            + t.powi(3) * y,
                    ));
                }
                current = (x, y);
            }
            SvgPathCommand::Close => {
                if contour.first() != contour.last()
                    && let Some(first) = contour.first().copied()
                {
                    contour.push(first);
                }
            }
        }
    }
    if !contour.is_empty() {
        contours.push(contour);
    }
    contours
}

fn contains_point(contours: &[Vec<(f32, f32)>], point: (f32, f32), rule: SvgFillRule) -> bool {
    let mut winding = 0_i32;
    let mut crossings = 0_u32;
    for contour in contours {
        for segment in contour.windows(2) {
            let (a, b) = (segment[0], segment[1]);
            if (a.1 > point.1) != (b.1 > point.1) {
                let x = a.0 + (point.1 - a.1) * (b.0 - a.0) / (b.1 - a.1);
                if x > point.0 {
                    crossings += 1;
                }
            }
            if a.1 <= point.1 {
                if b.1 > point.1 && cross(a, b, point) > 0.0 {
                    winding += 1;
                }
            } else if b.1 <= point.1 && cross(a, b, point) < 0.0 {
                winding -= 1;
            }
        }
    }
    match rule {
        SvgFillRule::EvenOdd => crossings % 2 == 1,
        SvgFillRule::NonZero => winding != 0,
    }
}

fn cross(a: (f32, f32), b: (f32, f32), point: (f32, f32)) -> f32 {
    (b.0 - a.0) * (point.1 - a.1) - (point.0 - a.0) * (b.1 - a.1)
}

fn point_segment_distance(point: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let segment = (b.0 - a.0, b.1 - a.1);
    let length_squared = segment.0 * segment.0 + segment.1 * segment.1;
    if length_squared <= f32::EPSILON {
        return (point.0 - a.0).hypot(point.1 - a.1);
    }
    let t = (((point.0 - a.0) * segment.0 + (point.1 - a.1) * segment.1) / length_squared)
        .clamp(0.0, 1.0);
    (point.0 - (a.0 + t * segment.0)).hypot(point.1 - (a.1 + t * segment.1))
}
