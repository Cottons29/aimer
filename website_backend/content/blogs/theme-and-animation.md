# Previewing The Theme And Animation Systems

A polished interface should feel consistent when it is still and intentional when it moves. Aimer treats those two
concerns as parts of the same system: themes provide typed values to a widget subtree, while animations interpolate
those values as the tree is rebuilt.

This post introduces Aimer's built-in light and dark themes, shows how descendants consume semantic colors, and then
uses the same ideas to animate both an entire theme and an individual widget.

## Start With Semantic Colors

`ThemeData` is Aimer's default application theme. It describes colors by purpose instead of by a specific shade:

- `primary_color` and `on_primary_color`
- `background_color` and `on_background_color`
- `surface_color` and `on_surface_color`

The `on_*` colors are intended for content drawn over their matching base colors. This keeps widgets reusable: a label
does not need to know whether the current background is light or dark; it only needs the correct semantic foreground.

Aimer includes light and dark defaults, and each value can be customized with the builder API:

```rust
use aimer::style::{Color, ThemeData};

let light = ThemeData::light();
let dark = ThemeData::dark();

let brand_theme = ThemeData::light()
    .primary_color(Color::Rgba(88, 86, 214, 255))
    .on_primary_color(Color::WHITE)
    .background_color(Color::Rgba(248, 249, 252, 255));
```

Starting from a built-in theme is useful because every semantic role receives a sensible value before the application
overrides its brand colors.

## Provide a Theme to the Widget Tree

`AnimatedTheme` installs a theme above its child. When its `data` changes, Aimer interpolates the old and new values and
publishes each intermediate theme to descendants.

```rust
use std::time::Duration;

use aimer::animation::Curve;
use aimer::style::{AnimatedTheme, ThemeData};
use aimer::Widget;

fn themed_app(child: impl Widget + 'static, dark_mode: bool) -> impl Widget {
    let theme = if dark_mode {
        ThemeData::dark()
    } else {
        ThemeData::light()
    };

    AnimatedTheme::new()
        .data(theme)
        .duration(Duration::from_millis(250))
        .curve(Curve::EaseInOut)
        .child(child)
}
```

The child comes last, following Aimer's widget builder convention. Rebuild this widget with a different `ThemeData` and
the existing subtree transitions to the new palette instead of changing every color at once.

`AnimatedTheme` uses a 200 millisecond linear transition by default. Set the duration to `Duration::ZERO` when an
immediate update is preferred.

## Read the Current Theme

A descendant subscribes to the nearest matching theme with `ThemeData::of(ctx)`. The widget is rebuilt whenever the
published theme value changes, including during an animated transition.

```rust
use aimer::style::{TextStyle, Theme, ThemeData};
use aimer::{BuildContext, Container, Text, Widget};

fn themed_message(ctx: &BuildContext) -> impl Widget {
    let theme = ThemeData::of(ctx);

    Container::new()
        .color(theme.background_color)
        .child(
            Text::new("Aimer follows the current theme")
                .text_style(TextStyle::new().color(theme.on_background_color)),
        )
}
```

Use `ThemeData::read(ctx)` when a value is needed without subscribing the current build, or `ThemeData::copied(ctx)` for
a copied value it cheap because ThemeData implemented Copy trait. All three lookups require a matching `AnimatedTheme` ancestor; calling them outside that subtree is a
programming error.

## A Light and Dark Theme Toggle

Theme selection belongs to application state. A small mode enum keeps that state separate from the palette itself:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    fn theme(self) -> ThemeData {
        match self {
            Self::Light => ThemeData::light(),
            Self::Dark => ThemeData::dark(),
        }
    }

    fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}
```

Store `ThemeMode` in a `StatefulWidget`, update it through `StateUpdater`, and pass `mode.theme()` to `AnimatedTheme`.
This is the pattern used by the Aimer website: one state change animates the background, text, surfaces, icons, and any
other descendant that reads semantic theme colors.

> press the button in the top-right corner to see the theme change. 

## Animate a Value Implicitly

Theme transitions are one form of implicit animation: the application declares a new target and Aimer handles the
movement between values. `ImplicitAnimatedBuilder` makes the same pattern available for any `Animatable` value.

```rust
use std::time::Duration;

