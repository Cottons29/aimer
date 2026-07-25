# Analytic AA by Default and a Default macOS Menu

Cupid now uses `AntiAlias::Analytic` as Aimer's default anti-aliasing method. It builds on the
coverage techniques already used by Aimer's common UI primitives and renders directly to the window
surface without allocating a multisampled texture for the entire window.

Aimer applications on macOS also receive a standard global menu bar by default. Both changes move
platform and rendering policy into the framework while preserving an explicit configuration path for
applications that need something different.

> Previously, **Aimer** is using MSAA4X method for AA the widgets. It use too much memory each frame and waste on UI framework.  

## How Analytic AA Works

Many UI edges do not need hardware multisampling to look smooth. Cupid's rectangle shader evaluates
signed distances for rounded corners, borders, outlines, shadows, and rounded clips. A narrow coverage
ramp around each boundary turns that distance into a partially transparent edge pixel.

The same principle applies throughout the built-in pipelines:

- images combine filtered texture alpha with analytic rectangular or rounded clipping;
- monochrome and color text use glyph-atlas coverage with analytic clipping;
- text decorations calculate procedural stroke coverage; and
- SVG clips use analytic coverage.

In `Analytic` mode, Cupid renders these pipelines directly into the one-sample surface texture. There
is no second full-window color target and no resolve operation at the end of the render pass.

Analytic AA is focused on the interface geometry Aimer renders most often. Tessellated SVG fill and
stroke boundaries and user-defined triangle geometry do not automatically gain shader-based edge
coverage, so applications should still inspect custom rendering on their supported devices.

## Analytic AA by Default

Applications now receive Analytic AA without any renderer configuration:

```rust
AimerApp::start(app);
```

The builder-based startup path uses the same default:

```rust
AimerApp::new()
    .child(app)
    .run();
```

This keeps the common path configuration-free and gives Aimer applications a consistent rendering
baseline across native and web targets. Because the renderer does not create a full-window
multisample target in this mode, it also avoids that target's allocation and resolve work. Actual
frame-time results still depend on scene composition, resolution, GPU, graphics backend, and driver.

> Enable the MSAA by this : 
> ```rust
> AimerApp::new()
>   .with_antialiasing(AntiAlias::Msaa4x) // Or using Msaa2x
>   
> ```
> 

## A Native Menu Bar by Default on macOS

macOS expects applications to participate in its global menu bar. Aimer now installs one as framework
startup work, before creating the first window, instead of requiring every application to repeat the
same platform-specific setup.

The default menu contains the familiar application, File, Edit, View, Window, and Help sections. It
includes native actions such as About, Services, Hide, Quit, Close Window, Undo, Redo, Cut, Copy,
Paste, Select All, Full Screen, Minimize, and Maximize. The menu resource remains alive until the
application exits.

This behavior is automatic for `AimerApp::start(app)` and builder-based applications on macOS. Other
platforms do not install a macOS menu and require no conditional application code.

## Ordered Startup Setup

The menu is built on Aimer's startup-hook mechanism. An application can add more setup callbacks with
`setup`; they run once in registration order after the framework's platform setup and before the first
window is created. Each callback's returned resource is retained until shutdown.

```rust
AimerApp::new()
    .setup(create_first_native_resource)
    .setup(create_second_native_resource)
    .child(app)
    .run();
```

Keeping the framework menu and application hooks in one ordered lifecycle makes native initialization
predictable while keeping the common path as small as `AimerApp::start(app)`.