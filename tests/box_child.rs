use aimer::gesture::gesture_detector::GestureDetector;
use aimer::mouse_region::MouseRegion;
use aimer::style::AnimatedTheme;
use aimer::{
    Align, AnyWidget, AspectRatio, Button, Container, Expanded, Opacity, Positioned, Provider,
    Scrollable, SizedBox, StoreProvider, ZeroSizedBox,
};

fn assert_any_widget(_: AnyWidget) {}

#[test]
fn box_child_erases_every_single_child_widget() {
    assert_any_widget(Expanded::new().box_child(ZeroSizedBox));
    assert_any_widget(Scrollable::new().box_child(ZeroSizedBox));
    assert_any_widget(AspectRatio::new().box_child(ZeroSizedBox));
    assert_any_widget(Container::new().box_child(ZeroSizedBox));
    assert_any_widget(Opacity::new().box_child(ZeroSizedBox));
    assert_any_widget(SizedBox::new().box_child(ZeroSizedBox));
    assert_any_widget(Align::new().box_child(ZeroSizedBox));
    assert_any_widget(Positioned::new().box_child(ZeroSizedBox));
    assert_any_widget(Button::new().box_child(ZeroSizedBox));
    assert_any_widget(GestureDetector::new().box_child(ZeroSizedBox));
    assert_any_widget(MouseRegion::new().box_child(ZeroSizedBox));
    assert_any_widget(
        Provider::new()
            .create(|| 0_u8)
            .box_child(ZeroSizedBox),
    );
    assert_any_widget(
        StoreProvider::<u8, u8>::new()
            .create(|| 0)
            .reducer(|state, action| *state = action)
            .box_child(ZeroSizedBox),
    );
    assert_any_widget(AnimatedTheme::new().box_child(ZeroSizedBox));
}
