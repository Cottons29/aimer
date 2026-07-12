use crate::ScrollBar;
use crate::input::{AsyncTextFieldCallback, TextField};
use aimer::AimerApp;
use aimer::macros::widget;
use aimer::style::*;
use aimer::*;
use uuid::Uuid;
#[allow(unused)]
pub fn start_my_list() {
    AimerApp::start(MyList::create_new())
}

#[widget(Stateful)]
pub struct MyList {
    // #[constructor(default)]
    // pub on_switch: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
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
        Container!(
            color: Colors::White,
            padding: LayoutSpacing!(top: Spacing::Px(45)),
            child: Column!(
                children: [
                    Container!(
                        height: 90.0,
                        box_decoration : BoxDecoration !(
                            background_color: Colors::Gray.alpha(128),
                        ),
                        padding: LayoutSpacing::vertical(Spacing::Px(10)),
                        child: Row!(
                            gaps: LayoutSpacing::all(Spacing::Px(10)),
                            // vertical_alignment: BoxAlignment::Center,
                            children: [


                                Container!(
                                    width: 70,
                                    margin: LayoutSpacing!(left: Spacing::Px(10)),
                                    child: Text!(
                                        format!("Item {}", self.list.len()),
                                        text_align: TextAlign::MidLeft,
                                        text_style: TextStyle!(
                                            font_size: 20,
                                            color: Colors::Black,
                                        )
                                    )
                                ),


                                Container!(
                                    width: 400,
                                    padding: LayoutSpacing::all(Spacing::Px(10)),
                                    child: TextField!(
                                        padding: LayoutSpacing::all(Spacing::Px(10)),
                                        text_align: TextAlign::MidLeft,
                                        controller: self.input_controller.clone(),
                                        input_type: InputType::Text,
                                        on_changed: {
                                            let is_cooldown = self.is_cooldown;
                                            AsyncTextFieldCallback(move |item: String| async move {
                                                if !is_cooldown {
                                                    console::debug!("Input changed: {}", item);
                                                }else{
                                                    console::debug!("Input changed: COOLDOWN");
                                                }
                                            })
                                        },
                                        prompt: "Input any here....",
                                        decoration: BoxDecoration!(

                                            background_color: Colors::Gray.alpha(140),
                                            border: BoxBorder::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Black,
                                                    stroke: 2,
                                                ),
                                            ),
                                            outline: BoxOutline::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Black,
                                                    stroke: 2,

                                                )
                                            ),
                                        ),
                                        hover_decoration: BoxDecoration!(

                                            background_color: Colors::Gray.alpha(70),
                                            border: BoxBorder::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Black,
                                                    stroke: 2,

                                                )
                                            ),
                                            outline: BoxOutline::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Green,
                                                    stroke: 2,

                                                )
                                            ),
                                        ),
                                        focus_decoration: BoxDecoration!(

                                            background_color: Colors::Gray.alpha(10),
                                            border: BoxBorder::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Green,
                                                    stroke: 2,

                                                )
                                            ),
                                            outline: BoxOutline::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Black,
                                                    stroke: 2,

                                                )
                                            ),
                                        )
                                    )
                                ),

                                Container!(
                                    padding: LayoutSpacing::all(Spacing::Px(10)),
                                    width: 100,
                                    child: Button!(
                                        on_press: {
                                            let updater = self.updater.clone();
                                             move || {
                                                println!("Button pressed with text: {}", updater.read_state().input_controller.text());
                                                updater.set_state(|state| {
                                                    let uuid = Uuid::new_v4().to_string();
                                                    // println!("Button pressed with text: {}", state.input_controller.text());
                                                    state.list.push(ListItem {
                                                        id: uuid,
                                                        text: state.input_controller.take(),
                                                    })
                                                });
                                            }
                                        },
                                        decoration: BoxDecoration!(
                                            background_color: Colors::Gray.alpha(150),
                                            border: BoxBorder::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Colors::Black,
                                                    stroke: 2,

                                                )
                                            ),
                                            outline: BoxOutline::all(
                                                BorderSlice!(
                                                    style: BorderStyle::Solid,
                                                    color: Color::Transparent,
                                                    stroke: 2,

                                                )
                                            ),
                                        ),
                                        child: Container!(

                                            child: Text!(
                                                "Add Item",
                                                text_align: TextAlign::MidCenter,
                                                text_style: TextStyle!(
                                                    color: Colors::Black,
                                                    font_size: 15,
                                                )
                                            )
                                        )
                                    )
                                ),
                            ]
                        )
                    ),
                    Scrollable!(
                        child: Column!(
                            children: self.list.iter().map(|item| {
                                Container!(
                                    margin: LayoutSpacing!(top: Spacing::Px(10)),
                                    color: Colors::Blue.alpha(15),
                                    height: 50,
                                    child: Row!(
                                        children: [
                                            Expanded!(
                                                child: Container!(
                                                    padding: LayoutSpacing!(left: Spacing::Px(10)),
                                                    child: Text!(
                                                        format!("Item : {}", item.text),
                                                        text_align: TextAlign::MidLeft,
                                                        text_style: TextStyle!(
                                                            color: Colors::Black,
                                                            font_size: 15,
                                                        )
                                                    )
                                                ),
                                            ),
                                            Container!(
                                                width: 100,
                                                height: 50,
                                                child: Button!(
                                                    on_press: {
                                                        let item_id = item.id.clone();
                                                        let updater = self.updater.clone();
                                                         move || {
                                                            let another_item_id = item_id.clone();
                                                            println!("Clicked on item with id: {}", item_id);
                                                            // println!("List items: {:#?}", updater.read_state().list);
                                                            updater.set_state( move |state| {
                                                                // println!("Deleting item with id: {}", id);
                                                                state.list.retain(|i| i.id != another_item_id);
                                                            });
                                                        }
                                                    },
                                                    decoration: BoxDecoration!(background_color: Colors::Gray),
                                                    child: Container!(
                                                        child: Text!(
                                                            "Delete",
                                                            text_align: TextAlign::MidCenter,
                                                            text_style: TextStyle!(
                                                                color: Colors::Black,
                                                                font_size: 15,
                                                            )
                                                        )
                                                    )
                                                )
                                            )
                                        ]
                                    )
                                )
                            }).collect::<Vec<Box<dyn Widget>>>()
                        )
                    )
                ]
            )
        )
    }
}
