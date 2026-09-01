mod render;

pub use render::MaterialPipeline;

/// Reserved custom-pipeline name for W14 material requests.
pub const MATERIAL_PIPELINE_NAME: &str = "aimer.material";

/// The packet version shared by the canvas bridge and Cupid.
pub const MATERIAL_PACKET_VERSION: u8 = 1;
/// The exact size of one encoded material request.
pub const MATERIAL_PACKET_LEN: usize = 244;
/// Maximum intermediate texture dimension used by one surface.
pub const MAX_INTERMEDIATE_DIMENSION: u32 = 2_048;
/// Maximum intermediate RGBA pixels used by one surface.
pub const MAX_INTERMEDIATE_PIXELS: u64 = 4 * 1024 * 1024;

/// Selects the surface family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum MaterialKind {
    /// Static translucent glass.
    #[default]
    Glass = 0,
    /// Dynamic glass with bounded liquid fields.
    Liquid = 1,
}

impl MaterialKind {
    #[inline]
    fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Glass),
            1 => Some(Self::Liquid),
            _ => None,
        }
    }
}

/// Policy controlling dynamic fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum MaterialMotionPolicy {
    /// Permit configured Liquid motion.
    #[default]
    Full = 0,
    /// Disable distortion and animated highlight motion.
    Reduced = 1,
}

impl MaterialMotionPolicy {
    #[inline]
    fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Full),
            1 => Some(Self::Reduced),
            _ => None,
        }
    }
}

/// An optional rounded clip for a surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialClip {
    /// `[x, y, width, height]` in local coordinates.
    pub rect: [f32; 4],
    /// `[top-left, top-right, bottom-right, bottom-left]` radii.
    pub corner_radii: [f32; 4],
}

/// A material request after it has crossed the canvas boundary.
///
/// This is intentionally a plain value model. GPU resources, offscreen
/// textures, and render-pass lifetimes belong to the stage that consumes this
/// request, not to the widget or canvas packages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialRequest {
    /// Surface family.
    pub kind: MaterialKind,
    /// Local bounds `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// Column-major 3x3 transform.
    pub transform: [f32; 9],
    /// Optional clip.
    pub clip: Option<MaterialClip>,
    /// Stable ordering key.
    pub z_order: u32,
    /// Tint RGBA.
    pub tint: [f32; 4],
    /// Surface opacity.
    pub opacity: f32,
    /// Backdrop blur radius.
    pub blur_radius: f32,
    /// Backdrop saturation multiplier.
    pub saturation: f32,
    /// Backdrop brightness multiplier.
    pub brightness: f32,
    /// Backdrop contrast multiplier.
    pub contrast: f32,
    /// Border RGBA.
    pub border_color: [f32; 4],
    /// Border width.
    pub border_width: f32,
    /// Surface corner radii.
    pub corner_radii: [f32; 4],
    /// Shadow RGBA.
    pub shadow_color: [f32; 4],
    /// Shadow blur radius.
    pub shadow_blur: f32,
    /// Shadow/elevation offset.
    pub elevation: f32,
    /// Liquid distortion strength.
    pub distortion_strength: f32,
    /// Liquid edge-lighting strength.
    pub edge_lighting: f32,
    /// Liquid specular strength.
    pub specular_highlight: f32,
    /// Liquid animation speed.
    pub animation_speed: f32,
    /// Injected animation time.
    pub animation_time: f32,
    /// Pointer/interaction influence.
    pub interaction: f32,
    /// Liquid organic-silhouette wobble strength.
    pub blob_amount: f32,
    /// Liquid deterministic silhouette variation seed, in `0.0..=1.0`.
    pub blob_seed: f32,
    /// Liquid backdrop lens/bulge magnification.
    pub magnification: f32,
    /// Liquid teardrop-tip taper strength.
    pub tip_pull: f32,
    /// Liquid chromatic-aberration strength at the refracting edge.
    pub chromatic_aberration: f32,
    /// Liquid fake 3D bevel radius in logical pixels, used to build the
    /// rounded-edge surface normal that drives physically-styled refraction.
    pub bevel_radius: f32,
    /// Sampling quality in `1..=4`.
    pub quality: u8,
    /// Reduced-motion policy.
    pub motion_policy: MaterialMotionPolicy,
}

