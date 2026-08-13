# Better DX with Aimer and News

Aimer is a cross-platform UI framework with an SDK for building native and web apps in Rust. it has its own rendering
engine and supports macOS, Windows, Android, iOS and Web(wasm).

## Story Behind It

By default, Aimer apps run with a simple `cargo run`. But that doesn't hold up on mobile: iOS and Android have no way to
run a raw executable, apps need to be bundled into platform-native formats, an iOS app bundle or an Android APK.
Without Aimer CLI, that means manually wiring up an Xcode project for iOS and a Gradle project for Android by hand,
every time.

That's why Aimer CLI was created.

## Aimer CLI

AimerCli is a command-line tool for scaffolding, building, assembling, and running your application. there provides some commands: 

- `create`: create a new Aimer project. the project structure looks like this:

```text
.
├── builds
│   ├── android
│   ├── web
│   ├── ios
│   ├── linux
│   ├── macos
│   └── windows
├── src
│   └── main.rs
├── Cargo.toml
├── aimer.toml
└── README.md
```

`create` asks a few questions first, app name, description, version, author, a group like `com.example.app`, and a
multi-select of the targets you actually care about, then writes only the scaffolds you picked. A project that will
never ship to iOS does not get an Xcode project it has to keep migrating. If any step of the scaffold fails, the
half-written directory is removed instead of being left behind for you to clean up.

The answers land in `aimer.toml`, the project manifest every other command reads:

```toml
[package]
name = "my_app"           # cargo package and binary name
version = "0.1.0"         # shown in the bundle's version fields
description = "A cool app"
author = "Cottons29"
group = "com.example.app" # the bundle/application id on macOS, iOS and Android

[build]
default_target = "macos"  # what `aimer build` picks when you don't pass --target

[assets]
# Copied into every platform's asset root, path preserved. The string here is
# also the runtime lookup key, so `ImageSource::Asset("assets/logo.png")` works
# on all six targets.
files = ["assets/logo.png", "assets/fonts/Inter.ttf"]
```

`group` matters more than it looks: it becomes the macOS/iOS bundle identifier and the Android application id, so the
platform scaffolds are generated from it and it is not something you want to change later on a whim.

- `run`: the command you actually live in.

With no arguments it lists every device it can find, the desktop you are sitting at, the web browser, every Android
device or emulator visible to `adb`, every booted iOS simulator, and lets you pick one. Scripts and IDEs don't want a
picker, so `--target macos` and `--device <id>` skip it entirely, and `--release` runs the whole pipeline in release
mode.

Once something is picked, the terminal turns into a console with three panes: **build logs**, **app logs**, and the
**inspector**, with a status bar that walks through locking, fetching, compiling, building, launching, running. Build
errors are collected and printed as one block at the end of the build instead of being scattered through the noise, and
a widget panic the app recovered from arrives as its own block too, you can read what happened without the app dying,
which is a story of its own further down.
`src/` and `crates/` are watched, so saving a `.rs` file rebuilds and relaunches the app on its own, debounced, so a
formatter touching ten files costs one rebuild. Logs scroll and page, a selection can be copied to the clipboard, and
the panes cycle with `Tab`.

That console is a terminal application, and terminal applications are exactly what an IDE run window is not. `--no-tui`
turns it off and prints plain lines to stdout and stderr, which is what you want from CI and from a run configuration
inside an IDE.

- `doctor`: the answer to "why doesn't it build on my machine".

