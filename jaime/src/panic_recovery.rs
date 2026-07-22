use aimer::macros::widget;
use aimer::*;

pub fn start_panic_recovery_example() {
    AimerApp::start(Container::new().child(PanicRecoveryExample::new()))
}

struct MissingProviderValue;

#[widget(Stateless)]
#[derive(Clone)]
struct PanicRecoveryExample {}

impl PanicRecoveryExample {
    fn new() -> Self {
        Self {}
    }
}

impl StatelessWidget for PanicRecoveryExample {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let _missing = ProviderHandle::<MissingProviderValue>::of(ctx);

        Container::new().child(Text::new(
            "This is replaced by the recovered red error screen.",
        ))
    }
}
