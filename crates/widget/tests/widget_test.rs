use widget::widget_attr::widget;
use widget::Constructor;
use widget::Widget;
use widget::base::BuildContext;
use widget::Element;
use widget::base::Size;
use widget::base::Vec2d;

// Dummy implementation for testing
pub struct DummyWidget;
impl Widget for DummyWidget {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(DummyElement)
    }
}
pub struct DummyElement;
impl Element for DummyElement {
    fn draw(&self, _ctx: &BuildContext) {}
}

#[widget(Stateless)]
pub struct MyStatelessWidget {
    size: Size,
}

impl widget::StatelessWidget for MyStatelessWidget {
    fn build(&self) -> impl Widget {
        DummyWidget
    }
}

#[widget(Stateful)]
pub struct MyStatefulWidget {
    initial_val: i32,
}

impl widget::StatefulWidget for MyStatefulWidget {
    type State = MyState;
    fn create_state(&self) -> Self::State {
        MyState { val: self.initial_val }
    }
}

pub struct MyState {
    val: i32,
}

impl widget::State<MyStatefulWidget> for MyState {
    fn build(&self) -> impl Widget {
        DummyWidget
    }
}

#[test]
fn test_widgets_compile_and_construct() {
    let _ = MyStatelessWidget!(
        size: Size { width: 10, height: 10 }
    );
    
    let _ = MyStatefulWidget!(
        initial_val: 42
    );
}

#[derive(Constructor)]
struct CollectionWidget {
    children: Vec<Box<dyn Widget>>,
}

#[derive(Constructor)]
struct DynIterWidget {
    #[constructor(dyn_iter)]
    children: Vec<Box<dyn Widget>>,
}

#[test]
fn test_collection_support() {
    let widget = CollectionWidget!(
        children: [DummyWidget, DummyWidget]
    );
    assert_eq!(widget.children.len(), 2);

    let widget2 = DynIterWidget!(
        children: [DummyWidget, DummyWidget, DummyWidget]
    );
    assert_eq!(widget2.children.len(), 3);

    let multi_type = CollectionWidget!(
        children: [
            DummyWidget,
            MyStatelessWidget!(size: Size { width: 10, height: 10 }),
        ]
    );
    assert_eq!(multi_type.children.len(), 2);
}
