# TextField Production Improvements

## Phase 1 — Critical Fixes & Features ✅

1. Fix `get_window().unwrap()` panic ✅
2. `UnsafeCell<bool>` → `Cell<bool>` ✅
3. `max_length` enforcement ✅
4. Undo/redo (Ctrl+Z/Y) ✅
5. `on_focus` / `on_blur` callbacks ✅
6. `read_only` field ✅
7. Unit tests (25) ✅

## Phase 2 — Advanced Interaction & Multi-line ✅

### 8. Double-click word selection / triple-click line selection ✅
- Track click timing via `last_click_time: Cell<AnimInstant>` + `click_count: Cell<u8>`
- Clicks within 500ms increment count; resets after timeout
- Double-click (count=2): `select_word_at()` using `unicode_segmentation::split_word_bound_indices()`
- Triple-click (count=3): `select_line_at()` selecting between `\n` boundaries
- Click position resolved in `draw()` via `pending_click: Cell<Option<Vec2d>>` for canvas text measurement access

### 9. Drag-to-select ✅
- `mouse_held: Cell<bool>` set on PointerDown, cleared on PointerUp
- PointerMove while held sets `pending_click` for deferred resolution
- Selection anchor set on first drag move if none exists
- Cursor icon shows Text during drag

### 10. Horizontal scroll for overflow text ✅
- `scroll_x: Cell<f32>` tracks horizontal offset
- `ensure_cursor_visible()` adjusts scroll after cursor movement
- Text rendered with `-scroll_x` offset in single-line mode
- Scroll resets on content change

### 11. IME pre-edit composition rendering ✅
- New `ElementEvent::ImePreedit { text, cursor }` variant in `element.rs`
- `event_handler.rs` forwards `Ime::Preedit` as `ImePreedit` event
- `preedit_text: Cell<String>` + `preedit_cursor: Cell<Option<(usize, usize)>>` on RawTextField
- Preedit text rendered at cursor position with underline in `draw()`
- Cleared on new click, Cancel, or empty preedit

### 12. max_lines / min_lines enforcement ✅
- `line_count()` counts `\n` chars + 1
- When `max_lines > 1`: Enter inserts `\n`, Ctrl+Enter submits
- ArrowUp/ArrowDown navigate between lines (column-aware)
- Multi-line rendering: text split by `\n`, each line rendered separately with `line_height = font_size * 1.4`
- Selection highlight and cursor work across lines

### 13. Undo stack size cap ✅
- `MAX_UNDO_DEPTH = 200` constant on TextFieldController
- `save_undo()` truncates oldest entry when stack exceeds cap

## Files Changed

| File | Changes |
|---|---|
| `crates/aimer_input/src/input_field/controller.rs` | Undo/redo + cap + 25 unit tests |
| `crates/aimer_input/src/input_field/raw_fields.rs` | All interaction features, multi-line, scroll, preedit |
| `crates/aimer_input/src/input_field/text_field.rs` | New field pass-throughs |
| `crates/aimer_events/src/element.rs` | New `ImePreedit` variant |
| `aimer_quiver/src/handler/event_handler.rs` | Forward Ime::Preedit |
| `crates/aimer_widget/src/widget/stateful.rs` | Handle ImePreedit in match |
| `crates/aimer_container/src/scrollable/handle_scroll.rs` | Handle ImePreedit in match |

original (width = 19)
|apple apple apple|

wrapping (width = 15)
|apple apple  |
|ple          |

expected render (width = 15)
|apple apple  |
|apple        |