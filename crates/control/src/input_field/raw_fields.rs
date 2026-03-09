use animation::AnimInstant;
use attribute::dimension::Dimension;
use attribute::size::{ResolvedSize, Size};
use std::cell::UnsafeCell;
use widget::base::{BuildContext, Color, Colors};
use widget::style::BoxConstraint;
use widget::style::border::BoxBorder;
use widget::text::{FontWeight, TextAlign};
use widget::{Drawable, Element, ElementEvent, KeyAction, NamedKey, TextStyle};

#[cfg(not(target_arch = "wasm32"))]
use skia_safe::{
    Color as SkColor, Font, FontMgr, Paint, Rect, TextBlob, font_style::FontStyle as SkFontStyle, paint::Style,
};
use utils::debug;

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static FONT_MGR: FontMgr = FontMgr::new();
}

#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
#[cfg(target_arch = "wasm32")]
type Float = f64;

pub struct TextFieldController {
    text: UnsafeCell<String>,
}

impl TextFieldController {
    pub fn new(text: String) -> Self {
        Self { text: UnsafeCell::new(text) }
    }

    pub fn text(&self) -> &str {
        unsafe { &*self.text.get() }
    }

    fn text_mut(&self) -> &mut String {
        unsafe { &mut *self.text.get() }
    }

    pub fn set_text(&self, text: String) {
        *self.text_mut() = text;
    }

    pub fn insert_char(&self, ch: char, offset: usize) {
        let s = self.text_mut();
        let byte_offset = s
            .char_indices()
            .nth(offset)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        s.insert(byte_offset, ch);
    }

    pub fn delete_char(&self, offset: usize) {
        let s = self.text_mut();
        if let Some((byte_offset, _ch)) = s.char_indices().nth(offset) {
            s.remove(byte_offset);
        }
    }

    pub fn char_count(&self) -> usize {
        self.text().chars().count()
    }
}

pub enum InputType {
    Text,
    Number,
    Obscure,
}

impl Default for InputType {
    fn default() -> Self {
        Self::Text
    }
}

pub struct Cursor {
    cursor: String,
    offset: UnsafeCell<usize>,
    visible: UnsafeCell<bool>,
    blink_rate_ms: u64,
    last_blink: UnsafeCell<AnimInstant>,
    radius: Option<f32>,
    color: Colors,
}

impl Cursor {
    pub fn new(color: Colors) -> Self {
        Self {
            cursor: "|".to_string(),
            offset: UnsafeCell::new(0),
            visible: UnsafeCell::new(true),
            blink_rate_ms: 500,
            last_blink: UnsafeCell::new(AnimInstant::now()),
            radius: None,
            color,
        }
    }

    pub fn offset(&self) -> usize {
        unsafe { *self.offset.get() }
    }

