use widget::{StatefulWidget, StatelessWidget, Widget};
use widget::widget_attr::widget;
use container::{Container, ZeroSizedBox};
use widget::base::*;


#[widget(StatelessWidget)]
pub struct MyWidget {
    num: u32,
}


impl StatelessWidget for MyWidget {
    fn build(&self) -> impl Widget {
        Container!(
            size: Size {width: 100, height: 50},
            color: Color::Basic(BasicColor::Green),
            child: ZeroSizedBox
        )
    }
}

