use std::cell::Cell;
use std::rc::Rc;

use aimer::callback::VoidCallback;
use aimer::gesture::gesture_detector::GestureDetector;
use aimer::mouse_region::{MouseRegion, PointerState};
use aimer::style::{TextAlign, TextDecoration, TextStyle, Theme, ThemeData};
use aimer::{
    BuildContext, Color, Row, SizedBox, State, StateUpdater, StatefulWidget, Svg, SvgDocument,
    SvgStyle, Text, Widget, widget,
};

#[cfg(feature = "portable-guest")]
use aimer::portable::{
    AimerReflectionType, DecodeError, Decoder, EncodeError, Encoder, FieldDescriptor, FieldKind,
    PortableApply, PortableEncode, StableId128, TypeSchema,
};

#[widget(Stateful)]
pub struct BlogBackButton {
    on_click: VoidCallback,
}

impl BlogBackButton {
    pub fn new() -> Self {
        Self {
            on_click: VoidCallback::default(),
        }
    }

    pub fn on_click(mut self, on_click: impl Into<VoidCallback>) -> Self {
        self.on_click = on_click.into();
        self
    }
}

impl Default for BlogBackButton {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BlogBackButtonState {
    is_hover: bool,
    on_click: VoidCallback,
    current_state: Rc<Cell<PointerState>>,
    updater: StateUpdater<Self>,
}

#[cfg(feature = "portable-guest")]
const BLOG_BACK_BUTTON_STATE_FIELDS: &[FieldDescriptor] = &[
    FieldDescriptor::new("is_hover", "bool", FieldKind::Retained),
    FieldDescriptor::new("on_click", "VoidCallback", FieldKind::Fresh),
    FieldDescriptor::new(
        "current_state",
        "Rc<Cell<PointerState>>",
        FieldKind::Fresh,
    ),
    FieldDescriptor::new(
        "updater",
        "StateUpdater<BlogBackButtonState>",
        FieldKind::Fresh,
    ),
];

#[cfg(feature = "portable-guest")]
const BLOG_BACK_BUTTON_STATE_SCHEMA: TypeSchema = TypeSchema::new(
    "BlogBackButtonState",
    StableId128::from_path(
        "aimer.type.v1",
        "website::components::back_button::BlogBackButtonState",
    ),
    BLOG_BACK_BUTTON_STATE_FIELDS,
);

#[cfg(feature = "portable-guest")]
impl AimerReflectionType for BlogBackButtonState {
    const TYPE_ID: StableId128 = StableId128::from_path(
        "aimer.type.v1",
        "website::components::back_button::BlogBackButtonState",
    );

    fn schema() -> &'static TypeSchema {
        &BLOG_BACK_BUTTON_STATE_SCHEMA
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncode for BlogBackButtonState {
    fn encode(&self, encoder: &mut Encoder<'_>) -> Result<(), EncodeError> {
        encoder.nested(|encoder| {
            encoder.field(&BLOG_BACK_BUTTON_STATE_FIELDS[0], |encoder| {
                self.is_hover.encode(encoder)
            })?;
            encoder.field(&BLOG_BACK_BUTTON_STATE_FIELDS[1], |_| Ok(()))?;
            encoder.field(&BLOG_BACK_BUTTON_STATE_FIELDS[2], |_| Ok(()))?;
            encoder.field(&BLOG_BACK_BUTTON_STATE_FIELDS[3], |_| Ok(()))
        })
    }
}

#[cfg(feature = "portable-guest")]
impl PortableApply for BlogBackButtonState {
    type Retained = bool;

    fn decode_retained(decoder: &mut Decoder<'_>) -> Result<Self::Retained, DecodeError> {
        decoder.nested(|decoder| {
            let is_hover = decoder
                .field(&BLOG_BACK_BUTTON_STATE_FIELDS[0])?
                .unwrap();
            let _ = decoder.field::<u8>(&BLOG_BACK_BUTTON_STATE_FIELDS[1])?;
            let _ = decoder.field::<u8>(&BLOG_BACK_BUTTON_STATE_FIELDS[2])?;
            let _ = decoder.field::<u8>(&BLOG_BACK_BUTTON_STATE_FIELDS[3])?;
            Ok(is_hover)
        })
    }

    fn apply_retained(&mut self, retained: Self::Retained) {
        self.is_hover = retained;
    }
}

impl StatefulWidget for BlogBackButton {
    type State = BlogBackButtonState;

    fn create_state(self) -> Self::State {
        Self::State {
            is_hover: false,
            on_click: self.on_click.clone(),
            current_state: Rc::default(),
            updater: StateUpdater::new(),
        }
    }
}

fn back_label_style(is_hover: bool, color: Color) -> TextStyle {
    TextStyle::new().color(color).text_decoration(if is_hover {
        TextDecoration::Underline
    } else {
        TextDecoration::None
    })
}

impl State<BlogBackButton> for BlogBackButtonState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let theme = ThemeData::of(ctx);
        let document = SvgDocument::from_svg(include_bytes!("../../assets/back-svgrepo-com.svg"))
            .expect("the bundled SVG should be valid");

        MouseRegion::new()
            .on_hover_enter({
                let updater = self.updater.clone();
                move || updater.set_state(|state| state.is_hover = true)
            })
            .on_hover_exit({
                let updater = self.updater.clone();
                move || updater.set_state(|state| state.is_hover = false)
            })
            .current_state(self.current_state.clone())
            .child(
                GestureDetector::new().on_tap(self.on_click.clone()).child(
                    Row::new().children([
                        Svg::new(document)
                            .style(
                                "#back_button_body",
                                SvgStyle::new().fill(theme.on_background_color),
                            )
                            .style(
                                "#back_button_head",
                                SvgStyle::new().fill(theme.on_background_color),
                            )
                            .width(16)
                            .height(16)
                            .boxed(),
                        SizedBox::new().width(8).boxed(),
                        Text::new("Back to blogs")
                            .text_align(TextAlign::MidCenter)
                            .text_style(back_label_style(self.is_hover, theme.on_background_color))
                            .boxed(),
                    ]),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use aimer::style::TextDecorationLine;

    use super::*;

    #[test]
    fn back_label_is_not_underlined_when_not_hovered() {
        assert_eq!(
            back_label_style(false, Color::BLACK).text_decoration.line,
            TextDecorationLine::NONE
        );
    }

    #[test]
    fn back_label_is_underlined_when_hovered() {
        assert_eq!(
            back_label_style(true, Color::BLACK).text_decoration.line,
            TextDecorationLine::UNDERLINE
        );
    }
}
