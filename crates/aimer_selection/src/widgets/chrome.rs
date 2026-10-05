use std::rc::Rc;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_canvas::Canvas;
use aimer_container::{Container, Opacity};
use aimer_cupid::utilities::{Color as V2Color, Rect};
use aimer_flex::{BoxAlignment, Column, Row};
use aimer_input::gesture::GestureEvent;
use aimer_input::gesture::gesture_detector::GestureDetector;
use aimer_input::mouse_region::MouseRegion;
use aimer_provider::ProviderContext;
use aimer_style::{LayoutSpacing, Spacing, TextStyle, ThemeData, ThemeTokens, apply_state_layer};
use aimer_text::Text;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, EventTreeRole, Focusable,
    LayoutElement, PortableWidget, Rebuildable, VisitorElement, Widget,
};

use crate::Key;

use super::keys::KeyRelay;

pub(crate) fn tokens(ctx: &BuildContext) -> ThemeTokens {
    ctx.try_copied::<ThemeData>()
        .map(|theme| theme.tokens())
        .unwrap_or_else(ThemeTokens::light)
}

pub(crate) fn surface_color(tokens: &ThemeTokens, hovered: bool, pressed: bool, disabled: bool) -> Color {
    let mut color = tokens.colors.surface;
    if disabled {
        color = apply_state_layer(color, tokens.state.disabled);
    } else if pressed {
        color = apply_state_layer(color, tokens.state.pressed);
    } else if hovered {
        color = apply_state_layer(color, tokens.state.hover);
    }
    color
}

pub(crate) fn label_style(tokens: &ThemeTokens, disabled: bool) -> TextStyle {
    let color = if disabled {
        apply_state_layer(tokens.colors.on_surface, tokens.state.disabled)
    } else {
        tokens.colors.on_surface
    };
    TextStyle::new()
        .font_size(tokens.typography.body.font_size as u32)
        .color(color)
}

pub(crate) fn error_text(tokens: &ThemeTokens, error: Option<&str>) -> Option<AnyWidget> {
    error.map(|error| {
        Text::new(error.to_owned())
            .text_style(
                TextStyle::new()
                    .font_size(tokens.typography.label.font_size as u32)
                    .color(tokens.colors.error),
            )
            .boxed()
    })
}

pub(crate) fn indicator(
    tokens: &ThemeTokens,
    filled: bool,
    radius: f32,
    hovered: bool,
    pressed: bool,
    disabled: bool,
) -> AnyWidget {
    let size = tokens.density.target_size(20.0);
    let border_color = if disabled {
        apply_state_layer(tokens.colors.outline, tokens.state.disabled)
    } else if filled {
        tokens.colors.primary
    } else {
        tokens.colors.outline
    };
    let fill = if filled {
        if disabled {
            apply_state_layer(tokens.colors.primary, tokens.state.disabled)
        } else {
            tokens.colors.primary
        }
    } else {
        surface_color(tokens, hovered, pressed, disabled)
    };
    // Keep the fixed target size on the indicator leaf so layout and both
    // render paths resolve the same dimensions under any parent.
    SelectionIndicator {
        size,
        radius,
        fill,
        border_color,
    }
    .boxed()
}

/// A leaf painter for the checkbox mark. Keeping the rounded fill and border
/// together avoids exposing a rounded child clip that the retained tree cannot
/// represent yet.
struct SelectionIndicator {
    size: f32,
    radius: f32,
    fill: Color,
    border_color: Color,
}

impl PortableWidget for SelectionIndicator {}

impl Widget for SelectionIndicator {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        RawSelectionIndicator {
            size: self.size,
            radius: self.radius,
            fill: self.fill,
            border_color: self.border_color,
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "SelectionIndicator"
    }
}

struct RawSelectionIndicator {
    size: f32,
    radius: f32,
    fill: Color,
    border_color: Color,
}

impl VisitorElement for RawSelectionIndicator {
    fn debug_name(&self) -> &'static str {
        "SelectionIndicator"
    }
}

impl EventElement for RawSelectionIndicator {
    fn event_tree_role(&self) -> EventTreeRole {
        EventTreeRole::Transparent
    }
}

impl Rebuildable for RawSelectionIndicator {}

impl LayoutElement for RawSelectionIndicator {
    fn size(&self) -> Option<Size> {
        Some(Size::square(self.size))
    }

    fn is_layout_stable(&self) -> bool {
        true
    }
}

