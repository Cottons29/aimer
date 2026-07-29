use std::time::Duration;

use aimer::callback::VoidCallback;
use aimer::macros::widget;
use aimer::style::*;
use aimer::{AimerApp, *};

/// Entries of the anchored menu opened by the first trigger.
const MENU_ITEMS: [&str; 4] = ["Profile", "Settings", "Shortcuts", "Sign out"];

const MENU_ITEM_HEIGHT: f32 = 40.0;
const MENU_PADDING: u32 = 8;
const TRIGGER_HEIGHT: f32 = 44.0;

/// Starts an anchored overlay showcase built on the `Floating` primitive.
///
/// The page presents three triggers that all use the same primitive:
///
/// * a dropdown menu below the top-left trigger, closing on selection, on an
///   outside press, or on `Escape`,
/// * a tooltip-like panel above the center trigger,
/// * a trigger pinned to the bottom-right corner, where the requested
///   `Bottom` side does not fit and `OverflowPolicy::Flip` moves the panel
///   above the trigger while the cross axis slides back into the viewport.
pub fn start_floating_example() {
    AimerApp::start(FloatingShowcase::new().boxed())
}

#[widget(Stateful)]
pub struct FloatingShowcase {}

impl FloatingShowcase {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct FloatingShowcaseState {
    menu_anchor: AnchorHandle,
    tooltip_anchor: AnchorHandle,
    corner_anchor: AnchorHandle,
    selected: Option<&'static str>,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for FloatingShowcase {
    type State = FloatingShowcaseState;

    fn create_state(&self) -> Self::State {
        FloatingShowcaseState {
            menu_anchor: AnchorHandle::new(),
            tooltip_anchor: AnchorHandle::new(),
            corner_anchor: AnchorHandle::new(),
            selected: None,
            updater: StateUpdater::empty(),
        }
    }
}

impl State<FloatingShowcase> for FloatingShowcaseState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::Rgb(17, 24, 39))
            .padding(LayoutSpacing::all(Spacing::Px(32)))
            .child(Column::new().children(vec![
                    Container::new()
                        .height(Dimension::Px(34.0))
                        .child(
                            Text::new("Floating: panels anchored to a trigger")
                                .text_align(TextAlign::MidLeft)
                                .text_style(
                                    TextStyle::new().font_size(24).color(Color::WHITE),
                                ),
                        )
                        .boxed(),
                    Container::new()
                        .height(Dimension::Px(24.0))
                        .child(
                            Text::new(match self.selected {
                                Some(item) => format!("Selected: {item}"),
                                None => "Nothing selected yet".to_string(),
                            })
                            .text_align(TextAlign::MidLeft)
                            .text_style(
                                TextStyle::new()
                                    .font_size(16)
                                    .color(Color::Rgb(148, 163, 184)),
                            ),
                        )
                        .boxed(),
                    SizedBox::new().height(24).boxed(),
                    Container::new()
                        .height(Dimension::Px(TRIGGER_HEIGHT))
                        .child(
                            Row::new()
                                .gaps(LayoutSpacing::all(Spacing::Px(16)))
                                .children(vec![
                                    self.menu_trigger(),
                                    self.tooltip_trigger(),
                                ]),
                        )
                        .boxed(),
                    Expanded::new()
                        .child(
                            Align::new()
                                .alignment(Alignment::BotRight)
                                .child(self.corner_trigger()),
                        )
                        .boxed(),
                ]))
    }
}

impl FloatingShowcaseState {
    /// Opens a dropdown menu below its trigger, aligned on the leading edge.
    fn menu_trigger(&self) -> AnyWidget {
        let anchor = self.menu_anchor.clone();
        let updater = self.updater.clone();
        trigger("Open menu", anchor.clone(), move || {
            Floating::new()
                .anchor(anchor.clone())
                .side(FloatingSide::Bottom)
                .align(FloatingAlign::Start)
                .gap(6.0)
                .animation(enter_animation())
                .child(menu_panel(updater.clone()))
                .show();
        })
    }

    /// Opens a tooltip-like panel above its trigger, centered on it.
    fn tooltip_trigger(&self) -> AnyWidget {
        let anchor = self.tooltip_anchor.clone();
        trigger("Show hint", anchor.clone(), move || {
            Floating::new()
                .anchor(anchor.clone())
                .side(FloatingSide::Top)
                .align(FloatingAlign::Center)
                .gap(8.0)
                .animation(enter_animation())
                .child(hint_panel())
                .show();
        })
    }

