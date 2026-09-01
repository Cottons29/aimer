use aimer_widget::base::{BuildContext, Color, ResolvedSize, Size, Vec2d};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement, PortableWidget,
    Rebuildable, RequiredChild, VisitorElement, Widget,
};
use aimer_canvas::{MaterialDrawRequest, MaterialKind};

/// The policy used by a material when deciding whether it may animate.
///
/// The policy is deliberately platform-neutral. A host can choose the reduced
/// policy for an accessibility or battery preference without exposing a
/// renderer or system visual-effect handle to the widget tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaterialMotionPolicy {
    /// Permit the material's configured animation and interaction response.
    #[default]
    Full,
    /// Disable animated fields and keep only the static surface treatment.
    Reduced,
}

/// Plain, bounded values used by [`Glass`] and as the base for [`Liquid`].
///
/// Values are normalized at construction and again when a widget becomes an
/// element. This keeps a stale or externally assembled value from introducing
/// a non-finite number into a canvas command. The bounds are intentionally
/// conservative so a single surface cannot request an unbounded blur or
/// sampling budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassMaterial {
    tint: Color,
    opacity: f32,
    blur_radius: f32,
    saturation: f32,
    brightness: f32,
    contrast: f32,
    border_width: f32,
    border_color: Color,
    corner_radii: [f32; 4],
    shadow_color: Color,
    shadow_blur: f32,
    elevation: f32,
    edge_lighting: f32,
    specular_highlight: f32,
    quality: u8,
    motion_policy: MaterialMotionPolicy,
}

impl GlassMaterial {
    /// The default backdrop blur radius in logical pixels.
    pub const DEFAULT_BLUR_RADIUS: f32 = 16.0;
    /// The largest blur radius accepted by the portable material contract.
    pub const MAX_BLUR_RADIUS: f32 = 96.0;
    /// The largest corner radius accepted before it is resolved against bounds.
    pub const MAX_CORNER_RADIUS: f32 = 1_024.0;
    /// The largest shadow blur accepted by the fallback path.
    pub const MAX_SHADOW_BLUR: f32 = 128.0;
    /// The largest fallback elevation accepted by the material contract.
    pub const MAX_ELEVATION: f32 = 64.0;
    /// The largest normalized raised-edge lighting strength.
    pub const MAX_EDGE_LIGHTING: f32 = 1.0;
    /// The largest normalized surface highlight strength.
    pub const MAX_SPECULAR_HIGHLIGHT: f32 = 1.0;
    /// The minimum number of samples in the portable quality policy.
    pub const MIN_QUALITY: u8 = 1;
    /// The maximum number of samples in the portable quality policy.
    pub const MAX_QUALITY: u8 = 4;

    /// Creates the restrained default translucent surface.
    #[inline]
    pub fn new() -> Self {
        Self {
            tint: Color::Rgba(255, 255, 255, 255),
            opacity: 0.72,
            blur_radius: 16.0,
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            border_width: 1.0,
            border_color: Color::Rgba(255, 255, 255, 96),
            corner_radii: [16.0; 4],
            shadow_color: Color::Rgba(0, 0, 0, 72),
            shadow_blur: 16.0,
            elevation: 4.0,
            edge_lighting: 0.35,
            specular_highlight: 0.16,
            quality: 2,
            motion_policy: MaterialMotionPolicy::Full,
        }
    }

    /// Sets the tint used by the surface.
    #[inline]
    pub fn tint(mut self, tint: impl Into<Color>) -> Self {
        self.tint = tint.into();
        self
    }

