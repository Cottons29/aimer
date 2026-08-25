use aimer_widget::portable::__anteros::{
    BOX_DECORATION_VALUE_MAXIMUM_ENCODED_BYTES, BOX_DECORATION_VALUE_NAME,
    BOX_DECORATION_VALUE_VERSION, LAYOUT_SPACING_VALUE_MAXIMUM_ENCODED_BYTES,
    LAYOUT_SPACING_VALUE_NAME, LAYOUT_SPACING_VALUE_VERSION,
    LINE_HEIGHT_VALUE_MAXIMUM_ENCODED_BYTES, LINE_HEIGHT_VALUE_NAME,
    LINE_HEIGHT_VALUE_VERSION,
    TEXT_STYLE_VALUE_MAXIMUM_ENCODED_BYTES, TEXT_STYLE_VALUE_NAME, TEXT_STYLE_VALUE_VERSION,
    PropertyId, PropertyValue, ValueSchemaMetadata, WidgetDocumentView,
};
use aimer_widget::portable::{
    PortableMaterializeError, PortableMaterializeProperty, PortableProperty,
    PortablePropertyConversion, PortablePropertyReflection,
};
#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{
    PortableBuildContext, PortableBuildError, PortableEncodeProperty, PortableWidgetResource,
};

use super::border::{BorderSlice, BorderStyle, BoxBorder, BoxOutline};
use super::box_decoration::BoxDecoration;
use super::box_decoration::border_radius::BorderRadius;
use super::box_decoration::box_shadow::{BoxShadow, ShadowSide};
use super::layout_spacing::{LayoutSpacing, Spacing};
use super::text_style::{
    FontFamily, FontStyle, FontWeight, LineHeight, TextAlign, TextDecoration,
    TextDecorationLine, TextDecorationStyle, TextOverflow, TextShadow, TextStyle, TextTransform,
};
use aimer_attribute::Dimension;
use aimer_color::prelude::Color;

impl PortableProperty for TextAlign {
    const REFLECTION: PortablePropertyReflection = PortablePropertyReflection::new(
        aimer_widget::portable::__anteros::PropertyValueKind::I64,
        PortablePropertyConversion::SignedInteger {
            minimum: 0,
            maximum: 8,
        },
    );
}