use aimer::animation::{Curve, ImplicitAnimatedBuilder};
use aimer::{Container, Widget};

fn animated_panel(target_width: f32) -> impl Widget {
    ImplicitAnimatedBuilder::new(
        target_width,
        Duration::from_millis(300),
        Curve::FastOutSlowIn,
        |width: &f32| {
            Container::new()
                .width(*width)
                .height(48)
        },
    )
    .key("animated-panel-width")
}
```

The first target is displayed immediately. On a later rebuild, a different target starts the animation from the value
currently on screen. If the target changes again before the transition finishes, Aimer retargets from that displayed
value rather than jumping back to an old endpoint.

Stable identity matters. Keep the implicit widget's key unchanged across target updates; replacing the key creates new
state, so there is no previous value from which to animate.

The builder can drive several visual properties from the same progress value. For example, the platform selector on
the Aimer website uses one animation to interpolate width, corner radius, background and foreground colors, checkmark
opacity, and spacing. Keeping those changes on one timeline makes the interaction feel like one motion rather than a
collection of unrelated effects.

## Use an Explicit Controller When You Need Lifecycle Control

Implicit animation is ideal when the destination is known. Use `AnimationController` when the application must decide
when an animation starts, reverses, repeats, or stops.

```rust
use std::time::Duration;

use aimer::animation::{AnimationController, Curve, FadeTransition};
use aimer::Widget;

fn fade_in(child: impl Widget + 'static) -> impl Widget {
    let controller = AnimationController::new(
        Duration::from_millis(250),
        Curve::EaseOut,
    );
    controller.forward_from_first_tick();

    FadeTransition::new(controller, child)
}
```

Transition widgets cover common effects:

- `FadeTransition` controls opacity.
- `SlideTransition` moves a child from an offset to its natural position.
- `ScaleTransition` applies uniform scale.
- `RotationTransition` rotates a child in turns.
- `AnimatedBuilder` rebuilds arbitrary content from the controller's current value.

Creating a controller does not start it. Call `forward()`, `forward_from_first_tick()`, or another lifecycle method
before expecting motion. `forward_from_first_tick()` is a good choice for a newly mounted transition because timing
begins when the first animation frame is sampled.

## Curves, Tweens, and Animatable Values

An animation controller produces normalized progress from `0.0` to `1.0`. A `Curve` transforms that progress to shape
the character of the motion, while a `Tween<T>` maps it onto a useful range.

```rust
use aimer::animation::{AnimatableExt, Curve};

let width = 120.0_f32
    .tween_to(280.0)
    .lerp(Curve::EaseOut.transform(0.5));
```

Linear motion is useful for constant-rate changes, `EaseOut` works well for elements entering the screen, and
`FastOutSlowIn` gives selection and layout changes a responsive start with a gentle finish. Aimer also provides cubic
Bezier, bounce, elastic, and deceleration curves.

Built-in animatable values include numeric values and tuples. Colors used by animated widgets can be converted to
`Rgba`, interpolated with `lerp`, and converted back to `Color`. Applications can also define richer typed themes with
`#[derive(Theme)]`; each field must itself implement `Animatable` so `AnimatedTheme` can interpolate it.

## Choosing the Right Tool

Use the smallest animation abstraction that expresses the interaction:

1. Use `AnimatedTheme` for coordinated application-wide visual changes.
2. Use `ImplicitAnimatedBuilder` when rebuilding with a new target should trigger motion automatically.
3. Use `FadeTransition`, `SlideTransition`, `ScaleTransition`, or `RotationTransition` for one controller-driven effect.
4. Use `AnimatedBuilder` when one controller must construct a custom frame.
5. Use `AnimatedSwitcher` when keyed children should cross-fade as content changes.

Themes make widgets agree on what colors mean. Animations make changes between those values understandable. Combining
the two lets an Aimer application remain declarative while still delivering transitions that feel deliberate across
desktop, mobile, and web.