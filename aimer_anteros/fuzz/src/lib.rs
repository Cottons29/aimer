use aimer_anteros::{
    CallbackEventView, DecodeLimits, Envelope, ManifestView, ModelLimits, StateBundleView,
    WidgetDocumentView,
};

const DECODE_LIMITS: DecodeLimits = DecodeLimits::new(256, 16, 64);
const MODEL_LIMITS: ModelLimits = ModelLimits::new(512, 16, 64, 128).max_widget_depth(16);

pub fn fuzz_envelope(data: &[u8]) {
    if let Ok(envelope) = Envelope::decode(data, DECODE_LIMITS) {
        std::hint::black_box(envelope);
    }
}

pub fn fuzz_portable_document(data: &[u8]) {
    let Some((&selector, input)) = data.split_first() else {
        return;
    };
    let input = &input[..input.len().min(513)];
    let mut normalized = Vec::new();
    let input = if selector & 4 != 0 && input.len() >= 4 {
        normalized.extend_from_slice(input);
        normalized[..4].copy_from_slice(match selector & 3 {
            0 => b"AEVT",
            1 => b"AMNF",
            2 => b"ASTA",
            _ => b"AWIR",
        });
        normalized.as_slice()
    } else {
        input
    };

    match selector & 3 {
        0 => fuzz_event(input),
        1 => fuzz_manifest(input),
        2 => fuzz_state(input),
        _ => fuzz_widget_document(input),
    }
}

fn fuzz_event(input: &[u8]) {
    if let Ok(event) = CallbackEventView::decode(input, MODEL_LIMITS) {
        std::hint::black_box(event.generation_id());
        std::hint::black_box(event.event_sequence());
        std::hint::black_box(event.callback_id());
        std::hint::black_box(event.widget_key());
        std::hint::black_box(event.event_kind());
        std::hint::black_box(event.event_schema());
        std::hint::black_box(event.monotonic_timestamp());
        std::hint::black_box(event.payload());
    }
}

fn fuzz_manifest(input: &[u8]) {
    if let Ok(manifest) = ManifestView::decode(input, MODEL_LIMITS) {
        std::hint::black_box(manifest.minimum_abi());
        std::hint::black_box(manifest.maximum_abi());
        std::hint::black_box(manifest.widget_ir_version());
        std::hint::black_box(manifest.callback_event_version());
        std::hint::black_box(manifest.state_version());
        std::hint::black_box(manifest.program_id());
        for capability in manifest.capabilities() {
            std::hint::black_box(capability.capability_id());
            std::hint::black_box(capability.abi_major());
            std::hint::black_box(capability.policy());
            std::hint::black_box(capability.contract_fingerprint());
        }
    }
}

fn fuzz_state(input: &[u8]) {
    if let Ok(state) = StateBundleView::decode(input, MODEL_LIMITS) {
        std::hint::black_box(state.application_id());
        std::hint::black_box(state.source_generation());
        for entry in state.entries() {
            std::hint::black_box(entry.state_id());
            std::hint::black_box(entry.schema_id());
            std::hint::black_box(entry.schema_version());
            std::hint::black_box(entry.policy());
            std::hint::black_box(entry.payload());
        }
    }
}

fn fuzz_widget_document(input: &[u8]) {
    if let Ok(document) = WidgetDocumentView::decode(input, MODEL_LIMITS) {
        std::hint::black_box(document.generation_id());
        std::hint::black_box(document.document_revision());
        std::hint::black_box(document.root_node());
        for index in 0..document.node_count() {
            let node = document.node(index).expect("validated node index");
            std::hint::black_box(node.widget_type());
            std::hint::black_box(node.widget_schema());
            std::hint::black_box(node.key());
            for child in node.children() {
                std::hint::black_box(child);
            }
            for property in node.properties() {
                std::hint::black_box(property.property_id());
                std::hint::black_box(property.value());
            }
            for callback in node.callbacks() {
                std::hint::black_box(callback.callback_id());
                std::hint::black_box(callback.event_kind());
                std::hint::black_box(callback.event_schema());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aimer_anteros::{
        AbiVersion, ApplicationManifest, CallbackEvent, EventId, StableId128, StateBundle,
        StateEntry, StatePolicy, Version, WidgetDocument, WidgetNode, WidgetSchemaId,
        CALLBACK_EVENT_FORMAT_VERSION, STATE_FORMAT_VERSION, WIDGET_IR_FORMAT_VERSION,
    };

    #[test]
    fn envelope_harness_accepts_canonical_and_rejects_malformed_input() {
        let encoded = Envelope::new(*b"AAPP", Version::new(1, 0), 2, 0, 3, &[4])
            .encode()
            .unwrap();

        fuzz_envelope(&encoded);
        assert!(Envelope::decode(&encoded[..encoded.len() - 1], DECODE_LIMITS).is_err());
    }

    #[test]
    fn document_harness_bounds_raw_and_magic_normalized_inputs() {
        for selector in 0..8 {
            let mut input = vec![selector];
            input.extend_from_slice(&[0xFF; 1_024]);
            fuzz_portable_document(&input);
        }
    }

    #[test]
    fn document_harness_traverses_every_canonical_document_family() {
        let identity = StableId128::from_bytes([0x11; 16]);
        let version = Version::new(1, 0);
        let event = CallbackEvent::new(1, 2, identity, EventId::new(3), version, 4, &[5])
            .encode(MODEL_LIMITS)
            .unwrap();
        let manifest = ApplicationManifest::new(
            AbiVersion::new(1, 0),
            AbiVersion::new(1, 0),
            WIDGET_IR_FORMAT_VERSION,
            CALLBACK_EVENT_FORMAT_VERSION,
            STATE_FORMAT_VERSION,
            identity,
            &[],
        )
        .encode(MODEL_LIMITS)
        .unwrap();
        let entries = [StateEntry::new(
            identity,
            StableId128::from_bytes([0x22; 16]),
            version,
            StatePolicy::Required,
            &[6],
        )];
        let state = StateBundle::new(identity, 1, &entries)
            .encode(MODEL_LIMITS)
            .unwrap();
        let nodes = [WidgetNode::new(WidgetSchemaId::new(1), version)];
        let widget = WidgetDocument::new(1, 1, 0, &nodes, &[], &[])
            .encode(MODEL_LIMITS)
            .unwrap();

        fuzz_event(&event);
        fuzz_manifest(&manifest);
        fuzz_state(&state);
        fuzz_widget_document(&widget);
    }
}