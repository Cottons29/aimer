impl LayoutElement for RawTextField {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let (w, h) = self.compute_dimensions(ctx);
        let scale = ctx.scale;
        let (ol, ot, or, ob) = self.outline_strokes(w, h, scale);
        ResolvedSize {
            width: w + ol + or,
            height: h + ot + ob,
        }
    }
}

impl Drawable for RawTextField {
    fn draw(&self, ctx: &BuildContext) {
        if self.observed_revision.get() != self.controller.revision() {
            self.sync_cursor_from_controller();
        }
        ctx.canvas.save();
        // Han is unified, so the ideographs a field holds do not say whether
        // they want a Chinese or a Japanese face: `你好` is covered by both and
        // would keep whichever the system prefers until `吗` — written only in
        // Chinese — is typed, at which point the word already on screen
        // changes typeface. The field knows better, because it remembers the
        // keyboard it was typed on, and says so before it draws or measures a
        // single glyph: the caret is placed from the same advances.
        ctx.canvas
            .set_text_language(self.controller.input_language());

        let (box_width, box_height) = self.compute_dimensions(ctx);
        let scale = ctx.scale;

        // Translate inward by outline strokes so the outline has room to draw
        let (ol, ot, _or, _ob) = self.outline_strokes(box_width, box_height, scale);
        ctx.canvas.translate((ol, ot).into());

        // Cache absolute bounds for hit-testing
        let (abs_x, abs_y) = {
            let (tx, ty) = ctx.canvas.get_transform_translation();
            (tx, ty)
        };

        self.cached_bounds
            .save(scale, abs_x, abs_y, box_width, box_height);

        // A field focused at construction time — `auto_focus`, or a rebuild that
        // preserved focus — never passed through `set_focused`, so make sure
        // platform text input is on before the first composition arrives. The
        // call is idempotent, so repeating it every frame is free.
        if self.enable && self.is_focused() {
            self.enable_platform_ime();
        }

        // --- Resolve active decoration ---
        let decoration = self.active_decoration();

        // --- Draw background + border + outline ---
        decoration.draw(ctx);

        // --- Padding ---
        let pad_top = self.padding.top.value(box_height, scale);
        let pad_bottom = self.padding.bottom.value(box_height, scale);
        let pad_left = self.padding.left.value(box_width, scale);
        let pad_right = self.padding.right.value(box_width, scale);

        ctx.canvas.save();
        let radii = decoration
            .border_radius
            .resolve(box_width, box_height, scale);
        let clip_radii = [
            if radii[0] > 0.0 {
                (radii[0] - pad_left.max(pad_top).min(radii[0])).max(0.0)
            } else {
                0.0
            },
            if radii[1] > 0.0 {
                (radii[1] - pad_right.max(pad_top).min(radii[1])).max(0.0)
            } else {
                0.0
            },
            if radii[2] > 0.0 {
                (radii[2] - pad_right.max(pad_bottom).min(radii[2])).max(0.0)
            } else {
                0.0
            },
            if radii[3] > 0.0 {
                (radii[3] - pad_left.max(pad_bottom).min(radii[3])).max(0.0)
            } else {
                0.0
            },
        ];
        ctx.canvas.set_clip_rounded(
            (pad_left, pad_top).into(),
            ResolvedSize {
                width: (box_width - pad_left - pad_right).max(0.0),
                height: (box_height - pad_top - pad_bottom).max(0.0),
            },
            clip_radii,
        );
        ctx.canvas.translate((pad_left, pad_top).into());

        let content_height = (box_height - pad_top - pad_bottom).max(0.0);
        let content_width = (box_width - pad_left - pad_right).max(0.0);

        let text = self.controller.text();
        let is_empty = text.is_empty();

        let font_size = self.scaled_font_size(&self.text_style, scale);

        // --- Process pending click (deferred from on_event for canvas access) ---
        if let Some(click_pos) = self.pending_click.take() {
            let geometry = self.editable_geometry(&ctx.canvas, font_size, content_width);
            let display_for_measure = geometry.display.as_ref();
            let click_canvas_x = click_pos.x * scale;
            let (hit_text, hit_start, hit_end, text_x, scroll_x) = if self.max_lines != Some(1) {
                let metrics = ctx.canvas.measure_text_metrics("", font_size, 0.0);
                let line_height = metrics.line_height;
                let total_height = geometry.visual_lines.len() as f32 * line_height;
                let base_y = vertical_block_offset(self.text_align, content_height, total_height);
                let click_canvas_y = click_pos.y * scale;
                let hit_y = click_canvas_y - abs_y - pad_top + self.scroll_y.get() - base_y;
                let line_index = if line_height > 0.0 {
                    (hit_y.max(0.0) / line_height) as usize
                } else {
                    0
                }
                .min(geometry.visual_lines.len().saturating_sub(1));
                let line = &geometry.visual_lines[line_index];
                (
                    &display_for_measure[line.byte_start..line.byte_end],
                    line.grapheme_start,
                    line.grapheme_end,
                    self.align_x(line.width, content_width),
                    0.0,
                )
            } else {
                (
                    display_for_measure,
                    0,
                    grapheme_count(display_for_measure),
                    self.align_x(geometry.text_width, content_width),
                    self.scroll_x.get(),
                )
            };
            let rel_x = click_canvas_x - abs_x - pad_left - text_x + scroll_x;
            let mut click_offset = hit_end;
            let mut acc_width = 0.0f32;
            for (index, grapheme) in unicode_segmentation::UnicodeSegmentation::graphemes(
                hit_text, true,
            )
            .enumerate()
            {
                let grapheme_width = ctx.canvas.measure_text(grapheme, font_size);
                if rel_x <= acc_width + grapheme_width / 2.0 {
                    click_offset = hit_start + index;
                    break;
                }
                acc_width += grapheme_width;
            }

            // Apply double/triple-click selection
            let click_count = self.click_count.get();
            match click_count {
                2 => self.select_word_at(click_offset),
                3 => {
                    self.select_line_at(click_offset);
                    self.click_count.set(0);
                }
                _ => {
                    // For drag-to-select: set anchor to the click position (not the old cursor)
                    // so the selection extends from the click point to the drag destination.
                    if self.mouse_held.get().is_some() && self.cursor.selection_anchor().is_none() {
                        self.cursor.set_selection_anchor(Some(click_offset));
                    }
                    self.cursor.set_offset(click_offset);
                }
            }
            self.reveal_caret.set(true);
            self.cursor.reset_blink();
        }

        // A verb chosen from the open menu is applied here rather than in
        // `on_event`: the menu is a modal, so a tap on one of its rows is
        // handled inside the overlay and never reaches this element.
        self.apply_chosen_action();

        // A menu asked for by the last gesture is raised here, where the click
        // it was asked from has just become a caret offset and a selection —
        // so its verbs describe what the user actually pressed on.
        self.raise_pending_menu();

        // Context with parent_size set to the padded content area
        let mut content_ctx = ctx.clone();
        content_ctx.parent_size = ResolvedSize {
            width: content_width,
            height: content_height,
        };

        // Absolute canvas origin of the content area, used to translate caret
        // positions into the logical window coordinates the IME expects.
        let content_origin = (abs_x + pad_left, abs_y + pad_top);

        if is_empty {
            if self.max_lines != Some(1) {
                self.scroll_y.set(0.0);
                self.scroll_y_extent.set(0.0);
                self.reveal_caret.set(false);
            }
            // --- Draw prompt (visible when field is empty and not composing) ---
            if self.placeholder_visible() {
                if !self.prompt.is_empty() {
                    let prompt_widget =
                        self.build_text_widget(&self.prompt, &self.prompt_style, self.text_align);
                    prompt_widget.draw(&content_ctx);
                } else if !self.hint.is_empty() {
                    let hint_widget =
                        self.build_text_widget(&self.hint, &self.hint_style, self.text_align);
                    hint_widget.draw(&content_ctx);
                }
            }

            // --- Draw cursor / composition when field is empty but focused ---
            if self.is_focused() {
                let cursor_x = self.align_x(0.0, content_width);
                let line_height = ctx.canvas.measure_text_metrics("", font_size, 0.0).line_height;
                let line_y = vertical_block_offset(self.text_align, content_height, line_height);
                let (cursor_top, cursor_height) = caret_band(line_y, line_height);

                self.publish_ime_caret(
                    cursor_x,
                    cursor_top,
                    cursor_height,
                    content_origin,
                    scale,
                );

                if self.is_composing() {
                    self.with_preedit(|preedit| {
                        self.draw_preedit(
                            preedit,
                            self.preedit_cursor.get(),
                            cursor_x,
                            line_y,
                            line_height,
                            &content_ctx,
                            font_size,
                            scale,
                        );
                    });
                } else if self.cursor.is_visible() {
                    let cursor_color: Color = self.cursor.color.into();
                    let stroke_w = 1.5 * scale;

                    ctx.canvas.fill_color_rect(
                        (cursor_x, cursor_top).into(),
                        ResolvedSize {
                            width: stroke_w,
                            height: cursor_height,
                        },
                        cursor_color,
                        [0.0; 4],
                    );
                }
            }
        } else {
            // --- Draw text ---
            let geometry = self.editable_geometry(&ctx.canvas, font_size, content_width);
            let display = geometry.display.as_ref();

            let is_multiline = self.max_lines != Some(1);

            if is_multiline {
                // --- Multi-line rendering ---
                let line_metrics = ctx.canvas.measure_text_metrics("", font_size, 0.0);
                let line_height = line_metrics.line_height;
                let total_text_height = geometry.visual_lines.len() as f32 * line_height;
                let scroll_extent = vertical_scroll_extent(
                    geometry.visual_lines.len(),
                    line_height,
                    content_height,
                );
                self.scroll_y_extent.set(scroll_extent);

                let cursor_offset = self.cursor.offset();
                let cursor_line = geometry
                    .visual_lines
                    .iter()
                    .rposition(|line| {
                        cursor_offset >= line.grapheme_start
                            && cursor_offset <= line.grapheme_end
                    })
                    .unwrap_or_else(|| geometry.visual_lines.len().saturating_sub(1));
                let mut scroll = self.scroll_y.get().min(scroll_extent);
                if self.reveal_caret.replace(false) {
                    scroll = scroll_to_reveal_line(
                        scroll,
                        cursor_line,
                        line_height,
                        content_height,
                        scroll_extent,
                    );
                }
                self.scroll_y.set(scroll);

                let base_y =
                    vertical_block_offset(self.text_align, content_height, total_text_height);

                for (line_idx, visual_line) in geometry.visual_lines.iter().enumerate() {
                    let line_y = base_y + line_idx as f32 * line_height - scroll;
                    if line_y + line_height <= 0.0 || line_y >= content_height {
                        continue;
                    }
                    let line = &display[visual_line.byte_start..visual_line.byte_end];
                    let line_x = self.align_x(visual_line.width, content_width);

                    // Draw selection highlight for this line
                    if let Some((sel_start, sel_end)) = self.cursor.selection_range() {
                        let line_start = visual_line.grapheme_start;
                        let line_end = visual_line.grapheme_end;

                        if sel_start < line_end && sel_end > line_start {
                            let local_start = sel_start.max(line_start) - line_start;
                            let local_end = sel_end.min(line_end) - line_start;
                            let hl_x = line_x
                                + self.text_width_to_offset(
                                    line,
                                    local_start,
                                    &ctx.canvas,
                                    font_size,
                                );
                            let hl_end_x = line_x
                                + self.text_width_to_offset(
                                    line,
                                    local_end,
                                    &ctx.canvas,
                                    font_size,
                                );

                            ctx.canvas.fill_color_rect(
                                (hl_x, line_y).into(),
                                ResolvedSize {
                                    width: hl_end_x - hl_x,
                                    height: line_height,
                                },
                                self.selection_color,
                                [0.0; 4],
                            );
                        }
                    }

                    // Draw line text
                    ctx.canvas.save();
                    ctx.canvas.translate((0.0, line_y).into());
                    let mut line_ctx = content_ctx.clone();
                    line_ctx.parent_size = ResolvedSize {
                        width: content_width,
                        height: line_height,
                    };
                    let line_widget =
                        self.build_text_widget(line, &self.text_style, self.text_align);
                    line_widget.draw(&line_ctx);
                    ctx.canvas.restore();

                    // Draw cursor / composition if on this line
                    if self.is_focused() && line_idx == cursor_line {
                            let local_off = cursor_offset - visual_line.grapheme_start;
                            let cursor_x = line_x
                                + self.text_width_to_offset(
                                    line,
                                    local_off,
                                    &ctx.canvas,
                                    font_size,
                                );
                            let (cursor_top, cursor_height) = caret_band(line_y, line_height);

                            self.publish_ime_caret(
                                cursor_x,
                                cursor_top,
                                cursor_height,
                                content_origin,
                                scale,
                            );

                            // The composition replaces the caret: drawing both
                            // would blink an insertion bar over the first
                            // composing glyph.
                            if self.is_composing() {
                                self.with_preedit(|preedit| {
                                    self.draw_preedit(
                                        preedit,
                                        self.preedit_cursor.get(),
                                        cursor_x,
                                        line_y,
                                        line_height,
                                        &content_ctx,
                                        font_size,
                                        scale,
                                    );
                                });
                            } else if self.cursor.is_visible() {
                                let cursor_color: Color = self.cursor.color.into();
                                let stroke_w = 1.5 * scale;

                                ctx.canvas.fill_color_rect(
                                    (cursor_x, cursor_top).into(),
                                    ResolvedSize {
                                        width: stroke_w,
                                        height: cursor_height,
                                    },
                                    cursor_color,
                                    [0.0; 4],
                                );
                            }
                    }
                }
            } else {
                // --- Single-line rendering (with horizontal scroll) ---
                let text_width = geometry.text_width;
                let text_x = self.align_x(text_width, content_width);

                // The one line this field holds occupies the same band a
                // multiline field gives its first line: the text is drawn as a
                // block of one line height, aligned inside the content area.
                // Selection and caret follow that band rather than the box,
                // which is as tall as whatever the parent handed the field.
                let line_height = ctx.canvas.measure_text_metrics("", font_size, 0.0).line_height;
                let line_y = vertical_block_offset(self.text_align, content_height, line_height);

                // Ensure cursor is visible
                self.ensure_cursor_visible(content_width, &ctx.canvas, font_size, &geometry);
                let scroll = self.scroll_x.get();

                // Draw text — RawTextWidget handles alignment via text_align + parent_size.
                // Apply scroll by translating the canvas so the visible portion aligns.
                ctx.canvas.save();
                ctx.canvas.translate((-scroll, 0.0).into());
                let text_widget =
                    self.build_text_widget(display, &self.text_style, self.text_align);
                text_widget.draw(&content_ctx);
                ctx.canvas.restore();

                // --- Draw selection highlight ---
                if let Some((sel_start, sel_end)) = self.cursor.selection_range()
                    && sel_start != sel_end
                {
                    let highlight_x = text_x - scroll
                        + geometry.prefix_width(sel_start, |prefix| {
                            ctx.canvas.measure_text(prefix, font_size)
                        });
                    let highlight_end_x = text_x - scroll
                        + geometry.prefix_width(sel_end, |prefix| {
                            ctx.canvas.measure_text(prefix, font_size)
                        });
                    let highlight_width = highlight_end_x - highlight_x;

                    ctx.canvas.fill_color_rect(
                        (highlight_x, line_y).into(),
                        ResolvedSize {
                            width: highlight_width,
                            height: line_height,
                        },
                        self.selection_color,
                        [0.0; 4],
                    );
                }

                // --- Draw cursor / IME composition ---
                if self.is_focused() {
                    let cursor_x = text_x - scroll
                        + geometry.prefix_width(self.cursor.offset(), |prefix| {
                            ctx.canvas.measure_text(prefix, font_size)
                        });
                    let (cursor_top, cursor_height) = caret_band(line_y, line_height);

                    self.publish_ime_caret(
                        cursor_x,
                        cursor_top,
                        cursor_height,
                        content_origin,
                        scale,
                    );

                    // The composition replaces the caret: drawing both would
                    // blink an insertion bar over the first composing glyph.
                    if self.is_composing() {
                        self.with_preedit(|preedit| {
                            self.draw_preedit(
                                preedit,
                                self.preedit_cursor.get(),
                                cursor_x,
                                line_y,
                                line_height,
                                &content_ctx,
                                font_size,
                                scale,
                            );
                        });
                    } else if self.cursor.is_visible() {
                        let cursor_color: Color = self.cursor.color.into();
                        let stroke_w = 1.5 * scale;

                        ctx.canvas.fill_color_rect(
                            (cursor_x, cursor_top).into(),
                            ResolvedSize {
                                width: stroke_w,
                                height: cursor_height,
                            },
                            cursor_color,
                            [0.0; 4],
                        );
                    }
                }
            }
        }

        ctx.canvas.clear_clip();
        ctx.canvas.restore(); // clip + translate
        // The language belongs to this field alone, so the widgets drawn after
        // it are judged on their own characters again.
        ctx.canvas.set_text_language(None);
        ctx.canvas.restore(); // outer save

        // Drive the caret from the frame clock: advance the shared blink
        // timeline owned by the field state and keep the frame loop awake while
        // this field holds focus. Detached sleeping threads used to schedule the
        // next toggle, which drifted with thread wake-up latency and restarted
        // whenever the element was rebuilt.
        if self.is_focused() {
            self.cursor.blink().tick(AnimInstant::now());
            aimer_events::window::request_animation_frame();
        }
    }
}