impl PortableMaterializeProperty for TextAlign {
    fn from_awir(
        _document: &WidgetDocumentView<'_>,
        property: PropertyId,
        value: PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        let PropertyValue::I64(value) = value else {
            return Err(PortableMaterializeError::InvalidPropertyType { property });
        };
        match value {
            0 => Ok(Self::TopLeft),
            1 => Ok(Self::TopCenter),
            2 => Ok(Self::TopRight),
            3 => Ok(Self::MidCenter),
            4 => Ok(Self::MidLeft),
            5 => Ok(Self::MidRight),
            6 => Ok(Self::BotLeft),
            7 => Ok(Self::BotCenter),
            8 => Ok(Self::BotRight),
            _ => Err(PortableMaterializeError::InvalidPropertyValue { property }),
        }
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for TextAlign {
    fn encode_property(
        self,
        _context: &mut PortableBuildContext,
    ) -> Result<PropertyValue, PortableBuildError> {
        Ok(PropertyValue::I64(match self {
            Self::TopLeft => 0,
            Self::TopCenter => 1,
            Self::TopRight => 2,
            Self::MidCenter => 3,
            Self::MidLeft => 4,
            Self::MidRight => 5,
            Self::BotLeft => 6,
            Self::BotCenter => 7,
            Self::BotRight => 8,
        }))
    }
}

// These bytes are part of the value codec contracts. A decoder must reject a
// different version until an explicit migration path is added.
const LAYOUT_SPACING_WIRE_VERSION: u8 = 1;
const BOX_DECORATION_WIRE_VERSION: u8 = 1;
const TEXT_STYLE_WIRE_VERSION: u8 = 2;
const LINE_HEIGHT_WIRE_VERSION: u8 = 1;

// Canonical enum tags. Do not renumber an existing tag; append a new tag and
// advance the value codec version when the old meaning cannot be preserved.
const SPACING_NONE_TAG: u8 = 0;
const SPACING_PX_TAG: u8 = 1;
const SPACING_PERCENT_TAG: u8 = 2;
const BORDER_STYLE_NONE_TAG: u8 = 0;
const BORDER_STYLE_SOLID_TAG: u8 = 1;
const BORDER_STYLE_DASHED_TAG: u8 = 2;
const BORDER_STYLE_DOTTED_TAG: u8 = 3;
const DIMENSION_AUTO_TAG: u8 = 0;
const DIMENSION_PX_TAG: u8 = 1;
const DIMENSION_PERCENT_TAG: u8 = 2;
const SHADOW_SIDE_ALL_TAG: u8 = 0;
const SHADOW_SIDE_TOP_TAG: u8 = 1;
const SHADOW_SIDE_RIGHT_TAG: u8 = 2;
const SHADOW_SIDE_BOTTOM_TAG: u8 = 3;
const SHADOW_SIDE_LEFT_TAG: u8 = 4;
const SHADOW_SIDE_VERTICAL_TAG: u8 = 5;
const SHADOW_SIDE_HORIZONTAL_TAG: u8 = 6;
const SHADOW_SIDE_RANGE_TAG: u8 = 7;
const SHADOW_SIDE_TOP_LEFT_TAG: u8 = 8;
const SHADOW_SIDE_TOP_RIGHT_TAG: u8 = 9;
const SHADOW_SIDE_BOTTOM_RIGHT_TAG: u8 = 10;
const SHADOW_SIDE_BOTTOM_LEFT_TAG: u8 = 11;
const OPTIONAL_NONE_TAG: u8 = 0;
const OPTIONAL_SOME_TAG: u8 = 1;
const BOOLEAN_FALSE_TAG: u8 = 0;
const BOOLEAN_TRUE_TAG: u8 = 1;
const FONT_STYLE_NORMAL_TAG: u8 = 0;
const FONT_STYLE_ITALIC_TAG: u8 = 1;
const FONT_STYLE_OBLIQUE_TAG: u8 = 2;
const FONT_STYLE_OBLIQUE_DEG_TAG: u8 = 3;
const FONT_WEIGHT_VERY_THIN_TAG: u8 = 0;
const FONT_WEIGHT_THIN_TAG: u8 = 1;
const FONT_WEIGHT_NORMAL_TAG: u8 = 2;
const FONT_WEIGHT_BOLD_TAG: u8 = 3;
const FONT_WEIGHT_BOLDER_TAG: u8 = 4;
const FONT_WEIGHT_VALUE_TAG: u8 = 5;
const TEXT_OVERFLOW_CLIP_TAG: u8 = 0;
const TEXT_OVERFLOW_ELLIPSIS_TAG: u8 = 1;
const TEXT_OVERFLOW_WRAP_TAG: u8 = 2;
const TEXT_OVERFLOW_VALUE_TAG: u8 = 3;
const TEXT_DECORATION_SOLID_TAG: u8 = 0;
const TEXT_DECORATION_DOUBLE_TAG: u8 = 1;
const TEXT_DECORATION_DOTTED_TAG: u8 = 2;
const TEXT_DECORATION_DASHED_TAG: u8 = 3;
const TEXT_DECORATION_WAVY_TAG: u8 = 4;
const TEXT_TRANSFORM_NONE_TAG: u8 = 0;
const TEXT_TRANSFORM_UPPERCASE_TAG: u8 = 1;
const TEXT_TRANSFORM_LOWERCASE_TAG: u8 = 2;
const TEXT_TRANSFORM_CAPITALIZE_TAG: u8 = 3;
const LINE_HEIGHT_NORMAL_TAG: u8 = 0;
const LINE_HEIGHT_PX_TAG: u8 = 1;
const LINE_HEIGHT_FACTOR_TAG: u8 = 2;

impl PortableProperty for LayoutSpacing {
    const REFLECTION: PortablePropertyReflection =
        PortablePropertyReflection::custom(ValueSchemaMetadata::from_canonical_name(
            LAYOUT_SPACING_VALUE_NAME,
            LAYOUT_SPACING_VALUE_VERSION,
            LAYOUT_SPACING_VALUE_MAXIMUM_ENCODED_BYTES,
        ));
}

impl PortableMaterializeProperty for LayoutSpacing {
    /// Decodes four canonical spacing values in top, bottom, left, right order.
    fn from_awir(
        document: &WidgetDocumentView<'_>,
        property: PropertyId,
        value: PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        let blob = property_blob(document, property, value)?;
        let mut reader = WireReader::new(blob, property);
        reader.require_version(LAYOUT_SPACING_WIRE_VERSION)?;
        let spacing = Self {
            top: reader.spacing()?,
            bottom: reader.spacing()?,
            left: reader.spacing()?,
            right: reader.spacing()?,
        };
        reader.finish()?;
        Ok(spacing)
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for LayoutSpacing {
    /// Encodes four stable spacing records in top, bottom, left, right order.
    fn encode_property(
        self,
        context: &mut PortableBuildContext,
    ) -> Result<PropertyValue, PortableBuildError> {
        let mut bytes = Vec::with_capacity(LAYOUT_SPACING_VALUE_MAXIMUM_ENCODED_BYTES as usize);
        bytes.push(LAYOUT_SPACING_WIRE_VERSION);
        push_spacing(&mut bytes, self.top);
        push_spacing(&mut bytes, self.bottom);
        push_spacing(&mut bytes, self.left);
        push_spacing(&mut bytes, self.right);
        context.push_owned_blob(bytes)
    }
}

#[cfg(feature = "portable-guest")]
fn push_spacing(bytes: &mut Vec<u8>, spacing: Spacing) {
    match spacing {
        Spacing::None => {
            bytes.push(SPACING_NONE_TAG);
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        Spacing::Px(value) => {
            bytes.push(SPACING_PX_TAG);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Spacing::Percent(value) => {
            bytes.push(SPACING_PERCENT_TAG);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

impl PortableProperty for TextStyle {
    const REFLECTION: PortablePropertyReflection =
        PortablePropertyReflection::custom(ValueSchemaMetadata::from_canonical_name(
            TEXT_STYLE_VALUE_NAME,
            TEXT_STYLE_VALUE_VERSION,
            TEXT_STYLE_VALUE_MAXIMUM_ENCODED_BYTES,
        ));
}

impl PortableMaterializeProperty for TextStyle {
    /// Decodes the version-one or version-two text style from one bounded AWIR
    /// blob. Version one remains accepted with the new properties at defaults.
    fn from_awir(
        document: &WidgetDocumentView<'_>,
        property: PropertyId,
        value: PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        let blob = property_blob(document, property, value)?;
        let mut reader = WireReader::new(blob, property);
        let version = reader.u8()?;
        if version != 1 && version != TEXT_STYLE_WIRE_VERSION {
            return Err(reader.invalid_value());
        }
        let style = TextStyle {
            font_size: reader.u32()?,
            font_family: FontFamily::from_raw(reader.u64()?),
            font_style: reader.font_style()?,
            font_weight: reader.font_weight()?,
            color: reader.color()?,
            background_color: reader.optional_color()?,
            text_overflow: reader.text_overflow()?,
            text_decoration: reader.text_decoration()?,
            text_transform: if version == 1 {
                TextTransform::None
            } else {
                reader.text_transform()?
            },
            letter_spacing: if version == 1 { 0.0 } else { reader.f32()? },
            word_spacing: if version == 1 { 0.0 } else { reader.f32()? },
            text_shadow: if version == 1 {
                None
            } else {
                reader.optional_text_shadow()?
            },
        };
        reader.finish()?;
        Ok(style)
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for TextStyle {
    /// Encodes all text metrics, paint, overflow, and decoration fields in a
    /// bounded, versioned payload.
    fn encode_property(
        self,
        context: &mut PortableBuildContext,
    ) -> Result<PropertyValue, PortableBuildError> {
        validate_text_style(&self)?;

        let mut bytes = Vec::with_capacity(TEXT_STYLE_VALUE_MAXIMUM_ENCODED_BYTES as usize);
        bytes.push(TEXT_STYLE_WIRE_VERSION);
        bytes.extend_from_slice(&self.font_size.to_le_bytes());
        bytes.extend_from_slice(&self.font_family.raw().to_le_bytes());
        push_font_style(&mut bytes, self.font_style);
        push_font_weight(&mut bytes, self.font_weight);
        push_color(&mut bytes, self.color);
        push_optional_color(&mut bytes, self.background_color);
        push_text_overflow(&mut bytes, self.text_overflow);
        bytes.push(self.text_decoration.line.bits());
        push_text_decoration_style(&mut bytes, self.text_decoration.style);
        push_optional_color(&mut bytes, self.text_decoration.color);
        push_optional_f32(&mut bytes, self.text_decoration.thickness);
        push_f32(&mut bytes, self.text_decoration.offset);
        push_text_transform(&mut bytes, self.text_transform);
        push_f32(&mut bytes, self.letter_spacing);
        push_f32(&mut bytes, self.word_spacing);
        push_optional_text_shadow(&mut bytes, self.text_shadow);
        context.push_owned_blob(bytes)
    }
}

#[cfg(feature = "portable-guest")]
fn validate_text_style(style: &TextStyle) -> Result<(), PortableBuildError> {
    if let Some(thickness) = style.text_decoration.thickness {
        validate_finite(thickness)?;
    }
    validate_finite(style.text_decoration.offset)?;
    validate_finite(style.letter_spacing)?;
    validate_finite(style.word_spacing)?;
    if let Some(shadow) = style.text_shadow {
        validate_finite(shadow.offset_x)?;
        validate_finite(shadow.offset_y)?;
        validate_finite(shadow.blur)?;
        if shadow.blur < 0.0 {
            return Err(PortableBuildError::InvalidPropertyValue {
                rust_type: "TextStyle::text_shadow.blur",
            });
        }
    }
    Ok(())
}

impl PortableProperty for LineHeight {
    const REFLECTION: PortablePropertyReflection =
        PortablePropertyReflection::custom(ValueSchemaMetadata::from_canonical_name(
            LINE_HEIGHT_VALUE_NAME,
            LINE_HEIGHT_VALUE_VERSION,
            LINE_HEIGHT_VALUE_MAXIMUM_ENCODED_BYTES,
        ));
}

impl PortableMaterializeProperty for LineHeight {
    fn from_awir(
        document: &WidgetDocumentView<'_>,
        property: PropertyId,
        value: PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        let blob = property_blob(document, property, value)?;
        let mut reader = WireReader::new(blob, property);
        reader.require_version(LINE_HEIGHT_WIRE_VERSION)?;
        let line_height = match reader.u8()? {
            LINE_HEIGHT_NORMAL_TAG => Self::Normal,
            LINE_HEIGHT_PX_TAG => {
                let value = reader.f32()?;
                if value <= 0.0 {
                    return Err(reader.invalid_value());
                }
                Self::Px(value)
            }
            LINE_HEIGHT_FACTOR_TAG => {
                let value = reader.f32()?;
                if value <= 0.0 {
                    return Err(reader.invalid_value());
                }
                Self::Factor(value)
            }
            _ => return Err(reader.invalid_value()),
        };
        reader.finish()?;
        Ok(line_height)
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for LineHeight {
    fn encode_property(
        self,
        context: &mut PortableBuildContext,
    ) -> Result<PropertyValue, PortableBuildError> {
        let mut bytes = Vec::with_capacity(LINE_HEIGHT_VALUE_MAXIMUM_ENCODED_BYTES as usize);
        bytes.push(LINE_HEIGHT_WIRE_VERSION);
        match self {
            Self::Normal => bytes.push(LINE_HEIGHT_NORMAL_TAG),
            Self::Px(value) => {
                validate_positive_finite(value)?;
                bytes.push(LINE_HEIGHT_PX_TAG);
                push_f32(&mut bytes, value);
            }
            Self::Factor(value) => {
                validate_positive_finite(value)?;
                bytes.push(LINE_HEIGHT_FACTOR_TAG);
                push_f32(&mut bytes, value);
            }
        }
        context.push_owned_blob(bytes)
    }
}

#[cfg(feature = "portable-guest")]
fn validate_positive_finite(value: f32) -> Result<(), PortableBuildError> {
    validate_finite(value)?;
    if value > 0.0 {
        Ok(())
    } else {
        Err(PortableBuildError::InvalidPropertyValue {
            rust_type: "aimer_style::LineHeight",
        })
    }
}

#[cfg(feature = "portable-guest")]
fn push_text_transform(bytes: &mut Vec<u8>, transform: TextTransform) {
    bytes.push(match transform {
        TextTransform::None => TEXT_TRANSFORM_NONE_TAG,
        TextTransform::Uppercase => TEXT_TRANSFORM_UPPERCASE_TAG,
        TextTransform::Lowercase => TEXT_TRANSFORM_LOWERCASE_TAG,
        TextTransform::Capitalize => TEXT_TRANSFORM_CAPITALIZE_TAG,
    });
}

#[cfg(feature = "portable-guest")]
fn push_optional_text_shadow(bytes: &mut Vec<u8>, shadow: Option<TextShadow>) {
    match shadow {
        None => bytes.push(OPTIONAL_NONE_TAG),
        Some(shadow) => {
            bytes.push(OPTIONAL_SOME_TAG);
            push_f32(bytes, shadow.offset_x);
            push_f32(bytes, shadow.offset_y);
            push_f32(bytes, shadow.blur);
            push_color(bytes, shadow.color);
        }
    }
}

#[cfg(feature = "portable-guest")]
fn push_font_style(bytes: &mut Vec<u8>, style: FontStyle) {
    match style {
        FontStyle::Normal => bytes.push(FONT_STYLE_NORMAL_TAG),
        FontStyle::Italic => bytes.push(FONT_STYLE_ITALIC_TAG),
        FontStyle::Oblique => bytes.push(FONT_STYLE_OBLIQUE_TAG),
        FontStyle::ObliqueDeg(degrees) => {
            bytes.push(FONT_STYLE_OBLIQUE_DEG_TAG);
            bytes.extend_from_slice(&degrees.to_le_bytes());
        }
    }
}

#[cfg(feature = "portable-guest")]
fn push_font_weight(bytes: &mut Vec<u8>, weight: FontWeight) {
    match weight {
        FontWeight::VeryThin => bytes.push(FONT_WEIGHT_VERY_THIN_TAG),
        FontWeight::Thin => bytes.push(FONT_WEIGHT_THIN_TAG),
        FontWeight::Normal => bytes.push(FONT_WEIGHT_NORMAL_TAG),
        FontWeight::Bold => bytes.push(FONT_WEIGHT_BOLD_TAG),
        FontWeight::Bolder => bytes.push(FONT_WEIGHT_BOLDER_TAG),
        FontWeight::Value(value) => {
            bytes.push(FONT_WEIGHT_VALUE_TAG);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

#[cfg(feature = "portable-guest")]
fn push_text_overflow(bytes: &mut Vec<u8>, overflow: TextOverflow) {
    match overflow {
        TextOverflow::Clip => bytes.push(TEXT_OVERFLOW_CLIP_TAG),
        TextOverflow::Ellipsis => bytes.push(TEXT_OVERFLOW_ELLIPSIS_TAG),
        TextOverflow::Wrap => bytes.push(TEXT_OVERFLOW_WRAP_TAG),
        TextOverflow::Value(value) => {
            bytes.push(TEXT_OVERFLOW_VALUE_TAG);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

#[cfg(feature = "portable-guest")]
fn push_text_decoration_style(bytes: &mut Vec<u8>, style: TextDecorationStyle) {
    bytes.push(match style {
        TextDecorationStyle::Solid => TEXT_DECORATION_SOLID_TAG,
        TextDecorationStyle::Double => TEXT_DECORATION_DOUBLE_TAG,
        TextDecorationStyle::Dotted => TEXT_DECORATION_DOTTED_TAG,
        TextDecorationStyle::Dashed => TEXT_DECORATION_DASHED_TAG,
        TextDecorationStyle::Wavy => TEXT_DECORATION_WAVY_TAG,
    });
}

impl PortableProperty for BoxDecoration {
    const REFLECTION: PortablePropertyReflection =
        PortablePropertyReflection::custom(ValueSchemaMetadata::from_canonical_name(
            BOX_DECORATION_VALUE_NAME,
            BOX_DECORATION_VALUE_VERSION,
            BOX_DECORATION_VALUE_MAXIMUM_ENCODED_BYTES,
        ));
}

impl PortableMaterializeProperty for BoxDecoration {
    /// Decodes the complete version-one decoration from one bounded AWIR blob.
    fn from_awir(
        document: &WidgetDocumentView<'_>,
        property: PropertyId,
        value: PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        let blob = property_blob(document, property, value)?;
        let mut reader = WireReader::new(blob, property);
        reader.require_version(BOX_DECORATION_WIRE_VERSION)?;
        let decoration = Self {
            border: reader.border()?,
            outline: reader.outline()?,
            border_radius: reader.border_radius()?,
            box_shadow: reader.shadows()?,
            background_color: std::cell::Cell::new(reader.optional_color()?),
        };
        reader.finish()?;
        Ok(decoration)
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for BoxDecoration {
    /// Encodes the complete decoration using the version-one blob contract.
    fn encode_property(
        self,
        context: &mut PortableBuildContext,
    ) -> Result<PropertyValue, PortableBuildError> {
        let shadow_count = checked_shadow_count(self.box_shadow.len())?;
        validate_decoration(&self)?;

        let mut bytes = Vec::new();
        bytes.push(BOX_DECORATION_WIRE_VERSION);
        push_border(&mut bytes, &self.border);
        push_outline(&mut bytes, &self.outline);
        push_border_radius(&mut bytes, &self.border_radius);
        bytes.extend_from_slice(&shadow_count.to_le_bytes());
        for shadow in &self.box_shadow {
            push_shadow(&mut bytes, shadow);
        }
        push_optional_color(&mut bytes, self.background_color.get());
        context.push_owned_blob(bytes)
    }
}

#[cfg(feature = "portable-guest")]
fn checked_shadow_count(length: usize) -> Result<u32, PortableBuildError> {
    u32::try_from(length).map_err(|_| PortableBuildError::LengthOverflow {
        resource: PortableWidgetResource::BlobBytes,
        actual: length,
    })
}

#[cfg(feature = "portable-guest")]
fn validate_decoration(decoration: &BoxDecoration) -> Result<(), PortableBuildError> {
    for slice in [
        &decoration.border.left,
        &decoration.border.right,
        &decoration.border.top,
        &decoration.border.bottom,
        &decoration.outline.left,
        &decoration.outline.right,
        &decoration.outline.top,
        &decoration.outline.bottom,
    ] {
        validate_dimension(slice.stroke)?;
    }
    for dimension in [
        decoration.border_radius.top_left,
        decoration.border_radius.top_right,
        decoration.border_radius.bottom_right,
        decoration.border_radius.bottom_left,
    ] {
        validate_dimension(dimension)?;
    }
    for shadow in &decoration.box_shadow {
        for value in [shadow.offset_x, shadow.offset_y, shadow.blur, shadow.spread] {
            validate_finite(value)?;
        }
        if let ShadowSide::Range(start, end) = shadow.side {
            validate_finite(start)?;
            validate_finite(end)?;
        }
    }
    Ok(())
}

#[cfg(feature = "portable-guest")]
fn validate_dimension(dimension: Dimension) -> Result<(), PortableBuildError> {
    match dimension {
        Dimension::Auto => Ok(()),
        Dimension::Px(value) | Dimension::Percent(value) => validate_finite(value),
    }
}

#[cfg(feature = "portable-guest")]
#[inline]
fn validate_finite(value: f32) -> Result<(), PortableBuildError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(PortableBuildError::NonFiniteFloat)
    }
}

#[cfg(feature = "portable-guest")]
fn push_border(bytes: &mut Vec<u8>, border: &BoxBorder) {
    push_border_slice(bytes, &border.left);
    push_border_slice(bytes, &border.right);
    push_border_slice(bytes, &border.top);
    push_border_slice(bytes, &border.bottom);
}

#[cfg(feature = "portable-guest")]
fn push_outline(bytes: &mut Vec<u8>, outline: &BoxOutline) {
    push_border_slice(bytes, &outline.left);
    push_border_slice(bytes, &outline.right);
    push_border_slice(bytes, &outline.top);
    push_border_slice(bytes, &outline.bottom);
}

#[cfg(feature = "portable-guest")]
fn push_border_slice(bytes: &mut Vec<u8>, slice: &BorderSlice) {
    bytes.push(match slice.style {
        BorderStyle::None => BORDER_STYLE_NONE_TAG,
        BorderStyle::Solid => BORDER_STYLE_SOLID_TAG,
        BorderStyle::Dashed => BORDER_STYLE_DASHED_TAG,
        BorderStyle::Dotted => BORDER_STYLE_DOTTED_TAG,
    });
    push_dimension(bytes, slice.stroke);
    push_color(bytes, slice.color);
}

#[cfg(feature = "portable-guest")]
fn push_border_radius(bytes: &mut Vec<u8>, radius: &BorderRadius) {
    push_dimension(bytes, radius.top_left);
    push_dimension(bytes, radius.top_right);
    push_dimension(bytes, radius.bottom_right);
    push_dimension(bytes, radius.bottom_left);
}

#[cfg(feature = "portable-guest")]
fn push_dimension(bytes: &mut Vec<u8>, dimension: Dimension) {
    match dimension {
        Dimension::Auto => {
            bytes.push(DIMENSION_AUTO_TAG);
            push_f32(bytes, 0.0);
        }
        Dimension::Px(value) => {
            bytes.push(DIMENSION_PX_TAG);
            push_f32(bytes, value);
        }
        Dimension::Percent(value) => {
            bytes.push(DIMENSION_PERCENT_TAG);
            push_f32(bytes, value);
        }
    }
}

#[cfg(feature = "portable-guest")]
fn push_shadow(bytes: &mut Vec<u8>, shadow: &BoxShadow) {
    push_f32(bytes, shadow.offset_x);
    push_f32(bytes, shadow.offset_y);
    push_f32(bytes, shadow.blur);
    push_f32(bytes, shadow.spread);
    push_color(bytes, shadow.color);
    bytes.push(u8::from(shadow.inset));
    push_shadow_side(bytes, shadow.side);
}

#[cfg(feature = "portable-guest")]
fn push_shadow_side(bytes: &mut Vec<u8>, side: ShadowSide) {
    match side {
        ShadowSide::All => bytes.push(SHADOW_SIDE_ALL_TAG),
        ShadowSide::Top => bytes.push(SHADOW_SIDE_TOP_TAG),
        ShadowSide::Right => bytes.push(SHADOW_SIDE_RIGHT_TAG),
        ShadowSide::Bottom => bytes.push(SHADOW_SIDE_BOTTOM_TAG),
        ShadowSide::Left => bytes.push(SHADOW_SIDE_LEFT_TAG),
        ShadowSide::Vertical => bytes.push(SHADOW_SIDE_VERTICAL_TAG),
        ShadowSide::Horizontal => bytes.push(SHADOW_SIDE_HORIZONTAL_TAG),
        ShadowSide::Range(start, end) => {
            bytes.push(SHADOW_SIDE_RANGE_TAG);
            push_f32(bytes, start);
            push_f32(bytes, end);
        }
        ShadowSide::TopLeft => bytes.push(SHADOW_SIDE_TOP_LEFT_TAG),
        ShadowSide::TopRight => bytes.push(SHADOW_SIDE_TOP_RIGHT_TAG),
        ShadowSide::BottomRight => bytes.push(SHADOW_SIDE_BOTTOM_RIGHT_TAG),
        ShadowSide::BottomLeft => bytes.push(SHADOW_SIDE_BOTTOM_LEFT_TAG),
    }
}

#[cfg(feature = "portable-guest")]
fn push_optional_color(bytes: &mut Vec<u8>, color: Option<Color>) {
    match color {
        None => bytes.push(OPTIONAL_NONE_TAG),
        Some(color) => {
            bytes.push(OPTIONAL_SOME_TAG);
            push_color(bytes, color);
        }
    }
}

#[cfg(feature = "portable-guest")]
fn push_optional_f32(bytes: &mut Vec<u8>, value: Option<f32>) {
    match value {
        None => bytes.push(OPTIONAL_NONE_TAG),
        Some(value) => {
            bytes.push(OPTIONAL_SOME_TAG);
            push_f32(bytes, value);
        }
    }
}

#[cfg(feature = "portable-guest")]
#[inline]
fn push_color(bytes: &mut Vec<u8>, color: Color) {
    // This custom style codec uses the stable packed Color representation that
    // its host decoder accepts; it is intentionally separate from inline RGBA.
    bytes.extend_from_slice(&color.as_u32().to_le_bytes());
}

#[cfg(feature = "portable-guest")]
#[inline]
fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn property_blob<'a>(
    document: &WidgetDocumentView<'a>,
    property: PropertyId,
    value: PropertyValue,
) -> Result<&'a [u8], PortableMaterializeError> {
    let PropertyValue::BlobRef(index) = value else {
        return Err(PortableMaterializeError::InvalidPropertyType { property });
    };
    document
        .blob(index)
        .ok_or(PortableMaterializeError::InvalidPropertyReference { property, index })
}

struct WireReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
    property: PropertyId,
}

impl<'a> WireReader<'a> {
    #[inline]
    const fn new(bytes: &'a [u8], property: PropertyId) -> Self {
        Self {
            bytes,
            cursor: 0,
            property,
        }
    }

    fn require_version(&mut self, expected: u8) -> Result<(), PortableMaterializeError> {
        if self.u8()? == expected {
            Ok(())
        } else {
            Err(self.invalid_value())
        }
    }

    fn spacing(&mut self) -> Result<Spacing, PortableMaterializeError> {
        let tag = self.u8()?;
        let value = self.u32()?;
        match (tag, value) {
            (SPACING_NONE_TAG, 0) => Ok(Spacing::None),
            (SPACING_PX_TAG, value) => Ok(Spacing::Px(value)),
            (SPACING_PERCENT_TAG, value) => Ok(Spacing::Percent(value)),
            _ => Err(self.invalid_value()),
        }
    }

    fn border(&mut self) -> Result<BoxBorder, PortableMaterializeError> {
        Ok(BoxBorder {
            left: self.border_slice()?,
            right: self.border_slice()?,
            top: self.border_slice()?,
            bottom: self.border_slice()?,
        })
    }

    fn outline(&mut self) -> Result<BoxOutline, PortableMaterializeError> {
        Ok(BoxOutline {
            left: self.border_slice()?,
            right: self.border_slice()?,
            top: self.border_slice()?,
            bottom: self.border_slice()?,
        })
    }

    fn border_slice(&mut self) -> Result<BorderSlice, PortableMaterializeError> {
        Ok(BorderSlice {
            style: match self.u8()? {
                BORDER_STYLE_NONE_TAG => BorderStyle::None,
                BORDER_STYLE_SOLID_TAG => BorderStyle::Solid,
                BORDER_STYLE_DASHED_TAG => BorderStyle::Dashed,
                BORDER_STYLE_DOTTED_TAG => BorderStyle::Dotted,
                _ => return Err(self.invalid_value()),
            },
            stroke: self.dimension()?,
            color: self.color()?,
        })
    }

    fn border_radius(&mut self) -> Result<BorderRadius, PortableMaterializeError> {
        Ok(BorderRadius {
            top_left: self.dimension()?,
            top_right: self.dimension()?,
            bottom_right: self.dimension()?,
            bottom_left: self.dimension()?,
        })
    }

    fn dimension(&mut self) -> Result<Dimension, PortableMaterializeError> {
        let tag = self.u8()?;
        let value = self.f32()?;
        match (tag, value) {
            (DIMENSION_AUTO_TAG, 0.0) => Ok(Dimension::Auto),
            (DIMENSION_PX_TAG, value) => Ok(Dimension::Px(value)),
            (DIMENSION_PERCENT_TAG, value) => Ok(Dimension::Percent(value)),
            _ => Err(self.invalid_value()),
        }
    }

    fn shadows(&mut self) -> Result<Vec<BoxShadow>, PortableMaterializeError> {
        const MINIMUM_SHADOW_BYTES: usize = 22;

        let count = usize::try_from(self.u32()?).map_err(|_| self.invalid_value())?;
        if count > self.remaining() / MINIMUM_SHADOW_BYTES {
            return Err(self.invalid_value());
        }
        let mut shadows = Vec::with_capacity(count);
        for _ in 0..count {
            shadows.push(self.shadow()?);
        }
        Ok(shadows)
    }

    fn shadow(&mut self) -> Result<BoxShadow, PortableMaterializeError> {
        Ok(BoxShadow {
            offset_x: self.f32()?,
            offset_y: self.f32()?,
            blur: self.f32()?,
            spread: self.f32()?,
            color: self.color()?,
            inset: self.boolean()?,
            side: self.shadow_side()?,
        })
    }

    fn shadow_side(&mut self) -> Result<ShadowSide, PortableMaterializeError> {
        match self.u8()? {
            SHADOW_SIDE_ALL_TAG => Ok(ShadowSide::All),
            SHADOW_SIDE_TOP_TAG => Ok(ShadowSide::Top),
            SHADOW_SIDE_RIGHT_TAG => Ok(ShadowSide::Right),
            SHADOW_SIDE_BOTTOM_TAG => Ok(ShadowSide::Bottom),
            SHADOW_SIDE_LEFT_TAG => Ok(ShadowSide::Left),
            SHADOW_SIDE_VERTICAL_TAG => Ok(ShadowSide::Vertical),
            SHADOW_SIDE_HORIZONTAL_TAG => Ok(ShadowSide::Horizontal),
            SHADOW_SIDE_RANGE_TAG => Ok(ShadowSide::Range(self.f32()?, self.f32()?)),
            SHADOW_SIDE_TOP_LEFT_TAG => Ok(ShadowSide::TopLeft),
            SHADOW_SIDE_TOP_RIGHT_TAG => Ok(ShadowSide::TopRight),
            SHADOW_SIDE_BOTTOM_RIGHT_TAG => Ok(ShadowSide::BottomRight),
            SHADOW_SIDE_BOTTOM_LEFT_TAG => Ok(ShadowSide::BottomLeft),
            _ => Err(self.invalid_value()),
        }
    }

    fn font_style(&mut self) -> Result<FontStyle, PortableMaterializeError> {
        match self.u8()? {
            FONT_STYLE_NORMAL_TAG => Ok(FontStyle::Normal),
            FONT_STYLE_ITALIC_TAG => Ok(FontStyle::Italic),
            FONT_STYLE_OBLIQUE_TAG => Ok(FontStyle::Oblique),
            FONT_STYLE_OBLIQUE_DEG_TAG => Ok(FontStyle::ObliqueDeg(self.i32()?)),
            _ => Err(self.invalid_value()),
        }
    }

    fn font_weight(&mut self) -> Result<FontWeight, PortableMaterializeError> {
        match self.u8()? {
            FONT_WEIGHT_VERY_THIN_TAG => Ok(FontWeight::VeryThin),
            FONT_WEIGHT_THIN_TAG => Ok(FontWeight::Thin),
            FONT_WEIGHT_NORMAL_TAG => Ok(FontWeight::Normal),
            FONT_WEIGHT_BOLD_TAG => Ok(FontWeight::Bold),
            FONT_WEIGHT_BOLDER_TAG => Ok(FontWeight::Bolder),
            FONT_WEIGHT_VALUE_TAG => Ok(FontWeight::Value(self.u32()?)),
            _ => Err(self.invalid_value()),
        }
    }

    fn text_overflow(&mut self) -> Result<TextOverflow, PortableMaterializeError> {
        match self.u8()? {
            TEXT_OVERFLOW_CLIP_TAG => Ok(TextOverflow::Clip),
            TEXT_OVERFLOW_ELLIPSIS_TAG => Ok(TextOverflow::Ellipsis),
            TEXT_OVERFLOW_WRAP_TAG => Ok(TextOverflow::Wrap),
            TEXT_OVERFLOW_VALUE_TAG => Ok(TextOverflow::Value(self.u32()?)),
            _ => Err(self.invalid_value()),
        }
    }

    fn text_transform(&mut self) -> Result<TextTransform, PortableMaterializeError> {
        match self.u8()? {
            TEXT_TRANSFORM_NONE_TAG => Ok(TextTransform::None),
            TEXT_TRANSFORM_UPPERCASE_TAG => Ok(TextTransform::Uppercase),
            TEXT_TRANSFORM_LOWERCASE_TAG => Ok(TextTransform::Lowercase),
            TEXT_TRANSFORM_CAPITALIZE_TAG => Ok(TextTransform::Capitalize),
            _ => Err(self.invalid_value()),
        }
    }

    fn text_decoration(&mut self) -> Result<TextDecoration, PortableMaterializeError> {
        let line = TextDecorationLine::from_bits(self.u8()?)
            .ok_or_else(|| self.invalid_value())?;
        let style = match self.u8()? {
            TEXT_DECORATION_SOLID_TAG => TextDecorationStyle::Solid,
            TEXT_DECORATION_DOUBLE_TAG => TextDecorationStyle::Double,
            TEXT_DECORATION_DOTTED_TAG => TextDecorationStyle::Dotted,
            TEXT_DECORATION_DASHED_TAG => TextDecorationStyle::Dashed,
            TEXT_DECORATION_WAVY_TAG => TextDecorationStyle::Wavy,
            _ => return Err(self.invalid_value()),
        };
        Ok(TextDecoration {
            line,
            style,
            color: self.optional_color()?,
            thickness: self.optional_f32()?,
            offset: self.f32()?,
        })
    }

    fn optional_color(&mut self) -> Result<Option<Color>, PortableMaterializeError> {
        match self.u8()? {
            OPTIONAL_NONE_TAG => Ok(None),
            OPTIONAL_SOME_TAG => self.color().map(Some),
            _ => Err(self.invalid_value()),
        }
    }

    fn optional_f32(&mut self) -> Result<Option<f32>, PortableMaterializeError> {
        match self.u8()? {
            OPTIONAL_NONE_TAG => Ok(None),
            OPTIONAL_SOME_TAG => self.f32().map(Some),
            _ => Err(self.invalid_value()),
        }
    }

    fn optional_text_shadow(&mut self) -> Result<Option<TextShadow>, PortableMaterializeError> {
        match self.u8()? {
            OPTIONAL_NONE_TAG => Ok(None),
            OPTIONAL_SOME_TAG => {
                let offset_x = self.f32()?;
                let offset_y = self.f32()?;
                let blur = self.f32()?;
                if blur < 0.0 {
                    return Err(self.invalid_value());
                }
                Ok(Some(TextShadow {
                    offset_x,
                    offset_y,
                    blur,
                    color: self.color()?,
                }))
            }
            _ => Err(self.invalid_value()),
        }
    }

    fn color(&mut self) -> Result<Color, PortableMaterializeError> {
        self.u32().map(Color::from_primitive)
    }

    fn boolean(&mut self) -> Result<bool, PortableMaterializeError> {
        match self.u8()? {
            BOOLEAN_FALSE_TAG => Ok(false),
            BOOLEAN_TRUE_TAG => Ok(true),
            _ => Err(self.invalid_value()),
        }
    }

    fn f32(&mut self) -> Result<f32, PortableMaterializeError> {
        let value = f32::from_bits(self.u32()?);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(self.invalid_value())
        }
    }

    fn u8(&mut self) -> Result<u8, PortableMaterializeError> {
        let value = *self
            .bytes
            .get(self.cursor)
            .ok_or_else(|| self.invalid_value())?;
        self.cursor += 1;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, PortableMaterializeError> {
        let end = self
            .cursor
            .checked_add(4)
            .ok_or_else(|| self.invalid_value())?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| self.invalid_value())?;
        self.cursor = end;
        Ok(u32::from_le_bytes(
            bytes.try_into().expect("four bytes were checked"),
        ))
    }

    fn i32(&mut self) -> Result<i32, PortableMaterializeError> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64, PortableMaterializeError> {
        let end = self
            .cursor
            .checked_add(8)
            .ok_or_else(|| self.invalid_value())?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| self.invalid_value())?;
        self.cursor = end;
        Ok(u64::from_le_bytes(
            bytes.try_into().expect("eight bytes were checked"),
        ))
    }

    fn finish(self) -> Result<(), PortableMaterializeError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(self.invalid_value())
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    #[inline]
    const fn invalid_value(&self) -> PortableMaterializeError {
        PortableMaterializeError::InvalidPropertyValue {
            property: self.property,
        }
    }
}

#[cfg(test)]
mod tests {
    use aimer_widget::portable::__anteros::{
        ModelLimits, PropertyId, PropertyValue, Version, WidgetDocument, WidgetDocumentView,
        WidgetNode, WidgetSchemaId,
    };
    use aimer_widget::portable::{
        PortableMaterializeProperty, PortableProperty, PortablePropertyConversion,
    };
    #[cfg(feature = "portable-guest")]
    use aimer_widget::portable::{
        PortableBuildContext, PortableBuildError, PortableEncodeProperty, PortableLimits,
        PortableWidgetLimits, SourceFingerprint, StableId128,
    };
    #[cfg(feature = "portable-guest")]
    use aimer_widget::portable::__anteros::WidgetProperty;
    #[cfg(feature = "portable-guest")]
    use super::{
        FONT_STYLE_NORMAL_TAG, FONT_WEIGHT_NORMAL_TAG, OPTIONAL_NONE_TAG,
        TEXT_DECORATION_SOLID_TAG, TEXT_OVERFLOW_WRAP_TAG,
    };

    use super::super::layout_spacing::{LayoutSpacing, Spacing};
    use super::super::text_style::{
        FontFamily, FontStyle, FontWeight, LineHeight, TextDecoration, TextDecorationLine,
        TextDecorationStyle, TextOverflow, TextShadow, TextStyle, TextTransform,
    };
    use crate::{
        BorderRadius, BorderSlice, BorderStyle, BoxBorder, BoxDecoration, BoxOutline, BoxShadow,
        ShadowSide,
    };
    use aimer_attribute::Dimension;
    use aimer_color::prelude::Color;

    const PROPERTY: PropertyId = PropertyId::new(7);
    const LIMITS: ModelLimits = ModelLimits::new(4_096, 4, 64, 4_096);

    #[cfg(feature = "portable-guest")]
    fn guest_context() -> PortableBuildContext {
        PortableBuildContext::new(
            1,
            0,
            PortableWidgetLimits::new(4, 16, 4, 4, 64, 4_096).with_max_blob_bytes(4_096),
            PortableLimits::new(4, 16, 64, 4_096, 4_096),
        )
        .unwrap()
    }

    #[test]
    fn layout_spacing_reflects_its_versioned_blob_schema() {
        let reflection = <LayoutSpacing as PortableProperty>::REFLECTION;
        let schema = reflection.value_schema().unwrap();

        assert_eq!(
            reflection.value_kind(),
            aimer_widget::portable::__anteros::PropertyValueKind::BlobRef
        );
        assert_eq!(
            reflection.conversion(),
            PortablePropertyConversion::CustomValue
        );
        assert_eq!(
            schema.canonical_name(),
            "aimer.value:aimer_style::LayoutSpacing"
        );
        assert_eq!(schema.version(), Version::new(1, 0));
        assert_eq!(schema.maximum_encoded_bytes(), 21);
    }

    #[test]
    fn text_style_reflects_its_versioned_blob_schema() {
        let reflection = <TextStyle as PortableProperty>::REFLECTION;
        let schema = reflection.value_schema().unwrap();

        assert_eq!(
            reflection.value_kind(),
            aimer_widget::portable::__anteros::PropertyValueKind::BlobRef
        );
        assert_eq!(
            schema.canonical_name(),
            "aimer.value:aimer_style::TextStyle"
        );
        assert_eq!(schema.version(), Version::new(2, 0));
        assert_eq!(schema.maximum_encoded_bytes(), 128);
    }

    #[test]
    fn line_height_reflects_its_versioned_blob_schema() {
        let reflection = <LineHeight as PortableProperty>::REFLECTION;
        let schema = reflection.value_schema().unwrap();

        assert_eq!(
            schema.canonical_name(),
            "aimer.value:aimer_style::LineHeight"
        );
        assert_eq!(schema.version(), Version::new(1, 0));
        assert_eq!(schema.maximum_encoded_bytes(), 5);
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn line_height_guest_encoding_round_trips_each_form() {
        for value in [
            LineHeight::Normal,
            LineHeight::Px(24.0),
            LineHeight::Factor(1.5),
        ] {
            let blob = encode_blob(value);
            assert_eq!(materialize_line_height(&blob), value);
        }
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn text_style_guest_encoding_round_trips_every_codec_variant() {
        let style = TextStyle::new()
            .font_size(24)
            .font_family(FontFamily::MONOSPACE)
            .font_style(FontStyle::ObliqueDeg(-7))
            .font_weight(FontWeight::Value(650))
            .color(Color::Rgba(10, 20, 30, 40))
            .background_color(Color::Rgba(50, 60, 70, 80))
            .text_overflow(TextOverflow::Value(3))
            .text_transform(TextTransform::Uppercase)
            .letter_spacing(0.75)
            .word_spacing(-1.25)
            .text_shadow(
                TextShadow::new()
                    .offset_x(2.0)
                    .offset_y(-1.0)
                    .blur(3.0)
                    .color(Color::Rgba(11, 12, 13, 14)),
            )
            .text_decoration(
                TextDecoration::new()
                    .line(TextDecorationLine::UNDERLINE | TextDecorationLine::ITALIC)
                    .style(TextDecorationStyle::Wavy)
                    .color(Color::Rgba(90, 100, 110, 120))
                    .thickness(1.25)
                    .offset(-0.5),
            );

        let blob = encode_blob(style);
        assert!(blob.len() <= 128);
        assert_eq!(materialize_text_style(&blob), style);
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn text_style_version_one_decodes_with_new_values_at_defaults() {
        let mut blob = vec![1_u8];
        blob.extend_from_slice(&19_u32.to_le_bytes());
        blob.extend_from_slice(&FontFamily::SANS_SERIF.raw().to_le_bytes());
        blob.push(FONT_STYLE_NORMAL_TAG);
        blob.push(FONT_WEIGHT_NORMAL_TAG);
        blob.extend_from_slice(&Color::BLACK.as_u32().to_le_bytes());
        blob.push(OPTIONAL_NONE_TAG);
        blob.push(TEXT_OVERFLOW_WRAP_TAG);
        blob.push(TextDecorationLine::NONE.bits());
        blob.push(TEXT_DECORATION_SOLID_TAG);
        blob.push(OPTIONAL_NONE_TAG);
        blob.push(OPTIONAL_NONE_TAG);
        blob.extend_from_slice(&0.0_f32.to_bits().to_le_bytes());

        let style = materialize_text_style(&blob);
        assert_eq!(style.font_size, 19);
        assert_eq!(style.text_overflow, TextOverflow::Wrap);
        assert_eq!(style.text_transform, TextTransform::None);
        assert_eq!(style.letter_spacing, 0.0);
        assert_eq!(style.word_spacing, 0.0);
        assert_eq!(style.text_shadow, None);
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn text_style_v2_rejects_unknown_transform_non_finite_spacing_and_bad_shadow() {
        let base = TextStyle::default();

        let mut unknown_transform = encode_blob(base);
        unknown_transform[29] = 99;
        assert_invalid_text_style_blob(&unknown_transform);

        let mut non_finite_spacing = encode_blob(TextStyle::default());
        non_finite_spacing[30..34].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert_invalid_text_style_blob(&non_finite_spacing);

        let mut bad_shadow = encode_blob(TextStyle::new().text_shadow(TextShadow::new()));
        bad_shadow[47..51].copy_from_slice(&(-1.0_f32).to_bits().to_le_bytes());
        assert_invalid_text_style_blob(&bad_shadow);
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn line_height_rejects_unknown_tags_non_finite_values_and_non_positive_values() {
        let mut non_finite = vec![1, 1];
        non_finite.extend_from_slice(&f32::NAN.to_bits().to_le_bytes());
        for blob in [
            vec![1, 99],
            non_finite,
            vec![1, 1, 0, 0, 0, 0],
            vec![1, 2, 0, 0, 0, 0],
        ] {
            assert_invalid_line_height_blob(&blob);
        }
    }

    #[test]
    fn box_decoration_reflects_an_unrestricted_versioned_blob_schema() {
        let reflection = <BoxDecoration as PortableProperty>::REFLECTION;
        let schema = reflection.value_schema().unwrap();

        assert_eq!(
            reflection.value_kind(),
            aimer_widget::portable::__anteros::PropertyValueKind::BlobRef
        );
        assert_eq!(
            reflection.conversion(),
            PortablePropertyConversion::CustomValue
        );
        assert_eq!(
            schema.canonical_name(),
            "aimer.value:aimer_style::BoxDecoration"
        );
        assert_eq!(schema.version(), Version::new(1, 0));
        assert_eq!(schema.maximum_encoded_bytes(), u32::MAX);
    }

    #[test]
    fn layout_spacing_materializes_every_spacing_variant_from_its_canonical_blob() {
        #[rustfmt::skip]
        let blob = [
            1, // wire version
            1, 12, 0, 0, 0, // top: 12 px
            2, 25, 0, 0, 0, // bottom: 25 percent
            0, 0, 0, 0, 0, // left: none
            1, 7, 0, 0, 0, // right: 7 px
        ];
        let image = widget_document(&blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();

        let spacing =
            LayoutSpacing::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)).unwrap();

        assert!(
            spacing
                == LayoutSpacing {
                    top: Spacing::Px(12),
                    bottom: Spacing::Percent(25),
                    left: Spacing::None,
                    right: Spacing::Px(7),
                }
        );
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn layout_spacing_guest_encoding_round_trips_its_canonical_blob() {
        let spacing = LayoutSpacing {
            top: Spacing::Px(12),
            bottom: Spacing::Percent(25),
            left: Spacing::None,
            right: Spacing::Px(7),
        };
        let mut context = guest_context();
        let value = spacing.encode_property(&mut context).unwrap();
        assert_eq!(value, PropertyValue::BlobRef(0));

        let node = context
            .push_node(
                WidgetSchemaId::new(1),
                Version::new(1, 0),
                None,
                SourceFingerprint::new(StableId128::from_u128(1)),
                &[WidgetProperty::new(PROPERTY, value)],
                &[],
            )
            .unwrap();
        let graph = context.finish_graph(node).unwrap();

        assert_eq!(
            graph.blob(0),
            Some(
                &[
                    1, // wire version
                    1, 12, 0, 0, 0, // top: 12 px
                    2, 25, 0, 0, 0, // bottom: 25 percent
                    0, 0, 0, 0, 0, // left: none
                    1, 7, 0, 0, 0, // right: 7 px
                ][..]
            )
        );
        let image = widget_document(graph.blob(0).unwrap());
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        assert!(
            LayoutSpacing::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)).unwrap()
                == spacing
        );
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn box_decoration_guest_encoding_round_trips_empty_and_complete_values() {
        let empty = BoxDecoration::new();
        let empty_blob = encode_blob(empty.clone());
        let mut expected_empty = vec![0_u8; 106];
        expected_empty[0] = 1;
        assert_eq!(empty_blob, expected_empty);
        assert_eq!(materialize_decoration(&empty_blob), empty);

        let complete = BoxDecoration {
            border: BoxBorder {
                left: BorderSlice {
                    style: BorderStyle::Solid,
                    stroke: Dimension::Px(2.0),
                    color: Color::RED,
                },
                right: BorderSlice {
                    style: BorderStyle::Dashed,
                    stroke: Dimension::Percent(5.0),
                    color: Color::GREEN,
                },
                top: BorderSlice {
                    style: BorderStyle::Dotted,
                    stroke: Dimension::Auto,
                    color: Color::BLUE,
                },
                bottom: BorderSlice::default(),
            },
            outline: BoxOutline::default(),
            border_radius: BorderRadius {
                top_left: Dimension::Px(4.0),
                top_right: Dimension::Percent(10.0),
                bottom_right: Dimension::Auto,
                bottom_left: Dimension::Px(8.0),
            },
            box_shadow: vec![
                BoxShadow::new()
                    .offset_x(1.0)
                    .offset_y(2.0)
                    .blur(3.0)
                    .spread(4.0)
                    .color(Color::BLACK),
                BoxShadow::new()
                    .offset_x(-1.0)
                    .offset_y(-2.0)
                    .blur(5.0)
                    .spread(-3.0)
                    .color(Color::BLUE)
                    .inset(true)
                    .side(ShadowSide::Range(0.25, 1.5)),
            ],
            background_color: std::cell::Cell::new(Some(Color::WHITE)),
        };
        let complete_blob = encode_blob(complete.clone());
        assert_eq!(complete_blob, complete_decoration_blob());
        assert_eq!(materialize_decoration(&complete_blob), complete);
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn box_decoration_guest_encoding_covers_every_shadow_side_tag() {
        let sides = [
            ShadowSide::All,
            ShadowSide::Top,
            ShadowSide::Right,
            ShadowSide::Bottom,
            ShadowSide::Left,
            ShadowSide::Vertical,
            ShadowSide::Horizontal,
            ShadowSide::Range(0.25, 1.5),
            ShadowSide::TopLeft,
            ShadowSide::TopRight,
            ShadowSide::BottomRight,
            ShadowSide::BottomLeft,
        ];

        for (index, side) in sides.into_iter().enumerate() {
            let decoration = BoxDecoration::new().add_shadow(
                BoxShadow::new()
                    .offset_x(index as f32)
                    .inset(index % 2 == 1)
                    .side(side),
            );
            let blob = encode_blob(decoration.clone());
            assert_eq!(blob[126], index as u8);
            assert_eq!(materialize_decoration(&blob), decoration);
        }
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn box_decoration_guest_encoding_rejects_non_finite_values_before_commit() {
        let mut context = guest_context();
        let value = BoxDecoration::new().border_radius(BorderRadius::new().top_left(f32::NAN));
        assert!(matches!(
            value.encode_property(&mut context),
            Err(PortableBuildError::NonFiniteFloat)
        ));

        let value = BoxDecoration::new().add_shadow(BoxShadow::new().blur(f32::INFINITY));
        assert!(matches!(
            value.encode_property(&mut context),
            Err(PortableBuildError::NonFiniteFloat)
        ));
    }

    #[test]
    fn box_decoration_materializes_the_complete_bounded_blob() {
        let blob = complete_decoration_blob();
        let image = widget_document(&blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();

        let decoration =
            BoxDecoration::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)).unwrap();

        assert_eq!(
            decoration,
            BoxDecoration {
                border: BoxBorder {
                    left: BorderSlice {
                        style: BorderStyle::Solid,
                        stroke: Dimension::Px(2.0),
                        color: Color::RED,
                    },
                    right: BorderSlice {
                        style: BorderStyle::Dashed,
                        stroke: Dimension::Percent(5.0),
                        color: Color::GREEN,
                    },
                    top: BorderSlice {
                        style: BorderStyle::Dotted,
                        stroke: Dimension::Auto,
                        color: Color::BLUE,
                    },
                    bottom: BorderSlice::default(),
                },
                outline: BoxOutline::default(),
                border_radius: BorderRadius {
                    top_left: Dimension::Px(4.0),
                    top_right: Dimension::Percent(10.0),
                    bottom_right: Dimension::Auto,
                    bottom_left: Dimension::Px(8.0),
                },
                box_shadow: vec![
                    BoxShadow::new()
                        .offset_x(1.0)
                        .offset_y(2.0)
                        .blur(3.0)
                        .spread(4.0)
                        .color(Color::BLACK),
                    BoxShadow::new()
                        .offset_x(-1.0)
                        .offset_y(-2.0)
                        .blur(5.0)
                        .spread(-3.0)
                        .color(Color::BLUE)
                        .inset(true)
                        .side(ShadowSide::Range(0.25, 1.5)),
                ],
                background_color: std::cell::Cell::new(Some(Color::WHITE)),
            }
        );
    }

    #[test]
    fn portable_style_values_reject_wrong_types_and_non_canonical_blobs() {
        let valid_spacing = [
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let image = widget_document(&valid_spacing);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        assert!(matches!(
            LayoutSpacing::from_awir(&document, PROPERTY, PropertyValue::Bool(false)),
            Err(
                aimer_widget::portable::PortableMaterializeError::InvalidPropertyType {
                    property: PROPERTY,
                }
            )
        ));

        let invalid_spacing_tag = [
            1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_invalid_spacing_blob(&invalid_spacing_tag);

        let mut trailing_spacing = valid_spacing.to_vec();
        trailing_spacing.push(0);
        assert_invalid_spacing_blob(&trailing_spacing);

        assert_invalid_spacing_blob(&valid_spacing[..20]);
    }

    #[test]
    fn decoration_rejects_a_shadow_count_not_backed_by_blob_bytes() {
        let mut blob = default_decoration_prefix();
        blob.extend_from_slice(&u32::MAX.to_le_bytes());
        let image = widget_document(&blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();

        assert_eq!(
            BoxDecoration::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)),
            Err(
                aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY,
                }
            )
        );
    }

    #[test]
    fn decoration_rejects_truncation_trailing_bytes_unknown_tags_and_non_finite_values() {
        let mut truncated = complete_decoration_blob();
        truncated.pop();
        assert_invalid_decoration_blob(&truncated);

        let mut trailing = complete_decoration_blob();
        trailing.push(0);
        assert_invalid_decoration_blob(&trailing);

        let mut unknown_style = empty_decoration_blob();
        unknown_style[1] = 4;
        assert_invalid_decoration_blob(&unknown_style);

        let mut unknown_dimension = empty_decoration_blob();
        unknown_dimension[2] = 3;
        assert_invalid_decoration_blob(&unknown_dimension);

        let mut unknown_shadow_side = default_decoration_prefix();
        unknown_shadow_side.extend_from_slice(&1_u32.to_le_bytes());
        push_shadow(
            &mut unknown_shadow_side,
            [0.0; 4],
            Color::BLACK,
            false,
            12,
            None,
        );
        assert_invalid_decoration_blob(&unknown_shadow_side);

        let mut noncanonical_boolean = default_decoration_prefix();
        noncanonical_boolean.extend_from_slice(&1_u32.to_le_bytes());
        push_shadow(
            &mut noncanonical_boolean,
            [0.0; 4],
            Color::BLACK,
            false,
            0,
            None,
        );
        noncanonical_boolean[125] = 2;
        assert_invalid_decoration_blob(&noncanonical_boolean);

        let mut noncanonical_option = default_decoration_prefix();
        noncanonical_option.extend_from_slice(&0_u32.to_le_bytes());
        noncanonical_option.push(2);
        assert_invalid_decoration_blob(&noncanonical_option);

        let mut non_finite = complete_decoration_blob();
        non_finite[3..7].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
        assert_invalid_decoration_blob(&non_finite);
    }

    fn assert_invalid_spacing_blob(blob: &[u8]) {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        assert!(matches!(
            LayoutSpacing::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)),
            Err(
                aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY,
                }
            )
        ));
    }

    fn assert_invalid_decoration_blob(blob: &[u8]) {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        assert!(matches!(
            BoxDecoration::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)),
            Err(
                aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY,
                }
            )
        ));
    }

    #[cfg(feature = "portable-guest")]
    fn encode_blob<T: PortableEncodeProperty>(value: T) -> Vec<u8> {
        let mut context = guest_context();
        let property = value.encode_property(&mut context).unwrap();
        let index = match property {
            PropertyValue::BlobRef(index) => index,
            other => panic!("expected a blob property, got {other:?}"),
        };
        let node = context
            .push_node(
                WidgetSchemaId::new(1),
                Version::new(1, 0),
                None,
                SourceFingerprint::new(StableId128::from_u128(2)),
                &[WidgetProperty::new(PROPERTY, property)],
                &[],
            )
            .unwrap();
        let graph = context.finish_graph(node).unwrap();
        graph.blob(index).unwrap().to_vec()
    }

    #[cfg(feature = "portable-guest")]
    fn materialize_decoration(blob: &[u8]) -> BoxDecoration {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        BoxDecoration::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)).unwrap()
    }

    #[cfg(feature = "portable-guest")]
    fn materialize_text_style(blob: &[u8]) -> TextStyle {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        TextStyle::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)).unwrap()
    }

    #[cfg(feature = "portable-guest")]
    fn assert_invalid_text_style_blob(blob: &[u8]) {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        assert!(matches!(
            TextStyle::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)),
            Err(
                aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY,
                }
            )
        ));
    }

    #[cfg(feature = "portable-guest")]
    fn materialize_line_height(blob: &[u8]) -> LineHeight {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        LineHeight::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)).unwrap()
    }

    #[cfg(feature = "portable-guest")]
    fn assert_invalid_line_height_blob(blob: &[u8]) {
        let image = widget_document(blob);
        let document = WidgetDocumentView::decode(&image, LIMITS).unwrap();
        assert!(matches!(
            LineHeight::from_awir(&document, PROPERTY, PropertyValue::BlobRef(0)),
            Err(
                aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY,
                }
            )
        ));
    }

    fn complete_decoration_blob() -> Vec<u8> {

        let mut blob = vec![1];
        push_border_slice(&mut blob, 1, 1, 2.0, Color::RED);
        push_border_slice(&mut blob, 2, 2, 5.0, Color::GREEN);
        push_border_slice(&mut blob, 3, 0, 0.0, Color::BLUE);
        push_border_slice(&mut blob, 0, 0, 0.0, Color::Transparent);
        for _ in 0..4 {
            push_border_slice(&mut blob, 0, 0, 0.0, Color::Transparent);
        }
        push_dimension(&mut blob, 1, 4.0);
        push_dimension(&mut blob, 2, 10.0);
        push_dimension(&mut blob, 0, 0.0);
        push_dimension(&mut blob, 1, 8.0);
        blob.extend_from_slice(&2_u32.to_le_bytes());
        push_shadow(
            &mut blob,
            [1.0, 2.0, 3.0, 4.0],
            Color::BLACK,
            false,
            0,
            None,
        );
        push_shadow(
            &mut blob,
            [-1.0, -2.0, 5.0, -3.0],
            Color::BLUE,
            true,
            7,
            Some((0.25, 1.5)),
        );
        blob.push(1);
        blob.extend_from_slice(&Color::WHITE.as_u32().to_le_bytes());
        blob
    }

    fn default_decoration_prefix() -> Vec<u8> {
        let mut blob = vec![1];
        for _ in 0..8 {
            push_border_slice(&mut blob, 0, 0, 0.0, Color::Transparent);
        }
        for _ in 0..4 {
            push_dimension(&mut blob, 0, 0.0);
        }
        blob
    }

    fn empty_decoration_blob() -> Vec<u8> {
        let mut blob = default_decoration_prefix();
        blob.extend_from_slice(&0_u32.to_le_bytes());
        blob.push(0);
        blob
    }

    fn push_border_slice(blob: &mut Vec<u8>, style: u8, dimension: u8, value: f32, color: Color) {
        blob.push(style);
        push_dimension(blob, dimension, value);
        blob.extend_from_slice(&color.as_u32().to_le_bytes());
    }

    fn push_dimension(blob: &mut Vec<u8>, tag: u8, value: f32) {
        blob.push(tag);
        blob.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    fn push_shadow(
        blob: &mut Vec<u8>,
        geometry: [f32; 4],
        color: Color,
        inset: bool,
        side: u8,
        range: Option<(f32, f32)>,
    ) {
        for value in geometry {
            blob.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        blob.extend_from_slice(&color.as_u32().to_le_bytes());
        blob.push(u8::from(inset));
        blob.push(side);
        if let Some((start, end)) = range {
            blob.extend_from_slice(&start.to_bits().to_le_bytes());
            blob.extend_from_slice(&end.to_bits().to_le_bytes());
        }
    }

    fn widget_document(blob: &[u8]) -> Vec<u8> {
        let nodes = [WidgetNode::new(WidgetSchemaId::new(1), Version::new(1, 0))];
        WidgetDocument::new(1, 1, 0, &nodes, &[], &[blob])
            .encode(LIMITS)
            .unwrap()
    }
}