impl MaterialRequest {
    /// Creates a request with the same safe defaults as the canvas bridge.
    #[inline]
    pub fn new(kind: MaterialKind, bounds: [f32; 4]) -> Self {
        Self {
            kind,
            bounds: normalize_rect(bounds),
            transform: identity_transform(),
            clip: None,
            z_order: 0,
            tint: [1.0, 1.0, 1.0, 1.0],
            opacity: 0.72,
            blur_radius: 16.0,
            saturation: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            border_color: [1.0, 1.0, 1.0, 96.0 / 255.0],
            border_width: 1.0,
            corner_radii: [16.0; 4],
            shadow_color: [0.0, 0.0, 0.0, 72.0 / 255.0],
            shadow_blur: 16.0,
            elevation: 4.0,
            distortion_strength: 0.18,
            edge_lighting: 0.35,
            specular_highlight: 0.28,
            animation_speed: 0.5,
            animation_time: 0.0,
            interaction: 0.0,
            blob_amount: 0.0,
            blob_seed: 0.5,
            magnification: 0.45,
            tip_pull: 0.0,
            chromatic_aberration: 0.3,
            bevel_radius: 28.0,
            quality: 2,
            motion_policy: MaterialMotionPolicy::Full,
        }
    }

    #[cfg(test)]
    pub(crate) fn encode_for_test(self) -> Vec<u8> {
        let request = self.normalized();
        let mut bytes = Vec::with_capacity(MATERIAL_PACKET_LEN);
        bytes.extend_from_slice(b"AIMR");
        bytes.push(MATERIAL_PACKET_VERSION);
        bytes.push(request.kind as u8);
        bytes.push(request.motion_policy as u8);
        bytes.push(request.quality);
        bytes.extend_from_slice(&request.z_order.to_le_bytes());
        bytes.push(request.clip.is_some() as u8);
        bytes.extend_from_slice(&[0, 0, 0]);
        let clip = request.clip.unwrap_or(MaterialClip {
            rect: [0.0; 4],
            corner_radii: [0.0; 4],
        });
        for values in [
            request.bounds.as_slice(),
            request.transform.as_slice(),
            clip.rect.as_slice(),
            clip.corner_radii.as_slice(),
            request.tint.as_slice(),
            core::slice::from_ref(&request.opacity),
            core::slice::from_ref(&request.blur_radius),
            core::slice::from_ref(&request.saturation),
            core::slice::from_ref(&request.brightness),
            core::slice::from_ref(&request.contrast),
            request.border_color.as_slice(),
            core::slice::from_ref(&request.border_width),
            request.corner_radii.as_slice(),
            request.shadow_color.as_slice(),
            core::slice::from_ref(&request.shadow_blur),
            core::slice::from_ref(&request.elevation),
            core::slice::from_ref(&request.distortion_strength),
            core::slice::from_ref(&request.edge_lighting),
            core::slice::from_ref(&request.specular_highlight),
            core::slice::from_ref(&request.animation_speed),
            core::slice::from_ref(&request.animation_time),
            core::slice::from_ref(&request.interaction),
            core::slice::from_ref(&request.blob_amount),
            core::slice::from_ref(&request.blob_seed),
            core::slice::from_ref(&request.magnification),
            core::slice::from_ref(&request.tip_pull),
            core::slice::from_ref(&request.chromatic_aberration),
            core::slice::from_ref(&request.bevel_radius),
        ] {
            for value in values {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        debug_assert_eq!(bytes.len(), MATERIAL_PACKET_LEN);
        bytes
    }

    /// Normalizes finite/clamped values and applies reduced-motion policy.
    #[inline]
    pub fn normalized(mut self) -> Self {
        self.bounds = normalize_rect(self.bounds);
        self.transform = normalize_transform(self.transform);
        self.clip = self.clip.map(|clip| MaterialClip {
            rect: normalize_rect(clip.rect),
            corner_radii: normalize_radii(clip.corner_radii),
        });
        self.tint = normalize_color(self.tint);
        self.opacity = normalize(self.opacity, 0.72, 0.0, 1.0);
        self.blur_radius = normalize(self.blur_radius, 16.0, 0.0, 96.0);
        self.saturation = normalize(self.saturation, 1.0, 0.0, 3.0);
        self.brightness = normalize(self.brightness, 1.0, 0.0, 3.0);
        self.contrast = normalize(self.contrast, 1.0, 0.0, 3.0);
        self.border_color = normalize_color(self.border_color);
        self.border_width = normalize(self.border_width, 1.0, 0.0, 16.0);
        self.corner_radii = normalize_radii(self.corner_radii);
        self.shadow_color = normalize_color(self.shadow_color);
        self.shadow_blur = normalize(self.shadow_blur, 16.0, 0.0, 128.0);
        self.elevation = normalize(self.elevation, 4.0, 0.0, 64.0);
        self.distortion_strength = normalize(self.distortion_strength, 0.18, 0.0, 1.0);
        self.edge_lighting = normalize(self.edge_lighting, 0.35, 0.0, 1.0);
        self.specular_highlight = normalize(self.specular_highlight, 0.28, 0.0, 1.0);
        self.animation_speed = normalize(self.animation_speed, 0.5, 0.0, 4.0);
        self.animation_time = normalize(self.animation_time, 0.0, -1_000_000.0, 1_000_000.0);
        self.interaction = normalize(self.interaction, 0.0, 0.0, 1.0);
        self.blob_amount = normalize(self.blob_amount, 0.0, 0.0, 1.0);
        self.blob_seed = normalize(self.blob_seed, 0.5, 0.0, 1.0);
        self.magnification = normalize(self.magnification, 0.45, 0.0, 1.0);
        self.tip_pull = normalize(self.tip_pull, 0.0, 0.0, 1.0);
        self.chromatic_aberration = normalize(self.chromatic_aberration, 0.3, 0.0, 1.0);
        self.bevel_radius = normalize(self.bevel_radius, 28.0, 0.0, 512.0);
        self.quality = self.quality.clamp(1, 4);
        if self.motion_policy == MaterialMotionPolicy::Reduced {
            self.distortion_strength = 0.0;
            self.animation_speed = 0.0;
        }
        self
    }

    /// Decodes the fixed packet emitted by `aimer_canvas`.
    pub fn decode(bytes: &[u8]) -> Result<Self, MaterialDecodeError> {
        if bytes.len() != MATERIAL_PACKET_LEN {
            return Err(MaterialDecodeError::InvalidLength {
                expected: MATERIAL_PACKET_LEN,
                actual: bytes.len(),
            });
        }
        let mut reader = PacketReader::new(bytes);
        if reader.bytes::<4>()? != b"AIMR" {
            return Err(MaterialDecodeError::InvalidMagic);
        }
        if reader.u8()? != MATERIAL_PACKET_VERSION {
            return Err(MaterialDecodeError::UnsupportedVersion);
        }
        let kind = MaterialKind::from_wire(reader.u8()?)
            .ok_or(MaterialDecodeError::UnsupportedKind)?;
        let motion_policy = MaterialMotionPolicy::from_wire(reader.u8()?)
            .ok_or(MaterialDecodeError::UnsupportedMotionPolicy)?;
        let quality = reader.u8()?;
        let z_order = reader.u32()?;
        let clip_present = reader.u8()?;
        let reserved = reader.bytes::<3>()?;
        if !reserved.iter().all(|value| *value == 0) {
            return Err(MaterialDecodeError::NonZeroReserved);
        }
        if clip_present > 1 {
            return Err(MaterialDecodeError::InvalidClipFlag);
        }

        let bounds = reader.f32s::<4>()?;
        let transform = reader.f32s::<9>()?;
        let clip_rect = reader.f32s::<4>()?;
        let clip_radii = reader.f32s::<4>()?;
        let tint = reader.f32s::<4>()?;
        let opacity = reader.f32()?;
        let blur_radius = reader.f32()?;
        let saturation = reader.f32()?;
        let brightness = reader.f32()?;
        let contrast = reader.f32()?;
        let border_color = reader.f32s::<4>()?;
        let border_width = reader.f32()?;
        let corner_radii = reader.f32s::<4>()?;
        let shadow_color = reader.f32s::<4>()?;
        let shadow_blur = reader.f32()?;
        let elevation = reader.f32()?;
        let distortion_strength = reader.f32()?;
        let edge_lighting = reader.f32()?;
        let specular_highlight = reader.f32()?;
        let animation_speed = reader.f32()?;
        let animation_time = reader.f32()?;
        let interaction = reader.f32()?;
        let blob_amount = reader.f32()?;
        let blob_seed = reader.f32()?;
        let magnification = reader.f32()?;
        let tip_pull = reader.f32()?;
        let chromatic_aberration = reader.f32()?;
        let bevel_radius = reader.f32()?;
        debug_assert_eq!(reader.position(), MATERIAL_PACKET_LEN);

        Ok(Self {
            kind,
            bounds,
            transform,
            clip: (clip_present == 1).then_some(MaterialClip {
                rect: clip_rect,
                corner_radii: clip_radii,
            }),
            z_order,
            tint,
            opacity,
            blur_radius,
            saturation,
            brightness,
            contrast,
            border_color,
            border_width,
            corner_radii,
            shadow_color,
            shadow_blur,
            elevation,
            distortion_strength,
            edge_lighting,
            specular_highlight,
            animation_speed,
            animation_time,
            interaction,
            blob_amount,
            blob_seed,
            magnification,
            tip_pull,
            chromatic_aberration,
            bevel_radius,
            quality,
            motion_policy,
        }
        .normalized())
    }

    /// Returns a deterministic phase for the Liquid highlight field.
    #[inline]
    pub fn effective_phase(self) -> f32 {
        let request = self.normalized();
        let phase = request.animation_time * request.animation_speed + request.interaction;
        phase.is_finite().then_some(phase).unwrap_or(0.0)
    }

    /// Creates the CPU-side fallback draw description.
    #[inline]
    pub fn fallback(self) -> FallbackDraw {
        let request = self.normalized();
        FallbackDraw {
            bounds: request.bounds,
            transform: request.transform,
            clip: request.clip,
            z_order: request.z_order,
            fill_color: multiply_alpha(request.tint, request.opacity),
            border_color: multiply_alpha(request.border_color, request.opacity),
            border_width: request.border_width,
            corner_radii: request.corner_radii,
            shadow_color: multiply_alpha(request.shadow_color, request.opacity),
            shadow_blur: request.shadow_blur,
            elevation: request.elevation,
        }
    }
}

/// Error returned when a canvas material packet cannot be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialDecodeError {
    /// Packet length was not the fixed schema length.
    InvalidLength { expected: usize, actual: usize },
    /// Packet did not begin with the W14 magic bytes.
    InvalidMagic,
    /// Packet version is not supported.
    UnsupportedVersion,
    /// Surface kind is not recognized.
    UnsupportedKind,
    /// Motion policy is not recognized.
    UnsupportedMotionPolicy,
    /// Reserved header bytes were non-zero.
    NonZeroReserved,
    /// Clip presence was neither zero nor one.
    InvalidClipFlag,
}

/// The stable draw values used by the translucent fallback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FallbackDraw {
    /// Local bounds.
    pub bounds: [f32; 4],
    /// Transform to use for the fallback.
    pub transform: [f32; 9],
    /// Effective clip.
    pub clip: Option<MaterialClip>,
    /// Ordering key.
    pub z_order: u32,
    /// Tint with opacity applied.
    pub fill_color: [f32; 4],
    /// Border color with opacity applied.
    pub border_color: [f32; 4],
    /// Border width.
    pub border_width: f32,
    /// Corner radii.
    pub corner_radii: [f32; 4],
    /// Shadow color with opacity applied.
    pub shadow_color: [f32; 4],
    /// Shadow blur.
    pub shadow_blur: f32,
    /// Elevation offset.
    pub elevation: f32,
}

