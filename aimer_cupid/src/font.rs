//! Deterministic font-family registration shared by Aimer styles and Cupid.
//!
//! Register immutable font bytes before `AimerApp::start`. Generic sans-serif
//! and monospace handles are always available without operating-system lookup.
//! Resolution prefers the requested style, then the nearest numeric weight;
//! normal style is the deterministic fallback when the requested style is not
//! registered. Cupid retains the selected family where it has glyphs and uses
//! its existing Unicode fallback chain only for missing glyphs.

use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
    ObliqueDeg(i32),
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq)]
pub enum FontWeight {
    VeryThin,
    Thin,
    #[default]
    Normal,
    Bold,
    Bolder,
    Value(u32),
}

impl FontWeight {
    /// Numeric CSS-style weight (100–900). 400 is normal, 700 is bold.
    pub fn numeric(self) -> u16 {
        match self {
            Self::VeryThin => 100,
            Self::Thin => 300,
            Self::Normal => 400,
            Self::Bold => 700,
            Self::Bolder => 900,
            Self::Value(value) => value.clamp(1, 1000) as u16,
        }
    }
}

/// A lightweight, process-stable handle to a generic or registered font family.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FontFamily(u64);

impl FontFamily {
    /// Aimer's bundled sans-serif family.
    pub const SANS_SERIF: Self = Self(0);
    /// Aimer's bundled monospace family.
    pub const MONOSPACE: Self = Self(1);

    #[doc(hidden)]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl Default for FontFamily {
    fn default() -> Self {
        Self::SANS_SERIF
    }
}

#[doc(hidden)]
pub const fn bundled_monospace_bytes() -> &'static [u8] {
    include_bytes!("../fonts/JetBrainsMono-Regular.ttf")
}

#[derive(Clone, Copy)]
pub struct FontRegistration<'a> {
    pub family: &'a str,
    pub bytes: &'a [u8],
    pub weight: FontWeight,
    pub style: FontStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    EmptyFamily,
    InvalidFont,
    ReservedFamily,
    DuplicateVariant {
        family: FontFamily,
        weight: u16,
        style: FontStyle,
    },
    HandleCollision,
}

impl Display for FontError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFamily => formatter.write_str("font family name cannot be empty"),
            Self::InvalidFont => {
                formatter.write_str("font bytes are empty, invalid, or unsupported")
            }
            Self::ReservedFamily => {
                formatter.write_str("generic Aimer font family names are reserved")
            }
            Self::DuplicateVariant { weight, style, .. } => {
                write!(
                    formatter,
                    "font variant {weight}/{style:?} is already registered"
                )
            }
            Self::HandleCollision => formatter.write_str("font family or face handle collision"),
        }
    }
}

impl std::error::Error for FontError {}

#[derive(Clone)]
#[doc(hidden)]
pub struct RegisteredFontFace {
    pub family: FontFamily,
    pub face_id: u32,
    pub bytes: Arc<[u8]>,
    pub weight: u16,
    pub style: FontStyle,
}

#[derive(Default)]
struct RegistryState {
    names: HashMap<String, FontFamily>,
    family_names: HashMap<FontFamily, String>,
    faces: HashMap<FontFamily, Vec<RegisteredFontFace>>,
    face_owners: HashMap<u32, (FontFamily, u16, FontStyle)>,
}

fn registry() -> &'static RwLock<RegistryState> {
    static REGISTRY: OnceLock<RwLock<RegistryState>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(RegistryState::default()))
}

fn normalize_family(name: &str) -> Result<String, FontError> {
    let normalized = name.trim().to_lowercase();
    if normalized.is_empty() {
        Err(FontError::EmptyFamily)
    } else if matches!(normalized.as_str(), "sans-serif" | "monospace") {
        Err(FontError::ReservedFamily)
    } else {
        Ok(normalized)
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    bytes
        .iter()
        .fold(OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
        })
}

fn family_handle(name: &str) -> FontFamily {
    FontFamily(stable_hash(name.as_bytes()) | (1 << 63))
}

fn face_id(family: FontFamily, weight: u16, style: FontStyle) -> u32 {
    let key = format!("{}:{weight}:{style:?}", family.raw());
    0x8000_0000 | (stable_hash(key.as_bytes()) as u32 & 0x7fff_ffff)
}

fn style_distance(requested: FontStyle, candidate: FontStyle) -> u8 {
    if requested == candidate {
        0
    } else if candidate == FontStyle::Normal {
        1
    } else if matches!(requested, FontStyle::Oblique | FontStyle::ObliqueDeg(_))
        && matches!(candidate, FontStyle::Oblique | FontStyle::ObliqueDeg(_))
    {
        2
    } else {
        3
    }
}