/// The share of a line's height the caret covers.
///
/// A native caret spans its whole line box bar a hairline of leading, so it
/// reaches over the ascender and under the descender of the text it stands
/// in — a caret trimmed much shorter than that reads as a tick next to the
/// glyphs instead of an insertion point between them.
const CARET_LINE_FRACTION: f32 = 0.94;

/// The offset at which a text block of `block_height` is drawn inside a
/// content area of `content_height`.
///
/// Text is laid out as one block of whole lines and then aligned vertically
/// within the content area, so every part of a field that has to meet the
/// text — the caret, the selection highlight, the composition, and click hit
/// testing — starts from this offset. A block taller than the area it is
/// drawn in starts at the top and overflows downwards, which is what the
/// clipped viewport of a scrolling field expects.
#[inline]
fn vertical_block_offset(align: TextAlign, content_height: f32, block_height: f32) -> f32 {
    let spare_height = (content_height - block_height).max(0.0);
    match align {
        TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => 0.0,
        TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => spare_height / 2.0,
        TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => spare_height,
    }
}

/// The `(top, height)` of the caret drawn on a line of `line_height` placed at
/// `line_y`.
///
/// The caret covers nearly all of its line and is centered on it, so the
/// leading it gives up is split evenly above and below the text. Every caret
/// a field paints — the one on the text, the one in an empty field, and the
/// one inside a composition — is measured here, against the line and never
/// against the height of the box the field was given: a field is routinely
/// handed a box several lines tall, and a caret sized from that box spans
/// text it does not stand in.
#[inline]
fn caret_band(line_y: f32, line_height: f32) -> (f32, f32) {
    let height = line_height * CARET_LINE_FRACTION;
    (line_y + (line_height - height) / 2.0, height)
}

