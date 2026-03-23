use super::glyph_rasterizer::GlyphRasterizer;

/// A positioned glyph ready for rendering.
pub struct PositionedGlyph {
    pub codepoint: char,
    /// Screen-space X of the glyph quad's top-left corner.
    pub x: f32,
    /// Screen-space Y of the glyph quad's top-left corner.
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub font_size: f32,
}

/// Simple horizontal text layout with basic line breaking.
pub fn layout_text(
    rasterizer: &mut GlyphRasterizer,
    text: &str,
    font_size: f32,
    origin_x: f32,
    origin_y: f32,
    max_width: f32,
) -> Vec<PositionedGlyph> {
    let (ascent, _descent, line_gap) = rasterizer.line_metrics(font_size);
    let line_height = ascent - _descent + line_gap;

    let mut glyphs = Vec::new();
    let mut pen_x = origin_x;
    let mut pen_y = origin_y;

    for c in text.chars() {
        if c == '\n' {
            pen_x = origin_x;
            pen_y += line_height;
            continue;
        }

        let rg = rasterizer.rasterize(c, font_size);

        // Simple word-wrap: if this glyph would exceed max_width, wrap.
        if max_width > 0.0 && pen_x + rg.advance_width > origin_x + max_width && pen_x > origin_x {
            pen_x = origin_x;
            pen_y += line_height;
        }

        let advance = rg.advance_width;

        if rg.width > 0 && rg.height > 0 {
            let gx = pen_x + rg.offset_x;
            // pen_y is the baseline; offset_y (ymin) is distance from baseline to
            // bottom of the glyph bitmap, so top of bitmap = baseline - offset_y - height.
            let gy = pen_y - rg.offset_y - rg.height as f32;

            glyphs.push(PositionedGlyph {
                codepoint: c,
                x: gx,
                y: gy,
                width: rg.width,
                height: rg.height,
                font_size,
            });
        }

        pen_x += advance;
    }

    glyphs
}