    /// Sets the surface opacity in `0.0..=1.0`.
    ///
    /// Non-finite values use the default opacity and finite values are clamped.
    #[inline]
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = normalize(opacity, 0.72, 0.0, 1.0);
        self
    }

    /// Sets the backdrop blur radius in logical pixels.
    #[inline]
    pub fn blur_radius(mut self, blur_radius: f32) -> Self {
        self.blur_radius = normalize(
            blur_radius,
            Self::DEFAULT_BLUR_RADIUS,
            0.0,
            Self::MAX_BLUR_RADIUS,
        );
        self
    }

    /// Sets the backdrop blur as a normalized intensity from `0.0` to `1.0`.
    ///
    /// `0.0` keeps the captured backdrop sharp and `1.0` uses the maximum
    /// bounded blur radius. [`Self::blur_radius`] remains available when an
    /// exact logical-pixel radius is preferred.
    #[inline]
    pub fn blur_intensity(mut self, intensity: f32) -> Self {
        let default_intensity = Self::DEFAULT_BLUR_RADIUS / Self::MAX_BLUR_RADIUS;
        let intensity = normalize(intensity, default_intensity, 0.0, 1.0);
        self.blur_radius = intensity * Self::MAX_BLUR_RADIUS;
        self
    }

    /// Sets the backdrop saturation multiplier.
    #[inline]
    pub fn saturation(mut self, saturation: f32) -> Self {
        self.saturation = normalize(saturation, 1.0, 0.0, 3.0);
        self
    }

    /// Sets the backdrop brightness multiplier.
    #[inline]
    pub fn brightness(mut self, brightness: f32) -> Self {
        self.brightness = normalize(brightness, 1.0, 0.0, 3.0);
        self
    }

    /// Sets the backdrop contrast multiplier.
    #[inline]
    pub fn contrast(mut self, contrast: f32) -> Self {
        self.contrast = normalize(contrast, 1.0, 0.0, 3.0);
        self
    }

    /// Sets the uniform border width in logical pixels.
    #[inline]
    pub fn border_width(mut self, border_width: f32) -> Self {
        self.border_width = normalize(border_width, 1.0, 0.0, 16.0);
        self
    }

    /// Sets the border highlight color.
    #[inline]
    pub fn border_color(mut self, border_color: impl Into<Color>) -> Self {
        self.border_color = border_color.into();
        self
    }

    /// Sets one corner radius for all four corners.
    #[inline]
    pub fn corner_radius(mut self, corner_radius: f32) -> Self {
        self.corner_radii = [normalize(
            corner_radius,
            16.0,
            0.0,
            Self::MAX_CORNER_RADIUS,
        ); 4];
        self
    }

    /// Sets `[top-left, top-right, bottom-right, bottom-left]` radii.
    #[inline]
    pub fn corner_radii(mut self, corner_radii: [f32; 4]) -> Self {
        for (slot, value) in self.corner_radii.iter_mut().zip(corner_radii) {
            *slot = normalize(value, 0.0, 0.0, Self::MAX_CORNER_RADIUS);
        }
        self
    }

    /// Sets the shadow color used by the GPU path and bounded fallback.
    #[inline]
    pub fn shadow_color(mut self, shadow_color: impl Into<Color>) -> Self {
        self.shadow_color = shadow_color.into();
        self
    }

    /// Sets the shadow blur radius in logical pixels.
    #[inline]
    pub fn shadow_blur(mut self, shadow_blur: f32) -> Self {
        self.shadow_blur = normalize(shadow_blur, 16.0, 0.0, Self::MAX_SHADOW_BLUR);
        self
    }

    /// Sets the non-negative shadow elevation in logical pixels.
    #[inline]
    pub fn elevation(mut self, elevation: f32) -> Self {
        self.elevation = normalize(elevation, 4.0, 0.0, Self::MAX_ELEVATION);
        self
    }

    /// Sets the normalized lighting strength around the raised glass edge.
    #[inline]
    pub fn edge_lighting(mut self, strength: f32) -> Self {
        self.edge_lighting = normalize(strength, 0.35, 0.0, Self::MAX_EDGE_LIGHTING);
        self
    }

    /// Sets the normalized strength of highlights across the glass surface.
    #[inline]
    pub fn specular_highlight(mut self, strength: f32) -> Self {
        self.specular_highlight = normalize(
            strength,
            0.16,
            0.0,
            Self::MAX_SPECULAR_HIGHLIGHT,
        );
        self
    }

    /// Sets the bounded sampling quality. Values outside `1..=4` are clamped.
    #[inline]
    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = quality.clamp(Self::MIN_QUALITY, Self::MAX_QUALITY);
        self
    }

    /// Sets the reduced-motion policy.
    #[inline]
    pub fn motion_policy(mut self, motion_policy: MaterialMotionPolicy) -> Self {
        self.motion_policy = motion_policy;
        self
    }

    /// Returns the configured tint.
    #[inline]
    pub fn tint_color(self) -> Color {
        self.tint
    }

    /// Returns the normalized opacity.
    #[inline]
    pub fn opacity_value(self) -> f32 {
        self.opacity
    }

    /// Returns the normalized blur radius.
    #[inline]
    pub fn blur_radius_value(self) -> f32 {
        self.blur_radius
    }

    /// Returns the normalized backdrop blur intensity.
    #[inline]
    pub fn blur_intensity_value(self) -> f32 {
        let default_intensity = Self::DEFAULT_BLUR_RADIUS / Self::MAX_BLUR_RADIUS;
        normalize(
            self.blur_radius / Self::MAX_BLUR_RADIUS,
            default_intensity,
            0.0,
            1.0,
        )
    }

    /// Returns the normalized saturation multiplier.
    #[inline]
    pub fn saturation_value(self) -> f32 {
        self.saturation
    }

    /// Returns the normalized brightness multiplier.
    #[inline]
    pub fn brightness_value(self) -> f32 {
        self.brightness
    }

    /// Returns the normalized contrast multiplier.
    #[inline]
    pub fn contrast_value(self) -> f32 {
        self.contrast
    }

    /// Returns the normalized border width.
    #[inline]
    pub fn border_width_value(self) -> f32 {
        self.border_width
    }

    /// Returns the border color.
    #[inline]
    pub fn border_color_value(self) -> Color {
        self.border_color
    }

    /// Returns the configured per-corner radii.
    #[inline]
    pub fn corner_radii_value(self) -> [f32; 4] {
        self.corner_radii
    }

    /// Returns the shadow color.
    #[inline]
    pub fn shadow_color_value(self) -> Color {
        self.shadow_color
    }

    /// Returns the normalized shadow blur.
    #[inline]
    pub fn shadow_blur_value(self) -> f32 {
        self.shadow_blur
    }

    /// Returns the normalized elevation.
    #[inline]
    pub fn elevation_value(self) -> f32 {
        self.elevation
    }

    /// Returns the normalized raised-edge lighting strength.
    #[inline]
    pub fn edge_lighting_value(self) -> f32 {
        self.edge_lighting
    }

    /// Returns the normalized surface highlight strength.
    #[inline]
    pub fn specular_highlight_value(self) -> f32 {
        self.specular_highlight
    }

    /// Returns the bounded quality value.
    #[inline]
    pub fn quality_value(self) -> u8 {
        self.quality
    }

    /// Returns the configured motion policy.
    #[inline]
    pub fn motion_policy_value(self) -> MaterialMotionPolicy {
        self.motion_policy
    }

    pub(crate) fn normalized(mut self) -> Self {
        self.opacity = normalize(self.opacity, 0.72, 0.0, 1.0);
        self.blur_radius = normalize(
            self.blur_radius,
            Self::DEFAULT_BLUR_RADIUS,
            0.0,
            Self::MAX_BLUR_RADIUS,
        );
        self.saturation = normalize(self.saturation, 1.0, 0.0, 3.0);
        self.brightness = normalize(self.brightness, 1.0, 0.0, 3.0);
        self.contrast = normalize(self.contrast, 1.0, 0.0, 3.0);
        self.border_width = normalize(self.border_width, 1.0, 0.0, 16.0);
        self.shadow_blur = normalize(self.shadow_blur, 16.0, 0.0, Self::MAX_SHADOW_BLUR);
        self.elevation = normalize(self.elevation, 4.0, 0.0, Self::MAX_ELEVATION);
        self.edge_lighting = normalize(
            self.edge_lighting,
            0.35,
            0.0,
            Self::MAX_EDGE_LIGHTING,
        );
        self.specular_highlight = normalize(
            self.specular_highlight,
            0.16,
            0.0,
            Self::MAX_SPECULAR_HIGHLIGHT,
        );
        self.quality = self.quality.clamp(Self::MIN_QUALITY, Self::MAX_QUALITY);
        for radius in &mut self.corner_radii {
            *radius = normalize(*radius, 0.0, 0.0, Self::MAX_CORNER_RADIUS);
        }
        self
    }

    fn fallback_tint(self) -> Color {
        with_opacity(self.tint, self.opacity)
    }

    fn fallback_border(self) -> Color {
        with_opacity(self.border_color, self.opacity)
    }

    fn fallback_shadow(self) -> Color {
        with_opacity(self.shadow_color, self.opacity)
    }
}