/// Capabilities and budgets available to the material stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialCapabilities {
    /// Whether the stage can sample a background representation.
    pub backdrop_sampling: bool,
    /// Whether the stage can run Liquid distortion sampling.
    pub distortion_sampling: bool,
    /// Per-surface dimension budget.
    pub max_surface_dimension: u32,
    /// Per-surface RGBA pixel budget.
    pub max_intermediate_pixels: u64,
    /// Maximum requested sample quality.
    pub max_samples: u8,
}

impl Default for MaterialCapabilities {
    fn default() -> Self {
        Self {
            backdrop_sampling: true,
            distortion_sampling: true,
            max_surface_dimension: MAX_INTERMEDIATE_DIMENSION,
            max_intermediate_pixels: MAX_INTERMEDIATE_PIXELS,
            max_samples: 4,
        }
    }
}

/// Bounded render route selected for one material request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialRenderPath {
    /// No pixels need to be emitted for a zero-sized surface.
    Skip,
    /// Use the Cupid material shader and a bounded intermediate.
    Gpu,
    /// Use the Cupid-owned translucent/border/shadow fallback.
    Fallback,
}

/// Explains why a non-empty material used the Cupid-owned fallback.
///
/// A fallback is an intentional render decision, not a silent loss of the
/// material request. Keeping the reason in the platform-neutral stage plan
/// lets a host report capability and budget decisions without exposing GPU or
/// native visual-effect handles to the widget layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialFallbackReason {
    /// The render stage cannot sample the already-painted backdrop.
    BackdropSamplingUnavailable,
    /// The surface cannot fit within the configured intermediate budget.
    IntermediateBudgetExceeded,
    /// The requested quality is higher than the stage's sample budget.
    SampleQualityUnavailable,
    /// Liquid distortion was requested but the stage cannot sample it.
    DistortionSamplingUnavailable,
}

