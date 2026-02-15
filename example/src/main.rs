use color::prelude::{BasicColor, Color};
use engine::{DemoButton, MyStatefulWidget, OxidizeApp, widgets::DemoButton};
use std::sync::{Arc, RwLock};
use widget::{Widget, base::*};
mod other;

// use en

fn main() {
    let num = Arc::new(RwLock::new(12));
    let num_clone = num.clone();
    let on_click = move || {
        println!("Clicked on Button");
        let mut num = num_clone.write().unwrap();
        *num += 1;
    };

    OxidizeApp::start(
        MyStatefulWidget!(
                num: num,
                child: DemoButton!(
            label: "Click me!".to_string(),
            size: Size {width: 320, height: 200},
            background: Color::Hex(0x000000),
            on_click: on_click,
        ),
            )
        .into(),
    );
}
