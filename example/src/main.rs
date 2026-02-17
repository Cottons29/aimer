use color::prelude::{BasicColor, Color};
use container::{Container, ZeroSizedBox};
use engine::OxidizeApp;
use widget::{Widget, base::*};
use other::MyWidget;
mod other;

fn get_widget() -> impl Widget {
    Container!(
        size: Size {width: 200, height: 300},
        color: Color::Basic(BasicColor::Blue),
        child: Container!(
            size : Size{width: 100, height: 150},
            color: Color::Basic(BasicColor::Green),
            child: ZeroSizedBox,
        )
    )
}

fn main() {
    // let num = Arc::new(RwLock::new(12));
    // let num_clone = num.clone();
    // let on_click = move || {
    //     println!("Clicked on Button");
    //     let mut num = num_clone.write().unwrap();
    //     *num += 1;
    // };
    //
    // OxidizeApp::start(MyStatefulWidget!(
    //         num: num,
    //         child: DemoButton!(
    //     label: "Click me!".to_string(),
    //     size: Size {width: 320, height: 200},
    //     background: Color::Hex(0x000000),
    //     on_click: on_click,
    // )));
    //
    let my_widget = get_widget();
    OxidizeApp::start(MyWidget!(num: 100));
}
