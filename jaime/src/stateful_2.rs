use crate::input::{AsyncTextFieldCallback, TextField};
use aimer::AimerApp;
use aimer::macros::widget;
use aimer::style::*;
use aimer::*;
use uuid::Uuid;
#[allow(unused)]
pub fn start_my_list() {
    AimerApp::start(MyList::new().boxed())
}

#[widget(Stateful)]
pub struct MyList {
}

impl MyList {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub struct ListItem {
    id: String,
    text: String,
}

pub struct MyListState {
    list: Vec<ListItem>,
    input_controller: TextFieldController,
    is_cooldown: bool,
    updater: StateUpdater<Self>,
}

impl Drop for ListItem {
    fn drop(&mut self) {
        println!("Dropping ListItem {}", self.id);
    }
}

impl StatefulWidget for MyList {
    type State = MyListState;
    fn create_state(&self) -> Self::State {
        MyListState { list: vec![], updater: StateUpdater::empty(), input_controller: TextFieldController::new(), is_cooldown: false }
    }
}

impl State<MyList> for MyListState {
    fn init_state(&mut self, _updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = _updater;
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        Container::new()
            .color(Colors::White.into())
            .padding(LayoutSpacing { top: Spacing::Px(45), ..Default::default() })
            .child(
                Column::new()
                    .children(vec![
                        Container::new()
                            .height(Dimension::Px(90.0))
                            .box_decoration(BoxDecoration::new()
                                .background_color(Colors::Gray.alpha(128)))
                            .padding(LayoutSpacing::vertical(Spacing::Px(10)))
                            .child(
                                Row::new()
                                    .gaps(LayoutSpacing::all(Spacing::Px(10)))
                                    .children(vec![
                                        Container::new()
                                            .width(Dimension::Px(70.0))
                                            .margin(LayoutSpacing { left: Spacing::Px(10), ..Default::default() })
                                            .child(
                                                Text::new(format!("Item {}", self.list.len()))
                                                    .text_align(TextAlign::MidLeft)
                                                    .text_style(TextStyle::new()
                                                        .font_size(20)
                                                        .color(Colors::Black))
                                            ).boxed(),
                                        Container::new()
                                            .width(Dimension::Px(400.0))
                                            .padding(LayoutSpacing::all(Spacing::Px(10)))
                                            .child(
                                                TextField::new()
                                                    .padding(LayoutSpacing::all(Spacing::Px(10)))
                                                    .text_align(TextAlign::MidLeft)
                                                    .controller(self.input_controller.clone())
                                                    .input_type(InputType::Text)
                                                    .on_changed({
                                                        let is_cooldown = self.is_cooldown;
                                                        AsyncTextFieldCallback(move |item: String| async move {
                                                            if !is_cooldown {
                                                                console::debug!("Input changed: {}", item);
                                                            }else{
                                                                console::debug!("Input changed: COOLDOWN");
                                                            }
                                                        })
                                                    })
                                                    .prompt("Input any here....")
                                                    .decoration(BoxDecoration::new()
                                                        .background_color(Colors::Gray.alpha(140))
                                                        .border(BoxBorder::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Black)
                                                                .stroke(2),
                                                        ))
                                                        .outline(BoxOutline::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Black)
                                                                .stroke(2),
                                                        )))
                                                    .hover_decoration(BoxDecoration::new()
                                                        .background_color(Colors::Gray.alpha(70))
                                                        .border(BoxBorder::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Black)
                                                                .stroke(2),
                                                        ))
                                                        .outline(BoxOutline::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Green)
                                                                .stroke(2),
                                                        )))
                                                    .focus_decoration(BoxDecoration::new()
                                                        .background_color(Colors::Gray.alpha(10))
                                                        .border(BoxBorder::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Green)
                                                                .stroke(2),
                                                        ))
                                                        .outline(BoxOutline::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Black)
                                                                .stroke(2),
                                                        )))
                                            ).boxed(),
                                        Container::new()
                                            .padding(LayoutSpacing::all(Spacing::Px(10)))
                                            .width(Dimension::Px(100.0))
                                            .child(
                                                Button::new()
                                                    .on_press({
                                                        let updater = self.updater.clone();
                                                         move || {
                                                            println!("Button pressed with text: {}", updater.read_state().input_controller.text());
                                                            updater.set_state(|state| {
                                                                let uuid = Uuid::new_v4().to_string();
                                                                state.list.push(ListItem {
                                                                    id: uuid,
                                                                    text: state.input_controller.take(),
                                                                })
                                                            });
                                                        }
                                                    })
                                                    .decoration(BoxDecoration::new()
                                                        .background_color(Colors::Gray.alpha(150))
                                                        .border(BoxBorder::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Colors::Black)
                                                                .stroke(2),
                                                        ))
                                                        .outline(BoxOutline::all(
                                                            BorderSlice::new()
                                                                .style(BorderStyle::Solid)
                                                                .color(Color::Transparent)
                                                                .stroke(2),
                                                        )))
                                                    .child(
                                                        Container::new()
                                                            .child(
                                                                Text::new("Add Item")
                                                                    .text_align(TextAlign::MidCenter)
                                                                    .text_style(TextStyle::new()
                                                                        .color(Colors::Black)
                                                                        .font_size(15))
                                                            )
                                                    )
                                            ).boxed(),
                                    ]).boxed()
                            ).boxed(),
                        Scrollable::new(
                            Column::new()
                                .children(self.list.iter().map(|item| {
                                    Container::new()
                                        .margin(LayoutSpacing { top: Spacing::Px(10), ..Default::default() })
                                        .color(Colors::Blue.alpha(15).into())
                                        .height(Dimension::Px(50.0))
                                        .child(
                                            Row::new()
                                                .children(vec![
                                                    Expanded::new()
                                                        .child(
                                                            Container::new()
                                                                .padding(LayoutSpacing { left: Spacing::Px(10), ..Default::default() })
                                                                .child(
                                                                    Text::new(format!("Item : {}", item.text))
                                                                        .text_align(TextAlign::MidLeft)
                                                                        .text_style(TextStyle::new()
                                                                            .color(Colors::Black)
                                                                            .font_size(15))
                                                                ),
                                                        ).boxed(),
                                                    Container::new()
                                                        .width(Dimension::Px(100.0))
                                                        .height(Dimension::Px(50.0))
                                                        .child(
                                                            Button::new()
                                                                .on_press({
                                                                    let item_id = item.id.clone();
                                                                    let updater = self.updater.clone();
                                                                     move || {
                                                                        let another_item_id = item_id.clone();
                                                                        updater.set_state( move |state| {
                                                                            state.list.retain(|i| i.id != another_item_id);
                                                                        });
                                                                    }
                                                                })
                                                                .decoration(BoxDecoration::new().background_color(Colors::Gray))
                                                                .child(
                                                                    Container::new()
                                                                        .child(
                                                                            Text::new("Delete")
                                                                                .text_align(TextAlign::MidCenter)
                                                                                .text_style(TextStyle::new()
                                                                                    .color(Colors::Black)
                                                                                    .font_size(15))
                                                                        )
                                                                )
                                                        ).boxed(),
                                                ]).boxed()
                                        ).boxed()
                                }).collect::<Vec<Box<dyn Widget>>>()
                                )
                        ).boxed()
                    ]).boxed()
            )
    }
}