impl Default for GlassMaterial {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the renderer-neutral request shared by Glass and Liquid.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_material_request(
    kind: MaterialKind,
    base: GlassMaterial,
    size: ResolvedSize,
    distortion_strength: f32,
    edge_lighting: f32,
    specular_highlight: f32,
    animation_speed: f32,
    animation_time: f32,
    interaction: f32,
    // `[blob_amount, blob_seed, magnification, tip_pull, chromatic_aberration,
    // bevel_radius]`: Liquid-only silhouette/refraction shape. Glass passes
    // `[0.0; 6]`.
    liquid_shape: [f32; 6],
) -> MaterialDrawRequest {
    let mut request = MaterialDrawRequest::new(kind, [0.0, 0.0, size.width, size.height]);
    request.tint = color_to_array(base.tint_color());
    request.opacity = base.opacity_value();
    request.blur_radius = base.blur_radius_value();
    request.saturation = base.saturation_value();
    request.brightness = base.brightness_value();
    request.contrast = base.contrast_value();
    request.border_color = color_to_array(base.border_color_value());
    request.border_width = base.border_width_value();
    request.corner_radii = base.corner_radii_value();
    request.shadow_color = color_to_array(base.shadow_color_value());
    request.shadow_blur = base.shadow_blur_value();
    request.elevation = base.elevation_value();
    request.distortion_strength = distortion_strength;
    request.edge_lighting = edge_lighting;
    request.specular_highlight = specular_highlight;
    request.animation_speed = animation_speed;
    request.animation_time = animation_time;
    request.interaction = interaction;
    request.blob_amount = liquid_shape[0];
    request.blob_seed = liquid_shape[1];
    request.magnification = liquid_shape[2];
    request.tip_pull = liquid_shape[3];
    request.chromatic_aberration = liquid_shape[4];
    request.bevel_radius = liquid_shape[5];
    request.quality = base.quality_value();
    request.motion_policy = match base.motion_policy_value() {
        MaterialMotionPolicy::Full => aimer_canvas::MaterialMotionPolicy::Full,
        MaterialMotionPolicy::Reduced => aimer_canvas::MaterialMotionPolicy::Reduced,
    };
    request.normalized()
}

#[inline]
fn color_to_array(color: Color) -> [f32; 4] {
    let (red, green, blue, alpha) = color.to_rgba();
    [
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
        f32::from(alpha) / 255.0,
    ]
}

/// A translucent, single-child surface.
///
/// `Glass` is a visual wrapper: it has no independent event, focus, or
/// accessibility node. The child is the final type-state transition and is
/// moved into the retained element without cloning. The bounded canvas fallback
/// remains underneath a Cupid-owned material enhancement, without changing
/// this widget's layout or interaction behavior.
pub struct Glass<W = RequiredChild> {
    child: W,
    material: GlassMaterial,
}

impl Glass {
    /// Creates a default glass builder without a child.
    #[inline]
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            material: GlassMaterial::new(),
        }
    }

    /// Replaces the material configuration.
    #[inline]
    pub fn material(mut self, material: GlassMaterial) -> Self {
        self.material = material.normalized();
        self
    }

    /// Sets the material tint.
    #[inline]
    pub fn tint(mut self, tint: impl Into<Color>) -> Self {
        self.material = self.material.tint(tint);
        self
    }

    /// Sets the material opacity.
    #[inline]
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.material = self.material.opacity(opacity);
        self
    }

    /// Sets the backdrop blur radius.
    #[inline]
    pub fn blur_radius(mut self, blur_radius: f32) -> Self {
        self.material = self.material.blur_radius(blur_radius);
        self
    }

    /// Sets the backdrop blur as a normalized intensity from `0.0` to `1.0`.
    #[inline]
    pub fn blur_intensity(mut self, intensity: f32) -> Self {
        self.material = self.material.blur_intensity(intensity);
        self
    }

    /// Sets backdrop saturation, brightness, and contrast independently.
    #[inline]
    pub fn saturation(mut self, saturation: f32) -> Self {
        self.material = self.material.saturation(saturation);
        self
    }

    /// Sets the backdrop brightness multiplier.
    #[inline]
    pub fn brightness(mut self, brightness: f32) -> Self {
        self.material = self.material.brightness(brightness);
        self
    }

    /// Sets the backdrop contrast multiplier.
    #[inline]
    pub fn contrast(mut self, contrast: f32) -> Self {
        self.material = self.material.contrast(contrast);
        self
    }

    /// Sets the border width.
    #[inline]
    pub fn border_width(mut self, border_width: f32) -> Self {
        self.material = self.material.border_width(border_width);
        self
    }

    /// Sets the border highlight color.
    #[inline]
    pub fn border_color(mut self, border_color: impl Into<Color>) -> Self {
        self.material = self.material.border_color(border_color);
        self
    }

    /// Sets a uniform corner radius.
    #[inline]
    pub fn corner_radius(mut self, corner_radius: f32) -> Self {
        self.material = self.material.corner_radius(corner_radius);
        self
    }

    /// Sets per-corner radii.
    #[inline]
    pub fn corner_radii(mut self, corner_radii: [f32; 4]) -> Self {
        self.material = self.material.corner_radii(corner_radii);
        self
    }

    /// Sets shadow color, blur, and elevation.
    #[inline]
    pub fn shadow_color(mut self, shadow_color: impl Into<Color>) -> Self {
        self.material = self.material.shadow_color(shadow_color);
        self
    }

    /// Sets shadow blur.
    #[inline]
    pub fn shadow_blur(mut self, shadow_blur: f32) -> Self {
        self.material = self.material.shadow_blur(shadow_blur);
        self
    }

    /// Sets shadow elevation.
    #[inline]
    pub fn elevation(mut self, elevation: f32) -> Self {
        self.material = self.material.elevation(elevation);
        self
    }

    /// Sets normalized lighting around the raised glass edge.
    #[inline]
    pub fn edge_lighting(mut self, strength: f32) -> Self {
        self.material = self.material.edge_lighting(strength);
        self
    }

    /// Sets normalized highlights across the glass surface.
    #[inline]
    pub fn specular_highlight(mut self, strength: f32) -> Self {
        self.material = self.material.specular_highlight(strength);
        self
    }

    /// Sets the bounded material quality.
    #[inline]
    pub fn quality(mut self, quality: u8) -> Self {
        self.material = self.material.quality(quality);
        self
    }

    /// Sets the reduced-motion policy.
    #[inline]
    pub fn motion_policy(mut self, motion_policy: MaterialMotionPolicy) -> Self {
        self.material = self.material.motion_policy(motion_policy);
        self
    }

    /// Attaches the required child and completes this builder.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> Glass<W> {
        Glass {
            child,
            material: self.material.normalized(),
        }
    }

    /// Attaches a child and erases the resulting widget type.
    #[inline]
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl Default for Glass {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Widget + 'static> PortableWidget for Glass<W> {}

impl<W: Widget + 'static> Widget for Glass<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        RawGlass {
            child: self.child.to_element(ctx),
            material: self.material.normalized(),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Glass"
    }
}