    pub fn set_offset(&self, offset: usize) {
        unsafe {
            *self.offset.get() = offset;
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { *self.visible.get() }
    }

    fn set_visible(&self, v: bool) {
        unsafe { *self.visible.get() = v; }
    }

    /// Toggle visibility if enough time has elapsed. Returns true if toggled.
    fn update_blink(&self) -> bool {
        let now = AnimInstant::now();
        let last = unsafe { *self.last_blink.get() };
        if now.duration_since(last).as_millis() as u64 >= self.blink_rate_ms {
            unsafe { *self.last_blink.get() = now; }
            let vis = self.is_visible();
            self.set_visible(!vis);
            true
        } else {
            false
        }
    }

    /// Reset cursor to visible and restart blink timer.
    fn reset_blink(&self) {
        self.set_visible(true);
        unsafe { *self.last_blink.get() = AnimInstant::now(); }
    }
}

pub enum ExpandDirection {
    Horizontal,
    Vertical,
    Both,
    None,
}

impl Default for ExpandDirection {
    fn default() -> Self {
        Self::None
    }
}

pub struct TextFieldStyle {
    pub background_color: Colors,
    pub border: BoxBorder,
    pub padding: Dimension,
}

impl Default for TextFieldStyle {
    fn default() -> Self {
        Self { background_color: Colors::White, border: BoxBorder::default(), padding: Dimension::Px(4.0) }
    }
}

pub(crate) struct RawTextField {
    pub input_type: InputType,
    pub controller: TextFieldController,
    pub prompt: String,
    pub hint: String,
    pub hint_style: TextStyle,
    pub text_style: TextStyle,
    pub prompt_style: TextStyle,
    pub text_align: TextAlign,
    pub auto_focus: bool,
    pub max_lines: Option<usize>,
    pub min_lines: Option<usize>,
    pub max_length: Option<usize>,
    pub enable: bool,
    pub expand: ExpandDirection,
    pub box_height: Dimension,
    pub box_width: Dimension,
    pub box_constraint: BoxConstraint,
    pub cursor: Cursor,
    pub style: TextFieldStyle,
    pub focused: UnsafeCell<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    pub cached_bounds: UnsafeCell<Option<Rect>>,
    #[cfg(target_arch = "wasm32")]
    pub cached_bounds: UnsafeCell<Option<(f64, f64, f64, f64)>>,
}

impl RawTextField {
    #[cfg(not(target_arch = "wasm32"))]
    fn make_font(&self, style: &TextStyle, scale: Float) -> Font {
        let weight = match style.font_weight {
            FontWeight::VeryThin => skia_safe::font_style::Weight::EXTRA_LIGHT,
            FontWeight::Thin => skia_safe::font_style::Weight::THIN,
            FontWeight::Normal => skia_safe::font_style::Weight::NORMAL,
            FontWeight::Bold => skia_safe::font_style::Weight::BOLD,
            FontWeight::Bolder => skia_safe::font_style::Weight::EXTRA_BOLD,
            FontWeight::Value(v) => skia_safe::font_style::Weight::from(v as i32),
        };

        let slant = match style.font_style {
            widget::text::FontStyle::Normal => skia_safe::font_style::Slant::Upright,
            widget::text::FontStyle::Italic => skia_safe::font_style::Slant::Italic,
            widget::text::FontStyle::Oblique | widget::text::FontStyle::ObliqueDeg(_) => {
                skia_safe::font_style::Slant::Oblique
            }
        };

        let sk_font_style = SkFontStyle::new(weight, skia_safe::font_style::Width::NORMAL, slant);
        let font_size = if style.font_size == 0 { 14.0 } else { style.font_size as Float };
        let scaled_size = font_size * scale;

        let typeface = FONT_MGR.with(|mgr| {
            mgr.match_family_style("Arial", sk_font_style)
                .or_else(|| mgr.match_family_style("Helvetica", sk_font_style))
                .or_else(|| mgr.match_family_style("", sk_font_style))
                .expect("Unable to load any typeface")
        });

        Font::new(typeface, scaled_size)
    }

    fn is_focused(&self) -> bool {
        unsafe { *self.focused.get() }
    }

    fn set_focused(&self, focused: bool) {
        unsafe {
            *self.focused.get() = focused;
        }
    }

    fn compute_dimensions(&self, ctx: &BuildContext) -> (Float, Float) {
        let scale = ctx.scale;
        let constraint = ctx.box_constraint;

        let width = match self.box_width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => constraint.max_width * (p / 100.0),
            Dimension::Auto => constraint.max_width,
        };

        let height = match self.box_height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => constraint.max_height * (p / 100.0),
            Dimension::Auto => {
                let font_size = if self.text_style.font_size == 0 { 14 } else { self.text_style.font_size };
                (font_size as Float + 8.0) * scale
            }
        };

        (width.max(0.0), height.max(0.0))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn cursor_x_offset(&self, font: &Font) -> Float {
        let text = self.controller.text();
        let offset = self.cursor.offset();
        let prefix: String = text.chars().take(offset).collect();
        let (w, _) = font.measure_text(&prefix, None);
        w
    }

    #[cfg(target_arch = "wasm32")]
    fn cursor_x_offset_wasm(&self, canvas: &web_sys::CanvasRenderingContext2d, font_str: &str) -> Float {
        let text = self.controller.text();
        let offset = self.cursor.offset();
        let prefix: String = text.chars().take(offset).collect();
        canvas.set_font(font_str);
        match canvas.measure_text(&prefix) {
            Ok(metrics) => metrics.width(),
            Err(_) => 0.0,
        }
    }
}