/// Bounded intermediate texture dimensions for one surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntermediateExtent {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA byte size.
    pub bytes: u64,
}

/// Describes the stage work without allocating GPU resources.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialStagePlan {
    /// Selected path.
    pub path: MaterialRenderPath,
    /// Explicit reason for [`MaterialRenderPath::Fallback`], if selected.
    pub fallback_reason: Option<MaterialFallbackReason>,
    /// Optional bounded intermediate.
    pub intermediate: Option<IntermediateExtent>,
    /// Effective dynamic phase.
    pub phase: f32,
    /// Effective distortion.
    pub distortion_strength: f32,
}

/// Selects a bounded GPU/fallback plan for a request.
pub fn plan_material(request: MaterialRequest, capabilities: MaterialCapabilities) -> MaterialStagePlan {
    let request = request.normalized();
    if request.bounds[2] <= 0.0 || request.bounds[3] <= 0.0 {
        return MaterialStagePlan {
            path: MaterialRenderPath::Skip,
            fallback_reason: None,
            intermediate: None,
            phase: 0.0,
            distortion_strength: 0.0,
        };
    }

    let intermediate = bounded_intermediate_extent(request, capabilities);
    let fallback_reason = if !capabilities.backdrop_sampling {
        Some(MaterialFallbackReason::BackdropSamplingUnavailable)
    } else if intermediate.is_none() {
        Some(MaterialFallbackReason::IntermediateBudgetExceeded)
    } else if request.quality > capabilities.max_samples {
        Some(MaterialFallbackReason::SampleQualityUnavailable)
    } else if request.kind == MaterialKind::Liquid
        && request.distortion_strength > 0.0
        && !capabilities.distortion_sampling
    {
        Some(MaterialFallbackReason::DistortionSamplingUnavailable)
    } else {
        None
    };
    let path = fallback_reason
        .map_or(MaterialRenderPath::Gpu, |_| MaterialRenderPath::Fallback);
    MaterialStagePlan {
        path,
        fallback_reason,
        intermediate: if path == MaterialRenderPath::Gpu {
            intermediate
        } else {
            None
        },
        phase: if path == MaterialRenderPath::Gpu {
            request.effective_phase()
        } else {
            0.0
        },
        distortion_strength: if path == MaterialRenderPath::Gpu {
            request.distortion_strength
        } else {
            0.0
        },
    }
}

