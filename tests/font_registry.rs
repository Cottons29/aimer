use aimer::style::TextStyle;
use aimer::{Color, RichText, SpanStyle, TextSpan};
use aimer_assets::{FontError, FontFamily, FontRegistration, FontRegistry, FontStyle, FontWeight};

const TEST_FONT: &[u8] = aimer_assets::bundled_monospace_bytes();

#[test]
fn named_font_registration_is_validated_and_stable() {
    assert_ne!(FontFamily::SANS_SERIF, FontFamily::MONOSPACE);

    let family = FontRegistry::register(FontRegistration {
        family: "Aimer Registry Test Mono",
        bytes: TEST_FONT,
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
    })
    .expect("valid font bytes should register");
    let bold_family = FontRegistry::register(FontRegistration {
        family: " aimer registry test mono ",
        bytes: TEST_FONT,
        weight: FontWeight::Bold,
        style: FontStyle::Normal,
    })
    .expect("a second variant should register under the same normalized family");

    assert_eq!(family, bold_family);

    let duplicate = FontRegistry::register(FontRegistration {
        family: "AIMER REGISTRY TEST MONO",
        bytes: TEST_FONT,
        weight: FontWeight::Value(400),
        style: FontStyle::Normal,
    });
    assert!(matches!(duplicate, Err(FontError::DuplicateVariant { .. })));

    let invalid = FontRegistry::register(FontRegistration {
        family: "Invalid Font Test",
        bytes: b"not a font",
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
    });
    assert_eq!(invalid, Err(FontError::InvalidFont));

    let empty_name = FontRegistry::register(FontRegistration {
        family: "  ",
        bytes: TEST_FONT,
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
    });
    assert_eq!(empty_name, Err(FontError::EmptyFamily));
}

#[test]
fn public_monospace_and_highlighted_rich_text_contracts_compose() {
    let family = FontRegistry::register(FontRegistration {
        family: "Aimer Public Text Test Mono",
        bytes: TEST_FONT,
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
    })
    .unwrap();
    let base = TextStyle::new().font_family(family);
    let span = TextSpan::root([
        TextSpan::new("let "),
        TextSpan::new("answer")
            .style(SpanStyle::new().background_color(Color::Rgba(255, 240, 120, 255))),
    ]);
    let flattened = span.flatten(&base);

    assert!(
        flattened
            .iter()
            .all(|span| span.style.font_family == family)
    );
    assert_eq!(
        flattened[0]
            .style
            .background_color,
        None
    );
    assert_eq!(
        flattened[1]
            .style
            .background_color,
        Some(Color::Rgba(255, 240, 120, 255))
    );

    let _widget = RichText::new(span)
        .text_style(base)
        .wrapped();
}