struct RawGlass {
    child: AnyElement,
    material: GlassMaterial,
}

impl Rebuildable for RawGlass {}

impl Drawable for RawGlass {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.child.computed_size(ctx);
        if size.width <= 0.0 || size.height <= 0.0 {
            self.child.draw(ctx);
            return;
        }

        let material = self.material.normalized();
        ctx.canvas.save();
        let radii = resolved_radii(material.corner_radii_value(), size);

        let shadow = material.fallback_shadow();
        if shadow.alpha() != 0 && material.shadow_blur_value() > 0.0 {
            ctx.canvas.draw_shadow_rect(
                Vec2d { x: 0.0, y: 0.0 },
                size,
                shadow,
                [0.0, material.elevation_value(), material.shadow_blur_value(), 0.0],
                radii,
                false,
                [0.0; 3],
            );
        }

        ctx.canvas
            .fill_color_rect_per_corner(Vec2d { x: 0.0, y: 0.0 }, size, material.fallback_tint(), radii);
        if material.border_width_value() > 0.0 && material.fallback_border().alpha() != 0 {
            ctx.canvas.stroke_rect_per_side(
                Vec2d { x: 0.0, y: 0.0 },
                size,
                material.fallback_border(),
                [material.border_width_value(); 4],
                radii,
            );
        }