/// Computes a downsampled, bounded intermediate allocation.
pub fn bounded_intermediate_extent(
    request: MaterialRequest,
    capabilities: MaterialCapabilities,
) -> Option<IntermediateExtent> {
    let request = request.normalized();
    let max_dimension = capabilities
        .max_surface_dimension
        .min(MAX_INTERMEDIATE_DIMENSION);
    if max_dimension == 0 {
        return None;
    }
    let downsample = match request.quality {
        1 => 4.0,
        2 => 2.0,
        _ => 1.0,
    };
    let width = (request.bounds[2] / downsample).ceil().max(1.0);
    let height = (request.bounds[3] / downsample).ceil().max(1.0);
    if !width.is_finite()
        || !height.is_finite()
        || width > max_dimension as f32
        || height > max_dimension as f32
    {
        return None;
    }
    let width = width as u32;
    let height = height as u32;
    let pixels = u64::from(width).checked_mul(u64::from(height))?;
    if pixels > capabilities.max_intermediate_pixels.min(MAX_INTERMEDIATE_PIXELS) {
        return None;
    }
    Some(IntermediateExtent {
        width,
        height,
        bytes: pixels.checked_mul(4)?,
    })
}

/// Sorts requests by stable z-order while preserving equal-key order.
pub fn order_requests(requests: &mut [MaterialRequest]) {
    requests.sort_by_key(|request| request.z_order);
}

/// Source for the material shader used by Cupid's registered material stage.
pub struct MaterialShader;

impl MaterialShader {
    /// Returns the bundled shader source without exposing a GPU handle.
    #[inline]
    pub const fn source() -> &'static str {
        include_str!("./material/shaders/material.wgsl")
    }
}

