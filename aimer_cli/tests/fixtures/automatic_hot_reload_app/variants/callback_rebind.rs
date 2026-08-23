use aimer::{AimerApp, BuildContext, Button, Container, State, StateUpdater, StatefulWidget, Text, Widget};

const EXPECTED_LABEL: &str = "CALLBACK REBOUND";
const EXPECTED_INCREMENT: u32 = 10;
const EXPECT_INCOMPATIBLE: bool = false;

#[derive(StatefulWidget)]
struct Counter;

impl Counter {
    #[inline]
    fn new() -> Self { Self }
}

struct CounterState { count: u32, updater: StateUpdater<Self> }

impl StatefulWidget for Counter {
    type State = CounterState;
    fn create_state(self) -> Self::State {
        CounterState { count: 0, updater: StateUpdater::new() }
    }
}

impl State<Counter> for CounterState {
    fn init_state(&mut self, updater: StateUpdater<Self>) { self.updater = updater; }
    fn build(&self, _: &BuildContext) -> impl Widget {
        let updater = self.updater.clone();
        Button::new()
            .on_press(move || updater.set_state(|state| state.count += EXPECTED_INCREMENT))
            .child(Text::new(format!("{EXPECTED_LABEL}: {}", self.count)))
    }
}

fn launch() { AimerApp::new().child(Container::new().child(Counter::new())).run(); }

#[aimer::main]
fn main() { launch(); }

#[cfg(test)]
mod generated_tests {
    use super::*;
    use aimer::anteros::{
        AbiStatus, CallbackEvent, ModelLimits, PropertyValue, StateBundleView, WidgetDocumentView,
        EVENT_BUTTON_PRESS, PROPERTY_TEXT_CONTENT,
    };
    use aimer_wasm_guest::GuestAdapter;

    fn model_limits() -> ModelLimits {
        ModelLimits::new(16_777_216, 65_536, 1_048_576, 16_777_216)
    }

    #[test]
    fn generated_adapter_builds_dispatches_and_transfers_state() {
        let mut active = GuestAdapter::new(
            __AimerGeneratedGuestProgram::default(),
            __AIMER_GENERATED_GUEST_LIMITS,
        )
        .unwrap();
        active.initialize(11).unwrap();
        active.manifest().unwrap();

        if EXPECT_INCOMPATIBLE {
            assert_eq!(active.build().unwrap_err().status(), AbiStatus::StateIncompatible);
            return;
        }

        let initial = active.build().unwrap();
        let initial = WidgetDocumentView::decode(&initial, model_limits()).unwrap();
        assert_eq!(initial.generation_id(), 11);
        assert_eq!(text_labels(&initial), [format!("{EXPECTED_LABEL}: 0")]);
        let binding = (0..initial.node_count())
            .find_map(|index| {
                initial
                    .node(index)
                    .unwrap()
                    .callbacks()
                    .find(|binding| binding.event_kind() == EVENT_BUTTON_PRESS)
            })
            .unwrap();
        println!("AUTOMATIC_CALLBACK_ID={:?}", binding.callback_id());
        let event = CallbackEvent::new(
            11,
            initial.document_revision(),
            binding.callback_id(),
            binding.event_kind(),
            binding.event_schema(),
            0,
            &[],
        )
        .encode(model_limits())
        .unwrap();
        let updated = active.dispatch_event(&event).unwrap().unwrap();
        let updated = WidgetDocumentView::decode(&updated, model_limits()).unwrap();
        assert_eq!(
            text_labels(&updated),
            [format!("{EXPECTED_LABEL}: {EXPECTED_INCREMENT}")],
        );

        let state = active.export_state().unwrap();
        let state_view = StateBundleView::decode(&state, model_limits()).unwrap();
        assert_eq!(state_view.source_generation(), 11);
        let retained_payload = state_view.entry(0).unwrap().payload().to_vec();

        let mut candidate = GuestAdapter::new(
            __AimerGeneratedGuestProgram::default(),
            __AIMER_GENERATED_GUEST_LIMITS,
        )
        .unwrap();
        candidate.initialize(12).unwrap();
        candidate.build().unwrap();
        candidate.import_state(&state).unwrap();
        let restored = candidate.build().unwrap();
        let restored = WidgetDocumentView::decode(&restored, model_limits()).unwrap();
        assert_eq!(
            text_labels(&restored),
            [format!("{EXPECTED_LABEL}: {EXPECTED_INCREMENT}")],
        );
        let exported = candidate.export_state().unwrap();
        let exported = StateBundleView::decode(&exported, model_limits()).unwrap();
        assert_eq!(exported.entry(0).unwrap().payload(), retained_payload);
    }

    fn text_labels(document: &WidgetDocumentView<'_>) -> Vec<String> {
        (0..document.node_count())
            .filter_map(|index| {
                document.node(index).unwrap().properties().find_map(|property| {
                    if property.property_id() != PROPERTY_TEXT_CONTENT {
                        return None;
                    }
                    match property.value() {
                        PropertyValue::StringRef(index) => {
                            Some(document.string(index).unwrap().to_owned())
                        }
                        _ => None,
                    }
                })
            })
            .collect()
    }
}