    /// Opens a menu from the bottom-right corner, where the requested side
    /// does not fit and the overflow policy flips the panel above the trigger.
    fn corner_trigger(&self) -> AnyWidget {
        let anchor = self.corner_anchor.clone();
        let updater = self.updater.clone();
        trigger("Flip near the edge", anchor.clone(), move || {
            Floating::new()
                .anchor(anchor.clone())
                .side(FloatingSide::Bottom)
                .align(FloatingAlign::End)
                .gap(6.0)
                .overflow(OverflowPolicy::Flip)
                .animation(enter_animation())
                .child(menu_panel(updater.clone()))
                .show();
        })
    }
}

/// Wraps a button in an [`Anchor`] so a panel can be pinned to it.
fn trigger(label: &str, handle: AnchorHandle, on_press: impl Into<VoidCallback>) -> AnyWidget {
    Anchor::new()
        .handle(handle)
        .child(
            Container::new()
                .width(Dimension::Px(200.0))
                .height(Dimension::Px(TRIGGER_HEIGHT))
                .child(
                    Button::new()
                        .on_press(on_press)
                        .decoration(
                            BoxDecoration::new()
                                .background_color(Color::Rgb(59, 130, 246))
                                .border_radius(10),
                        )
                        .child(
                            Text::new(label)
                                .text_align(TextAlign::MidCenter)
                                .text_style(TextStyle::new().font_size(16).color(Color::WHITE)),
                        ),
                ),
        )
        .boxed()
}

/// Builds the dropdown content: one row per [`MENU_ITEMS`] entry.
fn menu_panel(updater: StateUpdater<FloatingShowcaseState>) -> AnyWidget {
    Container::new()
        .width(Dimension::Px(200.0))
        .height(Dimension::Px(
            MENU_ITEMS.len() as f32 * MENU_ITEM_HEIGHT + MENU_PADDING as f32 * 2.0,
        ))
        .padding(LayoutSpacing::all(Spacing::Px(MENU_PADDING)))
        .box_decoration(
            BoxDecoration::new()
                .background_color(Color::WHITE)
                .border_radius(12)
                .box_shadow(vec![
                    BoxShadow::new()
                        .color(Color::BLACK.with_opacity(70))
                        .blur(18.0)
                        .offset_y(8.0),
                ]),
        )
        .child(
            Column::new().children(
                MENU_ITEMS
                    .iter()
                    .map(|item| menu_item(item, updater.clone()))
                    .collect::<Vec<AnyWidget>>(),
            ),
        )
        .boxed()
}

/// Builds a single selectable menu row.
///
/// Selecting an item records the choice and dismisses the topmost overlay
/// entry, which is this panel.
fn menu_item(item: &'static str, updater: StateUpdater<FloatingShowcaseState>) -> AnyWidget {
    Container::new()
        .height(Dimension::Px(MENU_ITEM_HEIGHT))
        .child(
            Button::new()
                .on_press(move || {
                    updater.set_state(move |state| state.selected = Some(item));
                    ModalController::dismiss_top();
                })
                .decoration(
                    BoxDecoration::new()
                        .background_color(Color::Transparent)
                        .border_radius(8),
                )
                .child(
                    Text::new(item)
                        .text_align(TextAlign::MidLeft)
                        .text_style(TextStyle::new().font_size(16).color(Color::Rgb(31, 41, 55))),
                ),
        )
        .boxed()
}

/// Builds the tooltip-like panel shown above its trigger.
fn hint_panel() -> AnyWidget {
    Container::new()
        .width(Dimension::Px(260.0))
        .height(Dimension::Px(72.0))
        .padding(LayoutSpacing::all(Spacing::Px(12)))
        .box_decoration(
            BoxDecoration::new()
                .background_color(Color::Rgb(30, 41, 59))
                .border_radius(10),
        )
        .child(
            Text::new("Press outside or hit Escape to dismiss.")
                .text_align(TextAlign::MidCenter)
                .text_style(TextStyle::new().font_size(15).color(Color::WHITE)),
        )
        .boxed()
}

/// The enter and exit transition shared by every panel of this showcase.
fn enter_animation() -> ModalAnimation {
    ModalAnimation::new()
        .enter_duration(Duration::from_millis(160))
        .exit_duration(Duration::from_millis(120))
        .enter_curve(Curve::EaseOut)
        .exit_curve(Curve::EaseIn)
        .content_scale_from(0.94)
}