It probes the toolchains the CLI shells out to, `rustc`, `cargo`, `trunk` for the web, `xcrun` for Apple platforms,
`adb` and `gradle` for Android, the platform linker (`cc` and `pkg-config` on Linux, MSVC's `link` on Windows), and the
Rust targets each platform needs, then prints what is missing along with the command that installs it. Tools that are
only needed for optional features are reported as optional rather than as a failure, so a missing `llvm-ar` doesn't look
like a broken machine when all it costs you is markdown syntax highlighting.

The rest of the commands, `build`, `assemble`, `clean`, `migrate` and `completions`, are covered in *New Updated
Aimer CLI*.

## News

### A panic no longer takes the app down

A widget that panics while it is being built used to end the process, which is a strange punishment for a typo in a
`build` method: everything you had typed into the app, every screen you had navigated to, gone, and the only trace left
was a stack trace in a terminal you had to go read.

Building a widget is now wrapped in a recovery boundary. Every phase the framework calls into your code,
`create_state`, `init_state`, a queued state mutation, `build`, a child's `to_element`, `adopt_config_from`, keyed state
construction, is caught on its own, so the diagnostic knows not just *which* widget failed but *what it was doing*. The
failing subtree is replaced by an error element, the rest of the tree keeps building, and the app keeps running.

The interesting part is what the report says. A recovered panic only carries its payload, the message, while the place
it came from is known to the panic hook alone. `aimer_utils::PanicSite` bridges the two: it records the location of
panics raised on this thread while a build is being watched (and keeps the default handler quiet meanwhile), so the
framework can point at the offending expression as precisely as the runtime does:

```text
Widget `HttpRequestButton` panicked during build: called `Option::unwrap()` on a `None` value

at jaime/src/http_request_button.rs:117:67

        let panic: Option<i32> = Option::None.unwrap();
                                 ^^^^^^^^^^^^^^^^^^^^^
```

The source line is read from the file when it is on disk and from sources embedded at compile time when it is not, which
is what makes the same report work on a phone and in a browser. A backtrace is appended only when the app was run with
`RUST_BACKTRACE` set, a caret under the expression is usually the whole answer.

That message is drawn *in place of the subtree*, in a red panel in Aimer's bundled JetBrains Mono, because a
proportional face would leave the carets pointing at whatever happens to sit above them. It is also logged, which is
where `aimer run` picks it up: the console recognises a recovered panic and gives it the framed, width-filling block a
failed build gets, wrapped rather than truncated when the pane is narrow, cutting the carets off would hide the very
expression that panicked, and re-laid out when you resize the pane. Under `--no-tui` the same block is printed as plain
lines.

Release builds keep the details out of the binary's output: the message is logged and the panel renders a generic
description instead. And since the release profile aborts on panic, recovery is exactly what it should be, a
development-time affordance that costs nothing in a shipped app.

### Resizable

`Resizable` is a new container in `aimer_container`: a single-child box the user resizes by dragging any of its four
edges or four corners. The cursor changes to the matching resize shape as the pointer reaches a grab band, and the band
reaches slightly outside the border, so the shape changes when you arrive at the edge rather than after crossing it.

Which sides are live is a bit flag, so a widget can offer any subset of the eight:

```rust
Resizable::new()
    .width(320.0)
    .height(200.0)
    .min_width(120.0)
    .max_width(640.0)
    .direction(Direction::RIGHT | Direction::BOTTOM | Direction::BOTTOM_RIGHT)
    .on_resize(|size: ResolvedSize| println!("{} x {}", size.width, size.height))
    .on_resize_zone(|zone: Direction| println!("hovering {zone:?}"))
    .child(panel)
```

`on_resize` reports every step of a drag, and `on_resize_zone` reports the side under the pointer, the same answer the
cursor shape is drawn from, so a panel can highlight the edge it is about to be dragged by, and report
`Direction::NONE` the moment the pointer leaves. The size survives a `set_state` in the middle of a drag, which is what
keeps a live readout from snapping the box back while you are still holding it.

The parent still owns the origin, so a `Resizable` changes its own width and height and grows from its fixed top-left
corner. There is a working demo in the `jaime` crate.

### Venus can await the rest of the ecosystem

`aimer_venus` is Aimer's UI-thread runtime: tasks are polled on the thread that builds, lays out and paints, in the
phase they were spawned into. That is what makes a task able to touch the widget tree without a lock, and it is also
why a `reqwest` call used to panic with *there is no reactor running*.

A runtime-backed future builds its resources on its **first poll** and looks its runtime up in a thread-local. Venus
polls on the UI thread, where that thread-local is empty. `TokioPollContext` enters the application's Tokio handle for
exactly the duration of one poll, scoped to the poll rather than to the task, because a guard held across an `await`
would leave the UI thread marked as being inside the runtime while it renders a frame.

It is installed once by the platform loop, next to `Venus::install`, so every spawn path gets it: `spawn`, `spawn_in`,
`spawn_frame`, `spawn_idle`, an `AsyncBuilder`, a future launched from a gesture handler. Nothing in the widget tree has
to know a runtime exists. Sockets and TLS handshakes run on Tokio's driver threads beside the frame, and the completion
comes home through Venus's waker.

### Focus is its own crate

Keyboard focus moved into `aimer_focus`, which owns exactly one decision, who receives keyboard and input-method
events, and holds no reference to an element tree, so focus policy can be reasoned about and tested on its own.

It is four pieces: `FocusNode`, the handle a focusable widget keeps across rebuilds; `FocusCandidate`, a target paired
with the identity of the element it is attached to `FocusManager`, which resolves the candidates of a frame into a
target and reports the transition; and `FocusTrap`, which confines focus to one region, what makes a modal an actual
*mode* rather than a floating box you can tab out of.

`aimer_widget` supplies the identities and turns a transition into `FocusLost` / `FocusGained`. `Focusable` wraps that
for widgets, and `Selectable` and `TextField` are wired through it, so a text field losing focus and a selection being
dropped are now the same event rather than two systems guessing about each other.

### Windows and Linux

`aimer run` now works on Windows and Linux, not just macOS. Both got their own build scaffolds under `builds/`, both are
picked up as devices by the run picker, and `doctor` checks the linker each one needs. Desktop Rust apps have always
been the easy target; the point is that the same command, the same console, and the same packaging path now apply on all
three desktops.

### Color is a struct now

`Color` used to be an enum carrying whatever the author wrote, RGB channels, HSLA components, a named color. It is now
a `#[repr(transparent)]` struct around a single `u32` of packed `0xAARRGGBB`, the layout every paint command and GPU
pipeline already expects.

The constructors keep their old shape, so nothing at a call site changes:

```rust
assert_eq!(Color::Rgb(255, 0, 0), Color::Hex(0xFF0000));
assert_eq!(Color::Hsl(0.0, 1.0, 0.5), Color::RED);
```

Each of them resolves to packed ARGB at construction, which buys three things: a style struct pays four bytes per color
instead of the twenty an HSLA-carrying enum cost, a retained widget never carries an unresolved color description into
the renderer, and two colors that mean the same thing compare equal, so `Color` is `Eq` and `Hash`.

The trade is that a color no longer remembers how it was written: an HSL color is stored as the RGB it resolves to, and
a named color is indistinguishable from its value once constructed.

> Everything above lives in the repository on [GitHub](https://github.com/Cottons29/aimer), and the `jaime` crate has a
> runnable example for most of it.
