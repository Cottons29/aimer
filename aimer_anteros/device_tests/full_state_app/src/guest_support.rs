use std::marker::PhantomData;

use aimer_wasm_guest::anteros::{
    AbiStatus, AbiVersion, ApplicationManifest, CallbackBinding, CallbackEventView, ModelLimits,
    PropertyValue, StableId128, StateBundle, StateBundleView, StateEntry, StatePolicy, Version,
    WidgetDocument, WidgetNode, WidgetProperty, EVENT_BUTTON_PRESS, PROPERTY_CONTAINER_COLOR,
    PROPERTY_CONTAINER_HEIGHT, PROPERTY_CONTAINER_WIDTH, PROPERTY_TEXT_CONTENT, WIDGET_BUTTON,
    WIDGET_COLUMN, WIDGET_CONTAINER, WIDGET_TEXT, CALLBACK_EVENT_FORMAT_VERSION,
    STATE_FORMAT_VERSION, WIDGET_IR_FORMAT_VERSION,
};
use aimer_wasm_guest::{CallbackRegistry, GuestError, GuestLimits, GuestProgram};

/// Portable document ceilings shared by every deterministic source variant.
pub const MODEL_LIMITS: ModelLimits =
    ModelLimits::new(4_096, 32, 128, 128).max_widget_depth(16);

/// Guest allocation ceilings referenced explicitly by `aimer.toml`.
pub const HOT_RELOAD_LIMITS: GuestLimits = GuestLimits::new(MODEL_LIMITS, 4, 16_384, 16);

const PROGRAM_ID: StableId128 = StableId128::from_bytes([0x11; 16]);
/// Stable callback identity retained while its implementation is rebound.
pub const CALLBACK_ID: StableId128 = StableId128::from_bytes([0x22; 16]);
const STATE_ID: StableId128 = StableId128::from_bytes([0x33; 16]);
const SCHEMA_ID: StableId128 = StableId128::from_bytes([0x44; 16]);
const ABI_VERSION: Version = Version::new(1, 0);

pub trait GuestVariant: Send + 'static {
    const SCHEMA_VERSION: Version;
    const STATE_TAG: u8;
    const CALLBACK_STEP: u8;
    const HEADER: &'static str;
    const BUTTON: &'static str;
    const TRAP_ON_BUILD: bool;

    fn migrate(previous: Version, payload: &[u8]) -> Option<Vec<u8>>;
}

/// Stateful guest implementation parameterized by one checked-in proof variant.
pub struct FullStateProgram<V> {
    counter: u8,
    callbacks: CallbackRegistry<u8>,
    generation_id: u64,
    marker: PhantomData<V>,
}

impl<V: GuestVariant> Default for FullStateProgram<V> {
    fn default() -> Self {
        let mut callbacks = CallbackRegistry::<u8>::new().max_callbacks(1);
        callbacks
            .register(CALLBACK_ID, increment::<V>)
            .expect("the static callback registration is valid");
        Self {
            counter: 0,
            callbacks,
            generation_id: 0,
            marker: PhantomData,
        }
    }
}

impl<V: GuestVariant> GuestProgram for FullStateProgram<V> {
    fn manifest(&self, limits: ModelLimits) -> Result<Vec<u8>, GuestError> {
        ApplicationManifest::new(
            AbiVersion::new(1, 0),
            AbiVersion::new(1, 0),
            WIDGET_IR_FORMAT_VERSION,
            CALLBACK_EVENT_FORMAT_VERSION,
            STATE_FORMAT_VERSION,
            PROGRAM_ID,
            &[],
        )
        .encode(limits)
        .map_err(GuestError::from_model)
    }

    fn initialize(&mut self, generation_id: u64) -> Result<(), GuestError> {
        self.generation_id = generation_id;
        Ok(())
    }

    fn build(&mut self, limits: ModelLimits) -> Result<Vec<u8>, GuestError> {
        assert!(!V::TRAP_ON_BUILD, "intentional full-state initial-build trap");
        widget_image::<V>(self.generation_id, self.counter, limits)
    }

    fn dispatch_event(
        &mut self,
        event: &CallbackEventView<'_>,
        limits: ModelLimits,
    ) -> Result<Option<Vec<u8>>, GuestError> {
        self.callbacks.dispatch(&mut self.counter, event)?;
        widget_image::<V>(self.generation_id, self.counter, limits).map(Some)
    }

    fn export_state(&self, limits: ModelLimits) -> Result<Vec<u8>, GuestError> {
        let payload = [self.counter, V::STATE_TAG];
        let payload = if V::SCHEMA_VERSION == ABI_VERSION {
            &payload[..1]
        } else {
            &payload[..]
        };
        let entries = [StateEntry::new(
            STATE_ID,
            SCHEMA_ID,
            V::SCHEMA_VERSION,
            StatePolicy::Required,
            payload,
        )];
        StateBundle::new(PROGRAM_ID, self.generation_id, &entries)
            .encode(limits)
            .map_err(GuestError::from_model)
    }

    fn import_state(&mut self, state: &StateBundleView<'_>) -> Result<(), GuestError> {
        let entry = compatible_entry::<V>(state)?;
        self.counter = entry.payload()[0];
        Ok(())
    }