#[cfg(test)]
mod caret_layout_tests {
    //! Where the caret is painted inside a field.
    //!
    //! A field draws its text as a stack of lines and its caret as a bar on
    //! the line the cursor sits on, so the caret must follow that line — never
    //! the height of the box the field happens to be given. A single-line
    //! field is routinely handed the whole remaining height of a column, and a
    //! caret spanning that box is nowhere near its text.

    use aimer_style::TextAlign;
    use aimer_widget::Drawable;

    use super::test_support::{
        dummy_build_context, focused_multiline_field, focused_single_line_field,
    };
    use crate::input_field::controller::TextFieldController;

    /// Asserts that `height` reads as a caret standing on a `line` tall line:
    /// nearly the whole line, and never taller than it.
    #[track_caller]
    fn assert_line_tall(height: f32, line: f32) {
        assert!(
            height >= line * 0.85 && height <= line,
            "caret is {height} tall, expected nearly the whole {line} tall line",
        );
    }

    #[test]
    fn an_empty_multiline_caret_is_one_line_tall() {
        let field = focused_multiline_field(TextFieldController::new(), 3);
        let ctx = dummy_build_context(400.0, 200.0);
        let line = line_height(&field, &ctx);

        field.draw(&ctx);

        let (_, height) = caret_band(&field);
        assert_line_tall(height, line);
    }

