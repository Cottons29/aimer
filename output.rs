#![feature(prelude_import)]
extern crate std;
#[prelude_import]
use std::prelude::rust_2024::*;
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

pub struct MyStatelessWidget {
    size: Size,
}
impl widget::Widget for MyStatelessWidget {
    fn to_element(&self, ctx: &widget::base::BuildContext)
        -> Box<dyn widget::Element> {
        use widget::StatelessWidget;
        let child_widget = self.build();
        let child_element = widget::Widget::to_element(&child_widget, ctx);
        Box::new(widget::StatelessElement { child: child_element })
    }
}
impl MyStatelessWidget {
    #[doc(hidden)]
    pub fn create_new(size: Size) -> Self { Self { size } }
}
#[doc =
"Constructor for [`MyStatelessWidget`].\n\nFields:\n- `size`: `Size`\n"]
#[macro_export]
macro_rules! MyStatelessWidget {
    (@ munch { size : $size_old : tt } size : $val : expr $
    (, $ ($rest : tt) *) ?) =>
    { MyStatelessWidget! (@ munch { size : ($val) } $ ($ ($rest) *) ?) };
    (@ munch { size : ($size : expr) }) =>
    { MyStatelessWidget :: create_new($size) }; (@ munch { size : () }) =>
    { compile_error! ("Missing field 'size'") };
    (@ munch { $ ($state : tt) * } $field : ident : $val : expr $
    (, $ ($rest : tt) *) ?) =>
    { compile_error! (concat! ("Unknown field: ", stringify! ($field))) };
    (@ munch { $ ($state : tt) * } $ ($rest : tt) *) =>
    { compile_error! (concat! ("Stuck on: ", stringify! ($ ($rest) *))) };
    ($ ($args : tt) *) =>
    { MyStatelessWidget! (@ munch { size : () } $ ($args) *) };
}

impl widget::StatelessWidget for MyStatelessWidget {
    fn build(&self) -> impl Widget { DummyWidget }
}

pub struct MyStatefulWidget {
    initial_val: i32,
}
impl widget::Widget for MyStatefulWidget {
    fn to_element(&self, ctx: &widget::base::BuildContext)
        -> Box<dyn widget::Element> {
        use widget::{StatefulWidget, State};
        let state = self.create_state();
        let child_element =
            {
                let child_widget = state.build();
                widget::Widget::to_element(&child_widget, ctx)
            };
        Box::new(widget::StatefulElement {
                child: child_element,
                state: Box::new(state),
            })
    }
}
impl MyStatefulWidget {
    #[doc(hidden)]
    pub fn create_new(initial_val: i32) -> Self { Self { initial_val } }
}
#[doc =
"Constructor for [`MyStatefulWidget`].\n\nFields:\n- `initial_val`: `i32`\n"]
#[macro_export]
macro_rules! MyStatefulWidget {
    (@ munch { initial_val : $initial_val_old : tt } initial_val : $val : expr
    $ (, $ ($rest : tt) *) ?) =>
    {
        MyStatefulWidget! (@ munch { initial_val : ($val) } $ ($ ($rest) *) ?)
    }; (@ munch { initial_val : ($initial_val : expr) }) =>
    { MyStatefulWidget :: create_new($initial_val) };
    (@ munch { initial_val : () }) =>
    { compile_error! ("Missing field 'initial_val'") };
    (@ munch { $ ($state : tt) * } $field : ident : $val : expr $
    (, $ ($rest : tt) *) ?) =>
    { compile_error! (concat! ("Unknown field: ", stringify! ($field))) };
    (@ munch { $ ($state : tt) * } $ ($rest : tt) *) =>
    { compile_error! (concat! ("Stuck on: ", stringify! ($ ($rest) *))) };
    ($ ($args : tt) *) =>
    { MyStatefulWidget! (@ munch { initial_val : () } $ ($args) *) };
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
    fn build(&self) -> impl Widget { DummyWidget }
}

extern crate test;
#[rustc_test_marker = "test_widgets_compile_and_construct"]
#[doc(hidden)]
pub const test_widgets_compile_and_construct: test::TestDescAndFn =
    test::TestDescAndFn {

        desc: test::TestDesc {
            name: test::StaticTestName("test_widgets_compile_and_construct"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "crates/widget/tests/widget_test.rs",
            start_line: 55usize,
            start_col: 4usize,
            end_line: 55usize,
            end_col: 38usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(test_widgets_compile_and_construct())),
    };
fn test_widgets_compile_and_construct() {
    let _ = MyStatelessWidget::create_new(Size { width: 10, height: 10 });
    let _ = MyStatefulWidget::create_new(42);
}

struct CollectionWidget {
    children: Vec<Box<dyn Widget>>,
}
impl CollectionWidget {
    #[doc(hidden)]
    pub fn create_new(children: Vec<Box<dyn Widget>>) -> Self {
        Self { children }
    }
}
#[doc =
"Constructor for [`CollectionWidget`].\n\nFields:\n- `children`: `Vec < Box < dyn Widget > >`\n"]
#[macro_export]
macro_rules! CollectionWidget {
    (@ munch { children : $children_old : tt } children : $val : expr $
    (, $ ($rest : tt) *) ?) =>
    {
        CollectionWidget!
        (@ munch
        {
            children :
            {
                let mut temp_vec = Vec :: new(); for item in $val
                { temp_vec.push(Box :: new(item) as _); } temp_vec
            }
        } $ ($ ($rest) *) ?)
    }; (@ munch { children : ($children : expr) }) =>
    { CollectionWidget :: create_new($children) }; (@ munch { children : () })
    => { compile_error! ("Missing field 'children'") };
    (@ munch { $ ($state : tt) * } $field : ident : $val : expr $
    (, $ ($rest : tt) *) ?) =>
    { compile_error! (concat! ("Unknown field: ", stringify! ($field))) };
    (@ munch { $ ($state : tt) * } $ ($rest : tt) *) =>
    { compile_error! (concat! ("Stuck on: ", stringify! ($ ($rest) *))) };
    ($ ($args : tt) *) =>
    { CollectionWidget! (@ munch { children : () } $ ($args) *) };
}

extern crate test;
#[rustc_test_marker = "test_collection_support"]
#[doc(hidden)]
pub const test_collection_support: test::TestDescAndFn =
    test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("test_collection_support"),
            ignore: false,
            ignore_message: ::core::option::Option::None,
            source_file: "crates/widget/tests/widget_test.rs",
            start_line: 71usize,
            start_col: 4usize,
            end_line: 71usize,
            end_col: 27usize,
            compile_fail: false,
            no_run: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(#[coverage(off)] ||
                test::assert_test_result(test_collection_support())),
    };
fn test_collection_support() {
    let widget = (/*ERROR*/);
    match (&widget.children.len(), &2) {
        (left_val, right_val) => {
            if !(*left_val == *right_val) {
                let kind = ::core::panicking::AssertKind::Eq;
                ::core::panicking::assert_failed(kind, &*left_val,
                    &*right_val, ::core::option::Option::None);
            }
        }
    };
}
#[rustc_main]
#[coverage(off)]
#[doc(hidden)]
pub fn main() -> () {
    extern crate test;
    test::test_main_static(&[&test_collection_support,
                    &test_widgets_compile_and_construct])
}
