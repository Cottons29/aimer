use aimer::BuildContext;
use aimer::animation::Animatable;
use aimer::style::{AnimatedTheme, Theme};
use aimer::{Text, Widget};

#[derive(Clone, Copy, Debug, PartialEq, Theme)]
struct AppTheme {
    opacity: f32,
    inset: i32,
}

#[derive(Clone, Debug, PartialEq, Theme)]
struct GenericTheme<T>
where
    T: Send,
{
    value: T,
}

fn assert_theme<T: Theme>() {}

#[test]
fn named_theme_interpolates_fields_and_preserves_exact_endpoints() {
    assert_theme::<AppTheme>();
    let begin = AppTheme {
        opacity: 0.0,
        inset: 2,
    };
    let end = AppTheme {
        opacity: 1.0,
        inset: 10,
    };

    assert_eq!(begin.lerp(&end, -1.0), begin);
    assert_eq!(
        begin.lerp(&end, 0.5),
        AppTheme {
            opacity: 0.5,
            inset: 6
        }
    );
    assert_eq!(begin.lerp(&end, 2.0), end);
}

#[test]
fn generic_theme_preserves_declared_bounds() {
    assert_theme::<GenericTheme<f32>>();
    let begin = GenericTheme { value: 2.0_f32 };
    let end = GenericTheme { value: 6.0_f32 };

    assert_eq!(begin.lerp(&end, 0.5), GenericTheme { value: 4.0 });
}

#[test]
fn derived_theme_exposes_snapshot_and_copy_lookup_signatures() {
    let _: fn(&BuildContext) -> aimer::provider::Snapshot<AppTheme> = AppTheme::of;
    let _: fn(&BuildContext) -> aimer::provider::Snapshot<AppTheme> = AppTheme::read;
    let _: fn(&BuildContext) -> AppTheme = AppTheme::copied;
}

#[test]
fn animated_theme_builder_accepts_a_derived_custom_theme() {
    fn assert_widget<T: Widget>(_widget: &T) {}

    let widget = AnimatedTheme::new()
        .data(AppTheme {
            opacity: 0.5,
            inset: 8,
        })
        .child(Text::new("child"));

    assert_widget(&widget);
}