    #[test]
    fn a_multiline_caret_is_one_line_tall_with_text() {
        let field = focused_multiline_field(TextFieldController::with_initial("hello"), 3);
        let ctx = dummy_build_context(400.0, 200.0);
        let line = line_height(&field, &ctx);

        field.draw(&ctx);

        let (_, height) = caret_band(&field);
        assert_line_tall(height, line);
    }

    /// The height of one line of the field's text, as the field measures it.
    fn line_height(field: &super::RawTextField, ctx: &aimer_widget::base::BuildContext) -> f32 {
        let font_size = field.scaled_font_size(&field.text_style, ctx.scale);
        ctx.canvas.measure_text_metrics("", font_size, 0.0).line_height
    }

    /// The vertical band the caret was painted in, as published to the input
    /// method — the same rectangle the field fills on the canvas.
    fn caret_band(field: &super::RawTextField) -> (f32, f32) {
        let caret = field
            .ime_cursor_area
            .get()
            .expect("a focused field publishes the caret it painted");
        (caret.y, caret.height)
    }

    #[test]
    fn a_single_line_caret_is_one_line_tall_in_a_tall_box() {
        let field = focused_single_line_field(TextFieldController::with_initial("hello"));
        let ctx = dummy_build_context(400.0, 600.0);
        let line = line_height(&field, &ctx);

        field.draw(&ctx);

        let (_, height) = caret_band(&field);
        assert_line_tall(height, line);
    }