impl Drawable for RawTextField {
    #[cfg(not(target_arch = "wasm32"))]
    fn draw(&self, ctx: &BuildContext) {
        ctx.canvas.save();

        let (box_width, box_height) = self.compute_dimensions(ctx);
        let scale = ctx.scale;

        // Cache absolute bounds for hit-testing
        let matrix = ctx.canvas.local_to_device_as_3x3();
        let abs_x = matrix.translate_x();
        let abs_y = matrix.translate_y();
        unsafe {
            *self.cached_bounds.get() = Some(Rect::from_xywh(abs_x, abs_y, box_width, box_height));
        }

        // --- Draw background ---
        let mut bg_paint = Paint::default();
        bg_paint.set_anti_alias(true);
        bg_paint.set_color(SkColor::from(Color::from(self.style.background_color)));
        bg_paint.set_style(Style::Fill);

        let rect = Rect::from_xywh(0.0, 0.0, box_width, box_height);
        ctx.canvas.draw_rect(rect, &bg_paint);

        // --- Draw border ---
        self.style
            .border
            .draw(ctx.canvas, box_width, box_height, scale);

        // --- Padding ---
        let padding = match self.style.padding {
            Dimension::Px(p) => p * scale,
            Dimension::Percent(p) => box_width * (p / 100.0),
            Dimension::Auto => 4.0 * scale,
        };

        ctx.canvas.save();
        ctx.canvas.clip_rect(
            Rect::from_xywh(padding, 0.0, (box_width - padding * 2.0).max(0.0), box_height),
            None,
            false,
        );
        ctx.canvas.translate((padding, 0.0));

        let content_height = box_height;

        let text = self.controller.text();
        let is_empty = text.is_empty();

        let font = self.make_font(&self.text_style, scale);
        let (_, metrics) = font.metrics();
        let text_y = content_height / 2.0 - (metrics.ascent + metrics.descent) / 2.0;

        if is_empty {
            // --- Draw prompt (visible when field is empty) ---
            if !self.prompt.is_empty() {
                let prompt_font = self.make_font(&self.prompt_style, scale);
                let mut prompt_paint = Paint::default();
                prompt_paint.set_anti_alias(true);
                prompt_paint.set_color(SkColor::from(self.prompt_style.color));

                if let Some(blob) = TextBlob::new(&self.prompt, &prompt_font) {
                    ctx.canvas
                        .draw_text_blob(&blob, (0.0, text_y), &prompt_paint);
                }
            }

            // --- Draw hint text (visible when field is empty and no prompt) ---
            if self.prompt.is_empty() && !self.hint.is_empty() {
                let hint_font = self.make_font(&self.hint_style, scale);
                let mut hint_paint = Paint::default();
                hint_paint.set_anti_alias(true);
                hint_paint.set_color(SkColor::from(self.hint_style.color));

                if let Some(blob) = TextBlob::new(&self.hint, &hint_font) {
                    ctx.canvas.draw_text_blob(&blob, (0.0, text_y), &hint_paint);
                }
            }
        } else {
            // --- Draw text (no prompt when there is input) ---
            let text_x = 0.0_f32;

            let display = match self.input_type {
                InputType::Obscure => "\u{2022}".repeat(self.controller.char_count()),
                _ => text.to_string(),
            };

            if !display.is_empty() {
                let mut text_paint = Paint::default();
                text_paint.set_anti_alias(true);
                text_paint.set_color(SkColor::from(self.text_style.color));

                if let Some(blob) = TextBlob::new(&display, &font) {
                    ctx.canvas
                        .draw_text_blob(&blob, (text_x, text_y), &text_paint);
                }
            }

            // --- Draw cursor ---
            if self.is_focused() && self.cursor.is_visible() {
                let cursor_x = text_x + self.cursor_x_offset(&font);
                let cursor_top = content_height * 0.15;
                let cursor_bottom = content_height * 0.85;

                let mut cursor_paint = Paint::default();
                cursor_paint.set_anti_alias(true);
                cursor_paint.set_color(SkColor::from(Color::from(self.cursor.color)));
                cursor_paint.set_style(Style::Stroke);
                cursor_paint.set_stroke_width(1.5 * scale);

                ctx.canvas
                    .draw_line((cursor_x, cursor_top), (cursor_x, cursor_bottom), &cursor_paint);
            }
        }

        // --- Draw cursor when field is empty but focused ---
        if is_empty && self.is_focused() && self.cursor.is_visible() {
            let cursor_x = 0.0_f32;
            let cursor_top = content_height * 0.15;
            let cursor_bottom = content_height * 0.85;

            let mut cursor_paint = Paint::default();
            cursor_paint.set_anti_alias(true);
            cursor_paint.set_color(SkColor::from(Color::from(self.cursor.color)));
            cursor_paint.set_style(Style::Stroke);
            cursor_paint.set_stroke_width(1.5 * scale);

            ctx.canvas
                .draw_line((cursor_x, cursor_top), (cursor_x, cursor_bottom), &cursor_paint);
        }

        ctx.canvas.restore(); // clip + translate
        ctx.canvas.restore(); // outer save

        // Drive cursor blink: toggle visibility and schedule next redraw while focused
        if self.is_focused() {
            self.cursor.update_blink();
            ctx.window.request_redraw();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn draw(&self, ctx: &BuildContext) {
        use color::prelude::ColorMixer;

        let canvas = &ctx.canvas;
        let (box_width, box_height) = self.compute_dimensions(ctx);
        let scale = ctx.scale;

        // Cache absolute bounds for hit-testing
        // On WASM the canvas transform gives us the absolute position
        let transform = canvas.get_transform().unwrap_or_else(|_| web_sys::DomMatrix::new().unwrap());
        let abs_x = transform.e();
        let abs_y = transform.f();
        unsafe {
            *self.cached_bounds.get() = Some((abs_x, abs_y, abs_x + box_width, abs_y + box_height));
        }

        // --- Draw background ---
        let bg_color: Color = self.style.background_color.into();
        canvas.set_fill_style_str(&bg_color.to_css_color());
        canvas.fill_rect(0.0, 0.0, box_width, box_height);

        // --- Draw border ---
        self.style.border.draw(canvas, box_width, box_height, scale);

        // --- Padding ---
        let padding = match self.style.padding {
            Dimension::Px(p) => p * scale,
            Dimension::Percent(p) => box_width * (p / 100.0),
            Dimension::Auto => 4.0 * scale,
        };

        canvas.save();
        canvas.begin_path();
        canvas.rect(padding, 0.0, (box_width - padding * 2.0).max(0.0), box_height);
        canvas.clip();
        let _ = canvas.translate(padding, 0.0);

        let content_height = box_height;

        let text = self.controller.text();
        let is_empty = text.is_empty();

        let get_css_font = |style: &TextStyle| -> String {
            let fs = if style.font_size == 0 { 14.0 } else { style.font_size as Float };
            let sfs = fs * scale;
            let weight = match style.font_weight {
                FontWeight::VeryThin => "100",
                FontWeight::Thin => "300",
                FontWeight::Normal => "normal",
                FontWeight::Bold => "bold",
                FontWeight::Bolder => "900",
                FontWeight::Value(_) => "normal",
            };
            let font_style = match style.font_style {
                widget::text::FontStyle::Normal => "normal",
                widget::text::FontStyle::Italic => "italic",
                widget::text::FontStyle::Oblique | widget::text::FontStyle::ObliqueDeg(_) => "oblique",
            };
            format!("{} {} {}px Arial, sans-serif", font_style, weight, sfs)
        };

        let text_font = get_css_font(&self.text_style);
        canvas.set_font(&text_font);
        canvas.set_text_baseline("middle");
        canvas.set_text_align("left");

        let text_y = content_height / 2.0;

        if is_empty {
            // --- Draw prompt ---
            if !self.prompt.is_empty() {
                let prompt_font = get_css_font(&self.prompt_style);
                canvas.set_font(&prompt_font);
                let argb = self.prompt_style.color.to_u32();
                let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
                let r = (argb >> 16) & 0xFF;
                let g = (argb >> 8) & 0xFF;
                let b = argb & 0xFF;
                canvas.set_fill_style_str(&format!("rgba({}, {}, {}, {})", r, g, b, a));
                let _ = canvas.fill_text(&self.prompt, 0.0, text_y);
            }

            // --- Draw hint ---
            if self.prompt.is_empty() && !self.hint.is_empty() {
                let hint_font = get_css_font(&self.hint_style);
                canvas.set_font(&hint_font);
                let argb = self.hint_style.color.to_u32();
                let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
                let r = (argb >> 16) & 0xFF;
                let g = (argb >> 8) & 0xFF;
                let b = argb & 0xFF;
                canvas.set_fill_style_str(&format!("rgba({}, {}, {}, {})", r, g, b, a));
                let _ = canvas.fill_text(&self.hint, 0.0, text_y);
            }
        } else {
            // --- Draw text ---
            let text_x = 0.0;

            let display = match self.input_type {
                InputType::Obscure => "\u{2022}".repeat(self.controller.char_count()),
                _ => text.to_string(),
            };

            if !display.is_empty() {
                let argb = self.text_style.color.to_u32();
                let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
                let r = (argb >> 16) & 0xFF;
                let g = (argb >> 8) & 0xFF;
                let b = argb & 0xFF;
                canvas.set_fill_style_str(&format!("rgba({}, {}, {}, {})", r, g, b, a));
                let _ = canvas.fill_text(&display, text_x, text_y);
            }

            // --- Draw cursor ---
            if self.is_focused() && self.cursor.is_visible() {
                let cursor_x = text_x + self.cursor_x_offset_wasm(canvas, &text_font);
                let cursor_top = content_height * 0.15;
                let cursor_bottom = content_height * 0.85;

                let cursor_color: Color = self.cursor.color.into();
                canvas.set_stroke_style_str(&cursor_color.to_css_color());
                canvas.set_line_width(1.5 * scale);
                canvas.begin_path();
                canvas.move_to(cursor_x, cursor_top);
                canvas.line_to(cursor_x, cursor_bottom);
                canvas.stroke();
            }
        }

        // --- Draw cursor when empty but focused ---
        if is_empty && self.is_focused() && self.cursor.is_visible() {
            let cursor_x = 0.0;
            let cursor_top = content_height * 0.15;
            let cursor_bottom = content_height * 0.85;

            let cursor_color: Color = self.cursor.color.into();
            canvas.set_stroke_style_str(&cursor_color.to_css_color());
            canvas.set_line_width(1.5 * scale);
            canvas.begin_path();
            canvas.move_to(cursor_x, cursor_top);
            canvas.line_to(cursor_x, cursor_bottom);
            canvas.stroke();
        }

        canvas.restore();

        // Drive cursor blink
        if self.is_focused() {
            self.cursor.update_blink();
            ctx.window.request_redraw();
        }
    }
}

impl Element for RawTextField {
    fn size(&self) -> Option<Size> {
        Some(Size { width: self.box_width, height: self.box_height })
    }

    fn on_event(&self, event: &ElementEvent) -> bool {
        if !self.enable {
            return false;
        }

        match event {
            ElementEvent::PointerDown(pos) => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let is_inside = unsafe {
                        if let Some(bounds) = *self.cached_bounds.get() {
                            pos.x >= bounds.left
                                && pos.x <= bounds.right
                                && pos.y >= bounds.top
                                && pos.y <= bounds.bottom
                        } else {
                            false
                        }
                    };

                    return if is_inside {
                        self.set_focused(true);
                        self.cursor.set_offset(self.controller.char_count());
                        self.cursor.reset_blink();
                        true
                    } else {
                        self.set_focused(false);
                        false
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let is_inside = unsafe {
                        if let Some((left, top, right, bottom)) = *self.cached_bounds.get() {
                            pos.x >= left
                                && pos.x <= right
                                && pos.y >= top
                                && pos.y <= bottom
                        } else {
                            false
                        }
                    };

                    if is_inside {
                        self.set_focused(true);
                        self.cursor.set_offset(self.controller.char_count());
                        self.cursor.reset_blink();
                        return true;
                    } else {
                        self.set_focused(false);
                        return false;
                    }
                }
            }
            ElementEvent::CharInput(ch) => {
                if !self.is_focused() {
                    return false;
                }

                let offset = self.cursor.offset();
                self.controller.insert_char(*ch, offset);
                self.cursor.set_offset(offset + 1);
                self.cursor.reset_blink();
                true
            }
            ElementEvent::KeyInput { key, action } => {
                if !self.is_focused() {
                    return false;
                }
                if *action != KeyAction::Pressed {
                    return false;
                }
                let result = match key {
                    NamedKey::Backspace => {
                        let offset = self.cursor.offset();
                        if offset > 0 {
                            self.controller.delete_char(offset - 1);
                            self.cursor.set_offset(offset - 1);
                        }
                        true
                    }
                    NamedKey::Delete => {
                        let offset = self.cursor.offset();
                        if offset < self.controller.char_count() {
                            self.controller.delete_char(offset);
                        }
                        true
                    }
                    NamedKey::ArrowLeft => {
                        let offset = self.cursor.offset();
                        if offset > 0 {
                            self.cursor.set_offset(offset - 1);
                        }
                        true
                    }
                    NamedKey::ArrowRight => {
                        let offset = self.cursor.offset();
                        if offset < self.controller.char_count() {
                            self.cursor.set_offset(offset + 1);
                        }
                        true
                    }
                    NamedKey::Home => {
                        self.cursor.set_offset(0);
                        true
                    }
                    NamedKey::End => {
                        self.cursor.set_offset(self.controller.char_count());
                        true
                    }
                    NamedKey::Escape => {
                        self.set_focused(false);
                        true
                    }
                    _ => false,
                };
                if result {
                    self.cursor.reset_blink();
                }
                result
            }
            ElementEvent::Cancel => {
                self.set_focused(false);
                true
            }
            _ => false,
        }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let (w, h) = self.compute_dimensions(ctx);
        ResolvedSize { width: w, height: h }
    }
}
