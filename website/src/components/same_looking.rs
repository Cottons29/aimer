use std::time::Duration;

use crate::components::animation_button::{AnimatedPlatformButtonList, PLATFORMS};
use crate::utils::{app_padding, is_mobile, mobile_title};
use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::native::haptic::{HapticPattern, Haptics};
use aimer::style::{FontWeight, TextDecoration, TextStyle, Theme, ThemeData};
use aimer::*;

#[cfg(feature = "portable-guest")]
use aimer::portable::{
    AimerReflectionType, DecodeError, Decoder, EncodeError, Encoder, FieldDescriptor, FieldKind,
    PortableApply, PortableEncode, StableId128, TypeSchema,
};

#[widget(Stateful)]
pub struct SameLookingSection {
    pub key: Option<Key>,
}

pub struct SameLookingSectionState {
    current_index: usize,
    state: StateUpdater<Self>,
}

#[cfg(feature = "portable-guest")]
const SAME_LOOKING_SECTION_STATE_FIELDS: &[FieldDescriptor] = &[
    FieldDescriptor::new("current_index", "usize", FieldKind::Retained),
    FieldDescriptor::new("state", "StateUpdater<SameLookingSectionState>", FieldKind::Fresh),
];

#[cfg(feature = "portable-guest")]
const SAME_LOOKING_SECTION_STATE_SCHEMA: TypeSchema = TypeSchema::new(
    "SameLookingSectionState",
    StableId128::from_path(
        "aimer.type.v1",
        "website::components::same_looking::SameLookingSectionState",
    ),
    SAME_LOOKING_SECTION_STATE_FIELDS,
);

#[cfg(feature = "portable-guest")]
impl AimerReflectionType for SameLookingSectionState {
    const TYPE_ID: StableId128 = StableId128::from_path(
        "aimer.type.v1",
        "website::components::same_looking::SameLookingSectionState",
    );

    fn schema() -> &'static TypeSchema {
        &SAME_LOOKING_SECTION_STATE_SCHEMA
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncode for SameLookingSectionState {
    fn encode(&self, encoder: &mut Encoder<'_>) -> Result<(), EncodeError> {
        encoder.nested(|encoder| {
            encoder.field(&SAME_LOOKING_SECTION_STATE_FIELDS[0], |encoder| {
                self.current_index.encode(encoder)
            })?;
            encoder.field(&SAME_LOOKING_SECTION_STATE_FIELDS[1], |_| Ok(()))
        })
    }
}

#[cfg(feature = "portable-guest")]
impl PortableApply for SameLookingSectionState {
    type Retained = usize;

    fn decode_retained(decoder: &mut Decoder<'_>) -> Result<Self::Retained, DecodeError> {
        decoder.nested(|decoder| {
            let current_index = decoder
                .field(&SAME_LOOKING_SECTION_STATE_FIELDS[0])?
                .unwrap();
            let _ = decoder.field::<u8>(&SAME_LOOKING_SECTION_STATE_FIELDS[1])?;
            Ok(current_index)
        })
    }

    fn apply_retained(&mut self, retained: Self::Retained) {
        self.current_index = retained;
    }
}

impl StatefulWidget for SameLookingSection {
    type State = SameLookingSectionState;

    fn create_state(self) -> Self::State {
        // The framework preserves the live state across parent rebuilds
        // (e.g. a window resize) by adopting it during reconciliation, so the
        // selected tab survives without any manual persistence — this only
        // needs to provide the initial value.
        SameLookingSectionState {
            current_index: 0,
            state: StateUpdater::new(),
        }
    }
}

const PLATFORM_IMAGE: &[&str] = &[
    "assets/macos_screenshot.png",
    "assets/ios_screenshot.png",
    "assets/web_screenshot.png",
    "assets/android_screenshot.png",
];
const PLATFORM_IMAGE_SWITCHER_KEY: &str = "platform-image-switcher";

fn platform_image(index: usize) -> &'static str {
    PLATFORM_IMAGE[index % PLATFORM_IMAGE.len()]
}

fn platform_image_switcher(index: usize) -> AnimatedSwitcher<AssetImage> {
    AnimatedSwitcher::new(
        Duration::from_millis(350),
        Curve::FastOutSlowIn,
        AssetImage::new(platform_image(index)),
    )
    .child_key(PLATFORMS[index % PLATFORMS.len()])
    .key(PLATFORM_IMAGE_SWITCHER_KEY)
}

impl State<SameLookingSection> for SameLookingSectionState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.state = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        // eprintln!("Current index: {}", self.current_index);
        Container::new()
            .color(theme.background_color)
            .padding(app_padding(ctx))
            .child(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Center)
                    .children([
                        Container::new()
                            .height(100)
                            .child(
                                Text::new("Consistence Looking").text_style(
                                    TextStyle::new()
                                        .font_size(mobile_title(ctx))
                                        .color(theme.on_background_color)
                                        .font_weight(FontWeight::Bolder)
                                        .text_decoration(TextDecoration::Underline),
                                ),
                            )
                            .boxed(),
                        SizedBox::new().height(24).boxed(),
                        Container::new()
                            .height(if is_mobile(ctx) { 250 } else { 450 })
                            .child(platform_image_switcher(self.current_index))
                            .boxed(),
                        SizedBox::new().height(40).boxed(),
                        AnimatedPlatformButtonList::new()
                            .selected_index(self.current_index)
                            .compact(is_mobile(ctx))
                            .on_selected({
                                let updater = self.state.clone();
                                move |index| {
                                    let pattern = HapticPattern::new()
                                        .transient(0.0, 1.0, 1.0) // sharp tap at t=0
                                        .continuous(0.1, 0.2, 0.3, 0.2)
                                        .transient(0.25, 1.0, 1.0); // soft buzz starting at t=0.1s for 0.4s
                                    Haptics::play_pattern(&pattern);
                                    // Haptics::impact(ImpactStyle::Rigid);
                                    if updater.read_state().current_index != index {
                                        updater.set_state(move |state| state.current_index = index);
                                    }
                                }
                            })
                            .boxed(),
                        SizedBox::new().height(40).boxed(),
                    ]),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_image_matches_selected_platform() {
        assert_eq!(platform_image(1), "assets/ios_screenshot.png");
    }

    #[test]
    fn platform_image_wraps_out_of_range_index() {
        assert_eq!(
            platform_image(PLATFORM_IMAGE.len()),
            "assets/macos_screenshot.png"
        );
    }

    #[test]
    fn platform_image_transition_has_stable_switcher_identity() {
        assert_eq!(
            Widget::key(&platform_image_switcher(0)),
            Some(Key::Value(PLATFORM_IMAGE_SWITCHER_KEY.to_owned()))
        );
    }
}