    #[test]
    fn a_single_line_caret_sits_on_its_text_line() {
        let field = focused_single_line_field(TextFieldController::with_initial("hello"));
        let ctx = dummy_build_context(400.0, 600.0);
        let line = line_height(&field, &ctx);

        field.draw(&ctx);

        // Text is top aligned by default, so the caret belongs to the first
        // line of the content area rather than to the middle of the box.
        let (top, _) = caret_band(&field);
        assert!(
            top < line,
            "caret starts at {top}, past the first {line}-tall line of text",
        );
    }

    #[test]
    fn an_empty_single_line_caret_sits_on_the_first_line() {
        let field = focused_single_line_field(TextFieldController::new());
        let ctx = dummy_build_context(400.0, 600.0);
        let line = line_height(&field, &ctx);

        field.draw(&ctx);

        let (top, height) = caret_band(&field);
        assert!(top < line, "caret of an empty field starts at {top}");
        assert_line_tall(height, line);
    }

    #[test]
    fn a_vertically_centered_single_line_caret_follows_its_text() {
        let mut field = focused_single_line_field(TextFieldController::with_initial("hello"));
        field.text_align = TextAlign::MidLeft;
        let ctx = dummy_build_context(400.0, 600.0);
        let line = line_height(&field, &ctx);

        field.draw(&ctx);

        // The text is centered in the content area, and the caret centers with
        // it instead of stretching across the box.
        let (top, height) = caret_band(&field);
        let center = top + height / 2.0;
        assert!(
            (center - 300.0).abs() <= line,
            "caret centered at {center}, expected the middle of a 600 tall box",
        );
        assert_line_tall(height, line);
    }
}
