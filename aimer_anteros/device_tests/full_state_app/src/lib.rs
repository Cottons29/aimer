//! Standalone application contract for milestone-V full-state device proofs.

mod guest;
mod guest_support;

pub use guest::FullStateGuest;
pub use guest_support::{CALLBACK_ID, HOT_RELOAD_LIMITS, MODEL_LIMITS};

#[cfg(not(target_arch = "wasm32"))]
use aimer::{AimerApp, Text};

/// Starts the native host shell used before the interpreted guest is committed.
#[cfg(not(target_arch = "wasm32"))]
#[aimer::main]
pub fn app_main() {
    AimerApp::start(Text::new("Aimer full-state proof: waiting for guest"));
}

#[cfg(test)]
mod tests {
    use crate::{CALLBACK_ID, FullStateGuest, HOT_RELOAD_LIMITS, MODEL_LIMITS};
    use aimer_wasm_guest::anteros::{
        CallbackEvent, ManifestView, PropertyValue, StableId128, StateBundleView, Version,
        WidgetDocumentView, EVENT_BUTTON_PRESS, PROPERTY_TEXT_CONTENT,
    };
    use aimer_wasm_guest::GuestAdapter;

    #[test]
    fn public_guest_builds_the_initial_persistent_counter_ui() {
        let mut guest = GuestAdapter::new(FullStateGuest::default(), HOT_RELOAD_LIMITS).unwrap();
        guest.initialize(17).unwrap();

        let manifest = guest.manifest().unwrap();
        let document = guest.build().unwrap();
        let manifest = ManifestView::decode(&manifest, MODEL_LIMITS).unwrap();
        let document = WidgetDocumentView::decode(&document, MODEL_LIMITS).unwrap();

        assert_eq!(manifest.program_id(), StableId128::from_bytes([0x11; 16]));
        assert_eq!(document.generation_id(), 17);
        assert_eq!(document.document_revision(), 0);
        assert_eq!(text_labels(&document), ["FULL STATE / INITIAL", "counter: 0", "increment +1"]);
        assert_eq!(
            document
                .node(document.root_node())
                .unwrap()
                .callbacks()
                .next()
                .unwrap()
                .callback_id(),
            CALLBACK_ID
        );
    }

    #[test]
    fn public_callback_updates_and_exports_persistent_state() {
        let mut guest = GuestAdapter::new(FullStateGuest::default(), HOT_RELOAD_LIMITS).unwrap();
        guest.initialize(23).unwrap();
        let event = CallbackEvent::new(
            23,
            1,
            CALLBACK_ID,
            EVENT_BUTTON_PRESS,
            Version::new(1, 0),
            1,
            &[],
        )
        .encode(MODEL_LIMITS)
        .unwrap();

        let updated = guest.dispatch_event(&event).unwrap().unwrap();
        let state = guest.export_state().unwrap();
        let updated = WidgetDocumentView::decode(&updated, MODEL_LIMITS).unwrap();
        let state = StateBundleView::decode(&state, MODEL_LIMITS).unwrap();

        assert_eq!(updated.document_revision(), 1);
        assert_eq!(text_labels(&updated)[1], "counter: 1");
        assert_eq!(state.entry(0).unwrap().payload(), [1]);
    }

    fn text_labels<'a>(document: &'a WidgetDocumentView<'a>) -> Vec<&'a str> {
        (0..document.node_count())
            .filter_map(|index| {
                document.node(index).unwrap().properties().find_map(|property| {
                    if property.property_id() != PROPERTY_TEXT_CONTENT {
                        return None;
                    }
                    match property.value() {
                        PropertyValue::StringRef(index) => document.string(index),
                        _ => None,
                    }
                })
            })
            .collect()
    }
}