struct PacketReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> PacketReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn position(&self) -> usize {
        self.position
    }

    fn bytes<const N: usize>(&mut self) -> Result<&'a [u8; N], MaterialDecodeError> {
        let end = self.position.checked_add(N).ok_or(MaterialDecodeError::InvalidLength {
            expected: MATERIAL_PACKET_LEN,
            actual: self.bytes.len(),
        })?;
        let slice = self.bytes.get(self.position..end).ok_or(MaterialDecodeError::InvalidLength {
            expected: MATERIAL_PACKET_LEN,
            actual: self.bytes.len(),
        })?;
        self.position = end;
        slice.try_into().map_err(|_| MaterialDecodeError::InvalidLength {
            expected: MATERIAL_PACKET_LEN,
            actual: self.bytes.len(),
        })
    }

    fn u8(&mut self) -> Result<u8, MaterialDecodeError> {
        Ok(self.bytes::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, MaterialDecodeError> {
        Ok(u32::from_le_bytes(*self.bytes::<4>()?))
    }

    fn f32(&mut self) -> Result<f32, MaterialDecodeError> {
        Ok(f32::from_bits(u32::from_le_bytes(*self.bytes::<4>()?)))
    }

    fn f32s<const N: usize>(&mut self) -> Result<[f32; N], MaterialDecodeError> {
        let mut values = [0.0; N];
        for value in &mut values {
            *value = self.f32()?;
        }
        Ok(values)
    }
}

#[inline]
fn identity_transform() -> [f32; 9] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
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
fn normalize_rect(mut rect: [f32; 4]) -> [f32; 4] {
    rect[0] = normalize(rect[0], 0.0, -1_000_000.0, 1_000_000.0);
    rect[1] = normalize(rect[1], 0.0, -1_000_000.0, 1_000_000.0);
    rect[2] = normalize(rect[2], 0.0, 0.0, 1_000_000.0);
    rect[3] = normalize(rect[3], 0.0, 0.0, 1_000_000.0);
    rect
}

#[inline]
fn normalize_transform(mut transform: [f32; 9]) -> [f32; 9] {
    for value in &mut transform {
        *value = normalize(*value, 0.0, -1_000_000.0, 1_000_000.0);
    }
    transform
}

#[inline]
fn normalize_color(mut color: [f32; 4]) -> [f32; 4] {
    for channel in &mut color {
        *channel = normalize(*channel, 0.0, 0.0, 1.0);
    }
    color
}

#[inline]
fn normalize_radii(mut radii: [f32; 4]) -> [f32; 4] {
    for radius in &mut radii {
        *radius = normalize(*radius, 0.0, 0.0, 1_024.0);
    }
    radii
}

#[inline]
fn multiply_alpha(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] = (color[3] * opacity).clamp(0.0, 1.0);
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(request: MaterialRequest) -> Vec<u8> {
        let request = request.normalized();
        let mut bytes = Vec::with_capacity(MATERIAL_PACKET_LEN);
        bytes.extend_from_slice(b"AIMR");
        bytes.push(MATERIAL_PACKET_VERSION);
        bytes.push(request.kind as u8);
        bytes.push(request.motion_policy as u8);
        bytes.push(request.quality);
        bytes.extend_from_slice(&request.z_order.to_le_bytes());
        bytes.push(request.clip.is_some() as u8);
        bytes.extend_from_slice(&[0, 0, 0]);
        let clip = request.clip.unwrap_or(MaterialClip {
            rect: [0.0; 4],
            corner_radii: [0.0; 4],
        });
        for values in [
            request.bounds.as_slice(),
            request.transform.as_slice(),
            clip.rect.as_slice(),
            clip.corner_radii.as_slice(),
            request.tint.as_slice(),
            core::slice::from_ref(&request.opacity),
            core::slice::from_ref(&request.blur_radius),
            core::slice::from_ref(&request.saturation),
            core::slice::from_ref(&request.brightness),
            core::slice::from_ref(&request.contrast),
            request.border_color.as_slice(),
            core::slice::from_ref(&request.border_width),
            request.corner_radii.as_slice(),
            request.shadow_color.as_slice(),
            core::slice::from_ref(&request.shadow_blur),
            core::slice::from_ref(&request.elevation),
            core::slice::from_ref(&request.distortion_strength),
            core::slice::from_ref(&request.edge_lighting),
            core::slice::from_ref(&request.specular_highlight),
            core::slice::from_ref(&request.animation_speed),
            core::slice::from_ref(&request.animation_time),
            core::slice::from_ref(&request.interaction),
            core::slice::from_ref(&request.blob_amount),
            core::slice::from_ref(&request.blob_seed),
            core::slice::from_ref(&request.magnification),
            core::slice::from_ref(&request.tip_pull),
            core::slice::from_ref(&request.chromatic_aberration),
            core::slice::from_ref(&request.bevel_radius),
        ] {
            for value in values {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn packet_decoder_normalizes_values_and_preserves_geometry() {
        let mut request = MaterialRequest::new(MaterialKind::Liquid, [2.0, 3.0, 80.0, 40.0]);
        request.opacity = f32::NAN;
        request.tint = [f32::INFINITY, -1.0, 0.5, 0.25];
        request.motion_policy = MaterialMotionPolicy::Reduced;
        request.distortion_strength = 1.0;
        request.animation_speed = 4.0;
        let decoded = MaterialRequest::decode(&packet(request)).unwrap();

        assert_eq!(decoded.bounds, [2.0, 3.0, 80.0, 40.0]);
        assert_eq!(decoded.opacity, 0.72);
        assert_eq!(decoded.tint, [0.0, 0.0, 0.5, 0.25]);
        assert_eq!(decoded.distortion_strength, 0.0);
        assert_eq!(decoded.animation_speed, 0.0);
    }

    #[test]
    fn liquid_shape_fields_round_trip_and_clamp() {
        let mut request = MaterialRequest::new(MaterialKind::Liquid, [0.0, 0.0, 40.0, 40.0]);
        request.blob_amount = 0.6;
        request.blob_seed = 0.9;
        request.magnification = 0.75;
        request.tip_pull = 0.4;
        request.chromatic_aberration = 0.65;
        request.bevel_radius = 40.0;
        let decoded = MaterialRequest::decode(&packet(request)).unwrap();
        assert_eq!(decoded.blob_amount, 0.6);
        assert_eq!(decoded.blob_seed, 0.9);
        assert_eq!(decoded.magnification, 0.75);
        assert_eq!(decoded.tip_pull, 0.4);
        assert_eq!(decoded.chromatic_aberration, 0.65);
        assert_eq!(decoded.bevel_radius, 40.0);

        let mut out_of_range = request;
        out_of_range.blob_amount = f32::NAN;
        out_of_range.blob_seed = -1.0;
        out_of_range.magnification = 5.0;
        out_of_range.tip_pull = -0.5;
        out_of_range.chromatic_aberration = 5.0;
        out_of_range.bevel_radius = f32::NAN;
        let clamped = MaterialRequest::decode(&packet(out_of_range)).unwrap();
        assert_eq!(clamped.blob_amount, 0.0);
        assert_eq!(clamped.blob_seed, 0.0);
        assert_eq!(clamped.magnification, 1.0);
        assert_eq!(clamped.tip_pull, 0.0);
        assert_eq!(clamped.chromatic_aberration, 1.0);
        assert_eq!(clamped.bevel_radius, 28.0);
    }

    #[test]
    fn decoder_rejects_wrong_schema_headers() {
        let request = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 10.0, 10.0]);
        let mut bytes = packet(request);
        bytes[0] = b'X';
        assert_eq!(MaterialRequest::decode(&bytes), Err(MaterialDecodeError::InvalidMagic));
        bytes[0] = b'A';
        bytes[4] = 99;
        assert_eq!(MaterialRequest::decode(&bytes), Err(MaterialDecodeError::UnsupportedVersion));
    }

    #[test]
    fn planning_skips_zero_size_and_falls_back_without_backdrop_sampling() {
        let zero = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 0.0, 10.0]);
        assert_eq!(
            plan_material(zero, MaterialCapabilities::default()).path,
            MaterialRenderPath::Skip
        );

        let request = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 80.0, 40.0]);
        let capabilities = MaterialCapabilities {
            backdrop_sampling: false,
            ..MaterialCapabilities::default()
        };
        assert_eq!(
            plan_material(request, capabilities).path,
            MaterialRenderPath::Fallback
        );
    }

    #[test]
    fn intermediate_extent_is_downsampled_and_bounded() {
        let request = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 1_000.0, 500.0]);
        let mut low = request;
        low.quality = 1;
        assert_eq!(
            bounded_intermediate_extent(low, MaterialCapabilities::default()),
            Some(IntermediateExtent {
                width: 250,
                height: 125,
                bytes: 125_000,
            })
        );

        let huge = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 100_000.0, 100_000.0]);
        assert!(bounded_intermediate_extent(huge, MaterialCapabilities::default()).is_none());
    }

    #[test]
    fn liquid_distortion_requires_capability_but_glass_does_not() {
        let capabilities = MaterialCapabilities {
            distortion_sampling: false,
            ..MaterialCapabilities::default()
        };
        let liquid = MaterialRequest::new(MaterialKind::Liquid, [0.0, 0.0, 20.0, 20.0]);
        assert_eq!(plan_material(liquid, capabilities).path, MaterialRenderPath::Fallback);
        let glass = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 20.0, 20.0]);
        assert_eq!(plan_material(glass, capabilities).path, MaterialRenderPath::Gpu);
    }

    #[test]
    fn material_planning_reports_why_the_explicit_fallback_was_selected() {
        let request = MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 20.0, 20.0]);
        let capabilities = MaterialCapabilities {
            backdrop_sampling: false,
            ..MaterialCapabilities::default()
        };

        let plan = plan_material(request, capabilities);

        assert_eq!(plan.path, MaterialRenderPath::Fallback);
        assert_eq!(
            plan.fallback_reason,
            Some(MaterialFallbackReason::BackdropSamplingUnavailable)
        );
    }

    #[test]
    fn material_planning_reports_an_intermediate_budget_fallback() {
        let request = MaterialRequest::new(MaterialKind::Liquid, [0.0, 0.0, 200.0, 100.0]);
        let capabilities = MaterialCapabilities {
            max_intermediate_pixels: 1,
            ..MaterialCapabilities::default()
        };

        let plan = plan_material(request, capabilities);

        assert_eq!(plan.path, MaterialRenderPath::Fallback);
        assert_eq!(
            plan.fallback_reason,
            Some(MaterialFallbackReason::IntermediateBudgetExceeded)
        );
    }

    #[test]
    fn fallback_multiplies_alpha_and_drops_dynamic_work() {
        let mut request = MaterialRequest::new(MaterialKind::Liquid, [0.0, 0.0, 10.0, 10.0]);
        request.opacity = 0.5;
        request.tint = [0.2, 0.3, 0.4, 0.8];
        let fallback = request.fallback();
        assert_eq!(fallback.fill_color, [0.2, 0.3, 0.4, 0.4]);
        assert_eq!(fallback.z_order, 0);
    }

    #[test]
    fn equal_z_order_is_stable_and_different_orders_are_sorted() {
        let mut requests = [
            MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 1.0, 1.0]),
            MaterialRequest::new(MaterialKind::Glass, [0.0, 0.0, 1.0, 1.0]),
            MaterialRequest::new(MaterialKind::Liquid, [0.0, 0.0, 1.0, 1.0]),
        ];
        requests[0].z_order = 3;
        requests[1].z_order = 1;
        requests[2].z_order = 2;
        order_requests(&mut requests);
        assert_eq!(requests[0].z_order, 1);
        assert_eq!(requests[1].z_order, 2);
        assert_eq!(requests[2].z_order, 3);
    }

    #[test]
    fn shader_contains_the_material_entry_points() {
        let source = MaterialShader::source();
        assert!(source.contains("@vertex"));
        assert!(source.contains("@fragment"));
        assert!(source.contains("backdrop"));
    }

    #[test]
    fn material_shader_contains_both_reference_surface_layers() {
        let source = MaterialShader::source();

        assert!(source.contains("glass_glow"));
        assert!(source.contains("glass_rim"));
        assert!(source.contains("liquid_shape_distance"));
        assert!(source.contains("liquid_rim"));
        assert!(source.contains("liquid_sheen"));
        assert!(source.contains("liquid_bevel_normal"));
        assert!(source.contains("liquid_bevel_refract_offset"));
        assert!(source.contains("liquid_shape_normal"));
        assert!(source.contains("liquid_chromatic_sample"));
        assert!(source.contains("adaptive_tint"));
    }

    #[test]
    fn glass_shader_uses_the_public_lighting_controls_and_user_tint() {
        let source = MaterialShader::source();

        assert!(source.contains("glass_frosted_base"));
        assert!(source.contains("material.tint.rgb"));
        assert!(source.contains("material.detail.x"));
        assert!(source.contains("material.light.w"));
        assert!(source.contains("let edge_color"));
    }

    #[test]
    fn material_shader_samples_a_pane_local_screen_space_backdrop() {
        let source = MaterialShader::source();

        assert!(source.contains("sample_backdrop"));
        assert!(source.contains("frosted_backdrop"));
        assert!(source.contains("input.pixel_pos"));
        assert!(source.contains("material.viewport.w"));
        assert!(source.contains("material.backdrop_rect.xy"));
        assert!(source.contains("material.backdrop_rect.zw"));
    }

    #[test]
    fn glass_shader_uses_a_bounded_nine_tap_frosted_kernel() {
        let source = MaterialShader::source();

        let kernel = source
            .split("fn frosted_backdrop")
            .nth(1)
            .and_then(|source| source.split("fn backdrop_to_material_rgb").next())
            .expect("frosted backdrop function");
        let frosted_path = kernel
            .split("let reed")
            .nth(1)
            .expect("non-zero frosted path");
        assert_eq!(frosted_path.matches("sample_backdrop(").count(), 9);
        assert!(kernel.contains("reed"));
        assert!(kernel.contains("ripple"));
    }

    #[test]
    fn glass_shader_skips_unneeded_backdrop_reads() {
        let source = MaterialShader::source();
        let kernel = source
            .split("fn frosted_backdrop")
            .nth(1)
            .and_then(|source| source.split("fn backdrop_to_material_rgb").next())
            .expect("frosted backdrop function");
        let fragment = source
            .split("fn fs_main")
            .nth(1)
            .expect("fragment function");

        assert!(kernel.contains("if radius <= 0.0"));
        let liquid_branch = fragment
            .find("if material.effect.x > 0.5")
            .expect("liquid-only branch");
        let refracted_sample = fragment
            .find("let refracted")
            .expect("liquid refraction sample");
        assert!(liquid_branch < refracted_sample);
    }

    #[test]
    fn material_shader_parses_and_validates() {
        use naga::valid::{Capabilities, ValidationFlags, Validator};

        let module = naga::front::wgsl::parse_str(MaterialShader::source())
            .expect("the material WGSL should parse");
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .expect("the material WGSL should validate");
    }
}