pub struct FontRegistry;

impl FontRegistry {
    /// Validates and registers one immutable family variant.
    ///
    /// Registration is intended to finish before `AimerApp::start`. Registering
    /// the same normalized family, numeric weight, and style twice is rejected.
    pub fn register(registration: FontRegistration<'_>) -> Result<FontFamily, FontError> {
        let family_name = normalize_family(registration.family)?;
        if registration.bytes.is_empty() || ttf_parser::Face::parse(registration.bytes, 0).is_err()
        {
            return Err(FontError::InvalidFont);
        }

        let family = family_handle(&family_name);
        let weight = registration.weight.numeric();
        let id = face_id(family, weight, registration.style);
        let mut state = registry()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(existing_name) = state
            .family_names
            .get(&family)
            && existing_name != &family_name
        {
            return Err(FontError::HandleCollision);
        }
        if let Some(owner) = state.face_owners.get(&id)
            && *owner != (family, weight, registration.style)
        {
            return Err(FontError::HandleCollision);
        }
        if state
            .faces
            .get(&family)
            .is_some_and(|faces| {
                faces
                    .iter()
                    .any(|face| face.weight == weight && face.style == registration.style)
            })
        {
            return Err(FontError::DuplicateVariant {
                family,
                weight,
                style: registration.style,
            });
        }

        state
            .names
            .insert(family_name.clone(), family);
        state
            .family_names
            .insert(family, family_name);
        state
            .face_owners
            .insert(id, (family, weight, registration.style));
        state
            .faces
            .entry(family)
            .or_default()
            .push(RegisteredFontFace {
                family,
                face_id: id,
                bytes: Arc::from(registration.bytes),
                weight,
                style: registration.style,
            });
        Ok(family)
    }

    pub fn family(name: &str) -> Option<FontFamily> {
        let normalized = name.trim().to_lowercase();
        match normalized.as_str() {
            "sans-serif" => Some(FontFamily::SANS_SERIF),
            "monospace" => Some(FontFamily::MONOSPACE),
            _ => registry()
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .names
                .get(&normalized)
                .copied(),
        }
    }

    #[doc(hidden)]
    pub fn resolve(
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<RegisteredFontFace> {
        let numeric_weight = weight.numeric();
        registry()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .faces
            .get(&family)?
            .iter()
            .min_by_key(|face| {
                (
                    style_distance(style, face.style),
                    face.weight
                        .abs_diff(numeric_weight),
                    face.weight,
                    face.face_id,
                )
            })
            .cloned()
    }

    #[doc(hidden)]
    pub fn faces() -> Vec<RegisteredFontFace> {
        let mut faces: Vec<_> = registry()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .faces
            .values()
            .flatten()
            .cloned()
            .collect();
        faces.sort_by_key(|face| face.face_id);
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FONT: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");

    #[test]
    fn nearest_variant_prefers_style_then_weight_deterministically() {
        let family = FontRegistry::register(FontRegistration {
            family: "aimer-font-nearest-test",
            bytes: TEST_FONT,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
        })
        .unwrap();
        FontRegistry::register(FontRegistration {
            family: "aimer-font-nearest-test",
            bytes: TEST_FONT,
            weight: FontWeight::Bold,
            style: FontStyle::Normal,
        })
        .unwrap();
        FontRegistry::register(FontRegistration {
            family: "aimer-font-nearest-test",
            bytes: TEST_FONT,
            weight: FontWeight::Normal,
            style: FontStyle::Italic,
        })
        .unwrap();

        let exact = FontRegistry::resolve(family, FontWeight::Bold, FontStyle::Normal).unwrap();
        assert_eq!((exact.weight, exact.style), (700, FontStyle::Normal));

        let nearest_weight =
            FontRegistry::resolve(family, FontWeight::Value(600), FontStyle::Normal).unwrap();
        assert_eq!(nearest_weight.weight, 700);

        let exact_style =
            FontRegistry::resolve(family, FontWeight::Bold, FontStyle::Italic).unwrap();
        assert_eq!(
            (exact_style.weight, exact_style.style),
            (400, FontStyle::Italic)
        );

        let normal_style_fallback =
            FontRegistry::resolve(family, FontWeight::Bold, FontStyle::Oblique).unwrap();
        assert_eq!(
            (normal_style_fallback.weight, normal_style_fallback.style),
            (700, FontStyle::Normal)
        );
    }
}
