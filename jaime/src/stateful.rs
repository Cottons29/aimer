use aimer::callback::VoidCallback;
use aimer::macros::widget;
use aimer::style::*;
use aimer::AimerApp;
use aimer::*;
use aimer::console::debug;

// this is the entry point of the app
pub fn start_counter() {
    // simply start the app with AimerApp::start
    AimerApp::start(CounterWidget::create_new(1, None))
}

// creating a widget with state
#[allow(non_snake_case)]
#[widget(Stateful)]
pub struct CounterWidget {
    pub initial_count: i32,
    #[constructor(default)]
    pub on_switch: Option<VoidCallback>,
}
// create a state for the CounterWidget
pub struct CounterState {
    count: i32,
    // on_switch: Option<VoidCallback>,
    updater: StateUpdater<Self>,
}

// implement the StatefulWidget trait for CounterWidget
impl StatefulWidget for CounterWidget {
    // define the state type for CounterWidget
    type State = CounterState;

    fn create_state(&self) -> CounterState {
        CounterState { count: self.initial_count, updater: StateUpdater::empty() }
    }
}
// implement the State trait for CounterState
impl State<CounterWidget> for CounterState {
    // setting up the updater for the state
    // if not init the state, the program will panic
    fn init_state(&mut self, _updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = _updater
    }

    // build the widget with state
    fn build(&self, _: &BuildContext) -> impl Widget {
        debug!("self.count: {}", self.count);
        let updater = self.updater.clone();
        Container!(
            color: Colors::Gray,
            padding: LayoutSpacing!(top: Spacing::Px(20)),
            child: Flex!(
                direction: LayoutDirection::Column,
                vertical_alignment: BoxAlignment::Center,
                horizontal_alignment: BoxAlignment::Center,
                children: [
                    // Text!(
                    //     "អរគុណ 你哈皮  With State 你好 きみなと  👉",
                    //     // "Stateful Counter",
                    //     text_align: TextAlign::MidCenter,
                    //     text_style: TextStyle!(
                    //         font_size: 15,
                    //         color: Colors::Black,
                    //         font_weight: FontWeight::Bolder,
                    //     )
                    // ),


                    SizedBox!(height: 50),

                    Text!(
                        {
                            debug!("Clicked: {}", self.count);
                            format!("Clicked: {}", self.count)
                        },
                        text_style: TextStyle!(
                            font_size: 25,
                            color: Colors::Black,
                        )
                    ),

                    SizedBox!(height: 50),

                    Container!(
                        width: 200,
                        height: 50,
                        // color: Colors::Yellow,
                        child: Button!(
                            on_press: move || {
                                // println!("Button pressed");
                                println!("Button pressed with state : {}", updater.read_state().count);
                                updater.set_state(|state| {
                                    state.count += 1;
                                });
                            },
                            decoration: BoxDecoration!(background_color: Color::BLUE),
                            child: Container!(
                                child: Text!(
                                    "Increase",
                                    text_align: TextAlign::MidCenter,
                                    text_style: TextStyle!(
                                        color: Colors::Black,
                                    )
                                )
                            )
                        )
                    ),

                    // Container!(
                    //     width: 200,
                    //     height: 50,
                    //     margin: LayoutSpacing!(top : Spacing::Px(10)),
                    //     child: Button!(
                    //         on_press:  {
                    //             let updater = self.updater.clone();
                    //             move || {
                    //
                    //                 updater.set_state(|state| {
                    //                     state.count -= 1;
                    //                 });
                    //
                    //             }
                    //         },
                    //         on_double_press: {
                    //             || {
                    //                 console::debug!("Double click on button");
                    //             }
                    //         },
                    //         decoration: BoxDecoration!(background_color: Color::BLUE),
                    //         child: Container!(
                    //             child: Text!(
                    //                 "Decrease",
                    //                 text_align: TextAlign::MidCenter,
                    //                 text_style: TextStyle!(
                    //                     color: Colors::Black,
                    //                 )
                    //             )
                    //         )
                    //     )
                    // ),

                    // Container!(
                    //     width: 200,
                    //     height: 50,
                    //     margin: LayoutSpacing!(top : Spacing::Px(10)),
                    //     child: Button!(
                    //         on_press:  {
                    //             // let _ = self.on_switch.clone();
                    //             move || {
                    //                 console::debug!("Switching to Stateful 2");
                    //             }
                    //         },
                    //         decoration: BoxDecoration!(background_color: Colors::Blue),
                    //         child: Text!(
                    //             "Switch to Stateful 2",
                    //             text_align: TextAlign::MidCenter,
                    //             text_style: TextStyle!(
                    //                 color: Colors::White,
                    //             )
                    //         )
                    //     )
                    // )
                ]
            )
        )
    }
}