impl Drawable for RawSelectionIndicator {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.size * ctx.scale;
        ctx.canvas.fill_rect_with_border_and_outline_per_side(
            Vec2d::ZERO,
            ResolvedSize {
                width: size,
                height: size,
            },
            self.fill,
            [self.radius * ctx.scale; 4],
            [2.0 * ctx.scale; 4],
            self.border_color,
            [0.0; 4],
            Color::Transparent,
        );
    }

    fn can_paint_local_v2(&self, ctx: &BuildContext) -> bool {
        ctx.scale.is_finite()
            && ctx.scale > 0.0
            && self.size.is_finite()
            && self.size > 0.0
            && self.size <= 1_000_000.0
            && self.radius.is_finite()
            && self.radius >= 0.0
    }

    fn paint_local_v2(&self, ctx: &BuildContext) {
        let canvas = Canvas::of(ctx);
        let (red, green, blue, alpha) = self.fill.to_rgba();
        let (border_red, border_green, border_blue, border_alpha) =
            self.border_color.to_rgba();
        canvas.fill_rect_styled(
            Rect::new(0.0, 0.0, self.size, self.size),
            V2Color::rgba8(red, green, blue, alpha),
            [self.radius; 4],
            [2.0; 4],
            V2Color::rgba8(border_red, border_green, border_blue, border_alpha),
            [0.0; 4],
            V2Color::transparent(),
        );
        canvas.finish();
    }
}

pub(crate) fn labeled_row(
    tokens: &ThemeTokens,
    indicator: AnyWidget,
    label: Option<&str>,
    disabled: bool,
) -> AnyWidget {
    let mut children = vec![indicator];
    if let Some(label) = label {
        children.push(
            Text::new(label.to_owned())
                .text_style(label_style(tokens, disabled))
                .boxed(),
        );
    }
    Row::new()
        .vertical_alignment(BoxAlignment::Center)
        .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.small as u32)))
        .children(children)
        .boxed()
}

pub(crate) fn labeled_option_row(
    tokens: &ThemeTokens,
    indicator: AnyWidget,
    label: &str,
    disabled: bool,
    highlighted: bool,
) -> AnyWidget {
    let mut style = label_style(tokens, disabled);
    if highlighted && !disabled {
        style = style.color(tokens.colors.primary);
    }
    Row::new()
        .vertical_alignment(BoxAlignment::Center)
        .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.small as u32)))
        .children(vec![
            indicator,
            Text::new(label.to_owned()).text_style(style).boxed(),
        ])
        .boxed()
}

pub(crate) fn control_shell(
    tokens: &ThemeTokens,
    hovered: bool,
    pressed: bool,
    disabled: bool,
    error: Option<&str>,
    child: AnyWidget,
) -> AnyWidget {
    // Deliberately no explicit `.height(...)` here: a fixed single-row height
    // (the density minimum target) would cap this shell to one row and clip
    // every option beyond the first for a multi-row control (`RadioGroup`,
    // an open `Select`/`Autocomplete`). `Dimension::Auto` — `Container`'s
    // default — grows to fit whatever content is passed in instead. The
    // density minimum itself is still met per row: `indicator` is sized
    // through `density.target_size`, which already floors at the
    // minimum touch target, so a single-row control (`Checkbox`, `Switch`)
    // still measures at least that tall without this shell forcing it.
    let mut children = vec![child];
    if let Some(error) = error_text(tokens, error) {
        children.push(error);
    }
    let body = Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(tokens.spacing.x_small as u32)))
        .color(surface_color(tokens, hovered, pressed, disabled))
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.x_small as u32)))
                .children(children),
        );
    if disabled {
        Opacity::new()
            .opacity(1.0)
            .child(body)
            .boxed()
    } else {
        body.boxed()
    }
}

pub(crate) fn wrap_interactive(
    disabled: bool,
    on_activate: Rc<dyn Fn()>,
    on_pressed: Rc<dyn Fn(bool)>,
    on_hovered: Rc<dyn Fn(bool)>,
    on_key: Rc<dyn Fn(Key) -> bool>,
    child: AnyWidget,
) -> AnyWidget {
    if disabled {
        return child;
    }
    Focusable::new()
        .child(
            KeyRelay::new().on_key(move |key| (on_key)(key)).child(
                MouseRegion::new()
                    .on_hover_enter({
                        let on_hovered = Rc::clone(&on_hovered);
                        move || on_hovered(true)
                    })
                    .on_hover_exit({
                        let on_hovered = Rc::clone(&on_hovered);
                        move || on_hovered(false)
                    })
                    .child(
                        GestureDetector::new()
                            .on_tap({
                                let on_activate = Rc::clone(&on_activate);
                                move || on_activate()
                            })
                            .on_gesture({
                                let on_pressed = Rc::clone(&on_pressed);
                                move |event: GestureEvent| match event {
                                    GestureEvent::TapDown { .. } => on_pressed(true),
                                    GestureEvent::TapUp { .. } | GestureEvent::TapCancel => {
                                        on_pressed(false)
                                    }
                                    _ => {}
                                }
                            })
                            .child(child),
                    ),
            ),
        )
        .boxed()
}