        ctx.canvas.draw_material(build_material_request(
            MaterialKind::Glass,
            material,
            size,
            0.0,
            material.edge_lighting_value(),
            material.specular_highlight_value(),
            0.0,
            0.0,
            0.0,
            [0.0; 6],
        ));

        // The material is painted before the child so the child's content stays
        // crisp above the frosted surface. No canvas state is changed on behalf
        // of the child besides the balanced save/restore above.
        self.child.draw(ctx);
        ctx.canvas.restore();
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        // The material samples the current framebuffer backdrop and its
        // configuration can change during a rebuild. Retaining this subtree
        // would freeze both the captured scene and the active blur radius.
        false
    }
}

impl EventElement for RawGlass {
    #[inline]
    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    #[inline]
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl LayoutElement for RawGlass {
    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child.flex()
    }

    fn is_layout_stable(&self) -> bool {
        self.child.is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child.pos_start_end()
    }
}

impl VisitorElement for RawGlass {
    #[inline]
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "Glass"
    }
}

#[inline]
fn normalize(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

#[inline]
fn with_opacity(color: Color, opacity: f32) -> Color {
    let (red, green, blue, alpha) = color.to_rgba();
    let alpha = (f32::from(alpha) * opacity)
        .round()
        .clamp(0.0, 255.0) as u8;
    Color::Rgba(red, green, blue, alpha)
}

#[inline]
fn resolved_radii(radii: [f32; 4], size: ResolvedSize) -> [f32; 4] {
    let limit = (size.width.min(size.height) * 0.5).max(0.0);
    radii.map(|radius| radius.min(limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StableChild;

    impl VisitorElement for StableChild {
        fn debug_name(&self) -> &'static str {
            "StableChild"
        }
    }

    impl Drawable for StableChild {
        fn draw(&self, _ctx: &BuildContext) {}

        fn is_paint_stable(&self) -> bool {
            true
        }
    }

    impl EventElement for StableChild {}
    impl LayoutElement for StableChild {}
    impl Rebuildable for StableChild {}

    #[test]
    fn material_values_are_finite_and_bounded() {
        let material = GlassMaterial::new()
            .opacity(f32::NAN)
            .blur_radius(f32::INFINITY)
            .saturation(-1.0)
            .brightness(99.0)
            .contrast(f32::NEG_INFINITY)
            .corner_radii([f32::NAN, -2.0, 2_000.0, f32::INFINITY])
            .shadow_blur(f32::INFINITY)
            .elevation(-5.0)
            .edge_lighting(2.0)
            .specular_highlight(f32::NAN)
            .quality(0);

        assert_eq!(material.opacity_value(), 0.72);
        assert_eq!(material.blur_radius_value(), 16.0);
        assert_eq!(material.saturation_value(), 0.0);
        assert_eq!(material.brightness_value(), 3.0);
        assert_eq!(material.contrast_value(), 1.0);
        assert_eq!(material.corner_radii_value(), [0.0, 0.0, GlassMaterial::MAX_CORNER_RADIUS, 0.0]);
        assert_eq!(material.shadow_blur_value(), 16.0);
        assert_eq!(material.elevation_value(), 0.0);
        assert_eq!(material.edge_lighting_value(), GlassMaterial::MAX_EDGE_LIGHTING);
        assert_eq!(material.specular_highlight_value(), 0.16);
        assert_eq!(material.quality_value(), GlassMaterial::MIN_QUALITY);
    }

    #[test]
    fn blur_intensity_is_a_bounded_public_alias_for_backdrop_radius() {
        let material = GlassMaterial::new().blur_intensity(0.75);

        assert_eq!(material.blur_intensity_value(), 0.75);
        assert_eq!(material.blur_radius_value(), GlassMaterial::MAX_BLUR_RADIUS * 0.75);

        let invalid = GlassMaterial::new().blur_intensity(f32::NAN);
        assert_eq!(invalid.blur_intensity_value(), 16.0 / GlassMaterial::MAX_BLUR_RADIUS);
    }

    #[test]
    fn glass_never_marks_a_backdrop_dependent_surface_as_paint_stable() {
        let glass = RawGlass {
            child: StableChild.boxed(),
            material: GlassMaterial::new(),
        };

        assert!(!glass.is_paint_stable());
    }

    #[test]
    fn radii_are_limited_to_half_the_surface_extent() {
        assert_eq!(
            resolved_radii([100.0, 8.0, 100.0, 0.0], ResolvedSize { width: 40.0, height: 20.0 }),
            [10.0, 8.0, 10.0, 0.0]
        );
    }

    #[test]
    fn fallback_alpha_multiplies_material_opacity() {
        let material = GlassMaterial::new()
            .tint(Color::Rgba(10, 20, 30, 200))
            .opacity(0.5);
        assert_eq!(material.fallback_tint().to_rgba(), (10, 20, 30, 100));
    }

    #[test]
    fn reduced_motion_is_a_plain_portable_policy() {
        let material = GlassMaterial::new().motion_policy(MaterialMotionPolicy::Reduced);
        assert_eq!(material.motion_policy_value(), MaterialMotionPolicy::Reduced);
    }
}
