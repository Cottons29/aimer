# Aimer Inspector

The **Aimer Inspector** is a built-in debugging tool that lets you visualize and explore your application's widget tree in real time. It works across all supported platforms — native (macOS, iOS, Android) and Web (WASM).

When enabled, the inspector:
- Draws a **visual overlay** on your running app, highlighting hovered widgets with their name and dimensions.
- Streams the full **widget tree** to the CLI console, where you can browse it as an indented tree view.

---

## How It Works

Under the hood, the inspector uses a **WebSocket** connection (default port `9229`) between the running application and the Aimer CLI console.

| Platform | Architecture |
|----------|-------------|
| Native (macOS, iOS, Android) | The **engine** hosts a WebSocket server. The CLI connects as a client. |
| Web (WASM) | The **CLI** hosts a WebSocket server. The browser app connects as a client. |

When active, the engine serializes the widget tree after each frame and broadcasts the JSON snapshot to every connected client. The tree includes each widget's name, position (`x`, `y`), and size (`width`, `height`).

---

## Toggling the Inspector

Press **F12** in the Aimer CLI console to toggle the inspector on or off. This:

1. Switches the console view to the **Inspector** pane.
2. Enables (or disables) the visual overlay on the running application.
3. Starts (or stops) broadcasting widget tree snapshots.

You can also cycle between the **App Logs**, **Build Logs**, and **Inspector** panes using the **Tab** key.

---

## The CLI Console

When you run your app with `aimer run`, the CLI opens an interactive TUI (terminal UI) console with three panes:

| Pane           | Description                                    |
|----------------|------------------------------------------------|
| **App Logs**   | Standard output from your running application. |
| **Build Logs** | Compilation and build output.                  |
| **Inspector**  | Live widget tree view (when enabled).          |

### Console Controls

| Key | Action |
|-----|--------|
| `Tab` | Switch between panes |
| `F12` | Toggle inspector on/off |
| `r` | Hot reload / rebuild |
| `c` | Copy current pane logs to clipboard |
| `↑` / `↓` | Scroll up/down |
| `PageUp` / `PageDown` | Scroll by 10 lines |
| `Shift+Q` | Exit |

---

## Inspector Overlay

When the inspector is enabled, a **visual overlay** is drawn on top of your application. As you hover over widgets, the overlay highlights the hovered widget with:

- A **blue border** around the widget's bounding box.
- A **translucent blue fill** over the widget area.
- A **label** above the widget showing its name and dimensions (e.g., `Container 200.0×100.0`).

This makes it easy to identify widget boundaries, debug layout issues, and understand how your widget tree maps to the visual output.

---

## Widget Tree View

In the Inspector pane of the CLI console, the widget tree is displayed as an indented text hierarchy. Each node shows:

- The **widget name** (e.g., `Container`, `Row`, `Text`)
- The **size and position** in the format `[width×height @ (x,y)]`

Example output:

```
▸ App [800×600 @ (0,0)]
  ▸ Column [800×600 @ (0,0)]
    ▸ Container [200×50 @ (0,0)]
      ▸ Text [180×20 @ (10,15)]
    ▸ Button [120×40 @ (0,50)]
      ▸ Text [100×20 @ (10,60)]
```

---

## Inspector States

The Inspector pane in the CLI shows different messages depending on the current connection state:

| State | Message |
|-------|---------|
| App not running | `Waiting for app to start...` |
| Connected, inspector off | `Inspector is OFF. Press F12 to enable.` |
| Connected, inspector on | Live widget tree |
| No tree data yet | `No widget tree received yet.` |

