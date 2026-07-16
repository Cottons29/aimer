use aimer_container::ZeroSizedBox;
use aimer_macro::widget;
use aimer_widget as widget;
use aimer_widget::base::BuildContext;
use aimer_widget::{StatelessWidget, Widget};

#[widget(Stateless)]
#[derive(Clone)]
pub struct MarkdownViewer;

impl StatelessWidget for MarkdownViewer {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        ZeroSizedBox
    }
}