    fn migrate_state(
        &mut self,
        state: &StateBundleView<'_>,
        limits: ModelLimits,
    ) -> Result<Vec<u8>, GuestError> {
        if state.application_id() != PROGRAM_ID || state.entry_count() != 1 {
            return Err(incompatible());
        }
        let entry = state.entry(0).unwrap();
        if entry.state_id() != STATE_ID || entry.schema_id() != SCHEMA_ID {
            return Err(incompatible());
        }
        let payload = V::migrate(entry.schema_version(), entry.payload()).ok_or_else(incompatible)?;
        let entries = [StateEntry::new(
            STATE_ID,
            SCHEMA_ID,
            V::SCHEMA_VERSION,
            StatePolicy::Required,
            &payload,
        )];
        StateBundle::new(PROGRAM_ID, self.generation_id, &entries)
            .encode(limits)
            .map_err(GuestError::from_model)
    }
}

pub fn reject_migration(_previous: Version, _payload: &[u8]) -> Option<Vec<u8>> {
    None
}

pub fn migrate_v1_to_v2(previous: Version, payload: &[u8], tag: u8) -> Option<Vec<u8>> {
    (previous == ABI_VERSION && payload.len() == 1).then(|| vec![payload[0], tag])
}

fn increment<V: GuestVariant>(counter: &mut u8, _event: &CallbackEventView<'_>) -> Result<bool, GuestError> {
    *counter = counter.saturating_add(V::CALLBACK_STEP);
    Ok(true)
}

fn compatible_entry<'a, V: GuestVariant>(
    state: &'a StateBundleView<'a>,
) -> Result<aimer_wasm_guest::anteros::StateEntryView<'a>, GuestError> {
    if state.application_id() != PROGRAM_ID || state.entry_count() != 1 {
        return Err(incompatible());
    }
    let entry = state.entry(0).unwrap();
    let expected_length = if V::SCHEMA_VERSION == ABI_VERSION { 1 } else { 2 };
    if entry.state_id() != STATE_ID
        || entry.schema_id() != SCHEMA_ID
        || entry.schema_version() != V::SCHEMA_VERSION
        || entry.payload().len() != expected_length
        || (expected_length == 2 && entry.payload()[1] != V::STATE_TAG)
    {
        return Err(incompatible());
    }
    Ok(entry)
}

fn incompatible() -> GuestError {
    GuestError::new(AbiStatus::StateIncompatible)
}

fn widget_image<V: GuestVariant>(
    generation_id: u64,
    counter: u8,
    limits: ModelLimits,
) -> Result<Vec<u8>, GuestError> {
    let root_children = [1_u32];
    let surface_children = [2_u32];
    let column_children = [3_u32, 4, 5];
    let button_children = [6_u32];
    let counter_label = format!("counter: {counter}");
    let text_property = |index| {
        [WidgetProperty::new(
            PROPERTY_TEXT_CONTENT,
            PropertyValue::StringRef(index),
        )]
    };
    let header_properties = text_property(0);
    let counter_properties = text_property(1);
    let button_properties = text_property(2);
    let shade = 0x50_u32.saturating_add(u32::from(counter).saturating_mul(0x20)).min(0xE0);
    let counter_color = (shade << 24) | (shade << 16) | (shade << 8) | 0xFF;
    let root_properties = [
        WidgetProperty::new(PROPERTY_CONTAINER_WIDTH, PropertyValue::F64(900.0)),
        WidgetProperty::new(PROPERTY_CONTAINER_HEIGHT, PropertyValue::F64(600.0)),
        WidgetProperty::new(PROPERTY_CONTAINER_COLOR, PropertyValue::Rgba(counter_color)),
    ];
    let container_properties = [
        WidgetProperty::new(PROPERTY_CONTAINER_WIDTH, PropertyValue::F64(180.0)),
        WidgetProperty::new(PROPERTY_CONTAINER_HEIGHT, PropertyValue::F64(48.0)),
    ];
    let callbacks = [CallbackBinding::new(EVENT_BUTTON_PRESS, ABI_VERSION, CALLBACK_ID)];
    let nodes = [
        WidgetNode::new(WIDGET_BUTTON, ABI_VERSION)
            .callbacks(&callbacks)
            .children(&root_children),
        WidgetNode::new(WIDGET_CONTAINER, ABI_VERSION)
            .properties(&root_properties)
            .children(&surface_children),
        WidgetNode::new(WIDGET_COLUMN, ABI_VERSION).children(&column_children),
        WidgetNode::new(WIDGET_TEXT, ABI_VERSION).properties(&header_properties),
        WidgetNode::new(WIDGET_TEXT, ABI_VERSION).properties(&counter_properties),
        WidgetNode::new(WIDGET_CONTAINER, ABI_VERSION)
            .properties(&container_properties)
            .children(&button_children),
        WidgetNode::new(WIDGET_TEXT, ABI_VERSION).properties(&button_properties),
    ];
    let strings = [V::HEADER, counter_label.as_str(), V::BUTTON];
    WidgetDocument::new(
        generation_id,
        u64::from(counter),
        0,
        &nodes,
        &strings,
        &[],
    )
    .encode(limits)
    .map_err(GuestError::from_model)
}
