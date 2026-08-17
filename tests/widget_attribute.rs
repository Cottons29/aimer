use aimer::*;

#[widget(Stateless)]
struct TupleAttributeStateless(String);

impl StatelessWidget for TupleAttributeStateless {
    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new(self.0.clone())
    }
}

#[widget(Stateless)]
enum EnumAttributeStateless {
    Label(String),
    Empty,
}

impl StatelessWidget for EnumAttributeStateless {
    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new(match self {
            Self::Label(label) => label.clone(),
            Self::Empty => String::from("empty"),
        })
    }
}

#[widget(Stateful)]
struct TupleAttributeStateful(i32);

struct TupleAttributeStatefulState(i32);

impl StatefulWidget for TupleAttributeStateful {
    type State = TupleAttributeStatefulState;

    fn create_state(self) -> Self::State {
        TupleAttributeStatefulState(self.0)
    }
}

impl State<TupleAttributeStateful> for TupleAttributeStatefulState {
    fn init_state(&mut self, _: StateUpdater<Self>) {}

    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new(self.0.to_string())
    }
}

#[widget(Stateful)]
enum EnumAttributeStateful {
    Count(i32),
    Empty,
}

struct EnumAttributeStatefulState(i32);

impl StatefulWidget for EnumAttributeStateful {
    type State = EnumAttributeStatefulState;

    fn create_state(self) -> Self::State {
        EnumAttributeStatefulState(match self {
            Self::Count(count) => count,
            Self::Empty => 0,
        })
    }
}

impl State<EnumAttributeStateful> for EnumAttributeStatefulState {
    fn init_state(&mut self, _: StateUpdater<Self>) {}

    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new(self.0.to_string())
    }
}

fn assert_widget<W: Widget>() {}

#[test]
fn stateless_attribute_supports_tuple_structs_and_enums() {
    assert_widget::<TupleAttributeStateless>();
    assert_widget::<EnumAttributeStateless>();
}

#[test]
fn stateful_attribute_supports_tuple_structs_and_enums() {
    assert_widget::<TupleAttributeStateful>();
    assert_widget::<EnumAttributeStateful>();
    assert_eq!(TupleAttributeStateful(4).create_state().0, 4);
    assert_eq!(EnumAttributeStateful::Count(9).create_state().0, 9);
}
