# Layout

The Oxidize framework uses a Flexbox-inspired layout engine that allows you to easily structure your UI horizontally, vertically, or in custom box alignments. 

## Flexbox-Inspired Containers

### Flex

The foundation of the layout system is the `Flex!` macro, which arranges its children in a flexible box layout.
- **`alignment`**: Defines how the children should be aligned within the box.
- **`spacing`**: Controls the space between children.
- **`direction`**: Defines the direction of the layout (e.g. `Horizontal`, `Vertical`).





```rust
Flex!(
    alignment: BoxAlignment::Center,
    children: vec![
        Text!("Left"),
        Text!("Center"),
        Text!("Right"),
    ],
    // Optional spacing or alignment attributes can be applied here
)
```

> Note: Consider using `Row!` and `Column!` instead of `Flex!` for readability and consistency.

### Row and Column

To align child widgets linearly, use `Row!` and `Column!`:
- **`Row!`**: Alway arranges its children horizontally.
- **`Column!`**: Alway arranges its children vertically.

```rust
Row!(
    children: vec![
        Text!("Left"),
        Text!("Center"),
        Text!("Right"),
    ],
    // Optional spacing or alignment attributes can be applied here
)
```

### The Container Widget

The `Container!` macro wraps another widget, allowing you to easily add structural elements:
- **Padding**
- **Margin**
- **Decoration** (such as backgrounds or borders)

```rust
Container!(
    padding: LayoutSpacing::all(Spacing::Px(16)),
    child: Text!("Padded Text")
)
```

### SizedBox

If you need a box with specific dimensions, use `SizedBox!`:
- **`width`**: The width dimension.
- **`height`**: The height dimension.
- **`color`**: Optional background color.
- **`child`**: The widget to contain.

```rust
SizedBox!(
    width: 100.0,
    height: 100.0,
    color: Colors::Blue,
    child: Text!("SizedBox")
)
```


### ZeroSizedBox

When a container need a widget but you don't want it to take up space or need a placeholder,

**Instead of using `SizedBox!`**

```rust
Container!(!(
    child: SizedBox!(
        width: 0.0,
        height: 0.0,
    )
)
```
**Use `ZeroSizedBox!`** 

```rust
Container!(!(
    child: ZeroSizedBox
)
```

> Note: `ZeroSizedBox` has better performance than `SizedBox` while `ZeroSizedBox` is completely skip the layout.


### Stack and Positioned

To layer widgets on top of each other, use the `Stack!` macro. You can combine it with `Positioned!` to control the exact placement of overlapping children.

- **`Stack!`**: Contains a list of children that are rendered in order.
- **`Positioned!`**: Controls precise layout (e.g., `left`, `top`, `right`, `bottom`, and `position` such as `Absolute` or `Relative`).

```rust
Stack!(
    children: vec![
        Container!( /* Background layer */ ),
        Positioned!(
            top: 20.0,
            left: 20.0,
            child: Text!("Overlapping Text")
        )
    ]
)
```

## Alignment and Spacing

Oxidize provides precise control over how elements are distributed within Flexbox-style layouts using alignment and spacing properties.

### BoxAlignment

The `BoxAlignment` enum defines how elements should be positioned within their parent container's axis.

Common values include:
- `Start`: Aligns children to the beginning of the container.
- `Center`: Centers children within the container.
- `End`: Aligns children to the end of the container.
- `Stretch`: Stretches children to fill the available cross-axis space.

**Example: Centering content in a Row**

```rust
Row!(
    alignment: BoxAlignment::Center,
    children: vec![
        Text!("Item 1"),
        Text!("Item 2"),
    ]
)
```

### Spacing and LayoutSpacing

The `Spacing` and `LayoutSpacing` types are used to specify margins and padding around elements.

- **`Spacing`**: An enum defining the unit of spacing. It can be a fixed pixel value (`Px(u32)`), a percentage (`Percent(u32)`), or `None`.
- **`LayoutSpacing`**: A struct containing `top`, `bottom`, `left`, and `right` spacing values. It provides convenient helper methods such as `all`, `vertical`,`horizontal`.

`LayoutSpacing` helper methods:
- `LayoutSpacing::all(space)`: Applies the same spacing to all four sides.
- `LayoutSpacing::vertical(space)`: Applies spacing to top and bottom.
- `LayoutSpacing::horizontal(space)`: Applies spacing to left and right.

**Example: Using LayoutSpacing**

```rust
Container!(
    padding: LayoutSpacing::all(Spacing::Px(16)),
    child: Text!("Padded Text")
)
```

## Scrolling

When a layout exceeds the viewable bounds, use the `Scrollable!` macro.

```rust
Scrollable!(
    child: Column!(
        // Long list of widgets here
    )
)
```
> **⚠️ Unstable**: `Scrollable` supported on all majors but is currently it has some undefined behavior..

## Animations and Transitions (Experimental)

Oxidize includes an animation framework to bring your layouts to life. (Note: These features are marked `⛔️ Very Unstable`)
- **`AnimationController`**: Controls playback direction and looping (forward, reverse, repeat).
- **Curves**: Smooth interpolations like `EaseIn`, `EaseOut`, `Bounce`, and `Linear`.
- **Effects**: Wrappers like `Animated` that apply transitions (`Opacity`, `Scale`, `Translate`, `Rotate`, `SlideX`, `SlideY`).
