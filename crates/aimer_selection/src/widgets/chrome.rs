use std::rc::Rc;

use aimer_container::{Container, Opacity, SizedBox};
use aimer_flex::{BoxAlignment, Column, Row};
use aimer_input::gesture::GestureEvent;
use aimer_input::gesture::gesture_detector::GestureDetector;
use aimer_input::mouse_region::MouseRegion;
use aimer_provider::ProviderContext;
use aimer_style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, LayoutSpacing, Spacing, TextStyle,
    ThemeData, ThemeTokens, apply_state_layer,
};
use aimer_text::Text;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{AnyWidget, Focusable, Widget};

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
    // `width`/`height` are set directly on the decorated `Container` itself
    // rather than through a wrapping `SizedBox`. `Dimension::Px` is resolved
    // the same way during layout (`computed_size`) and during paint
    // (`draw`'s own local size calculation): both read the fixed pixel value
    // and ignore the ambient box constraint entirely. Nesting an
    // `Auto`-sized decorated `Container` inside a `SizedBox` instead made
    // the painted size depend on whatever constraint happened to be ambient
    // at paint time, which does not always match what layout reserved —
    // that mismatch was silently painting this indicator at zero size.
    Container::new()
        .width(size)
        .height(size)
        .box_decoration(
            BoxDecoration::new()
                .background_color(fill)
                .border(BoxBorder::all(
                    BorderSlice::new()
                        .style(BorderStyle::Solid)
                        .stroke(2.0)
                        .color(border_color),
                ))
                .border_radius(aimer_style::BorderRadius::new().top_left(radius).top_right(radius).bottom_right(radius).bottom_left(radius)),
        )
        .child(SizedBox::new().width(size).height(size))
        .boxed()
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
    // density minimum itself is still met per row: `indicator`'s `SizedBox`
    // is sized through `density.target_size`, which already floors at the
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
