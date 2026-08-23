//! A small, deterministic application surface for the hot-reload proof.
//!
//! Jaime's normal entry point is the window showcase, while its routing and
//! async request examples exercise separate screens. This fixture keeps the
//! proof focused: it is a public Jaime widget that uses the same provider,
//! state, collection, and callback APIs as those screens without depending on
//! a real network request or a platform window.

use aimer::{
    BuildContext, Button, Column, Provider, ProviderContext, State, StateUpdater, StatefulWidget,
    Text, Widget,
};
use aimer::provider::PortableProviderCodec;

#[cfg(feature = "portable-guest")]
use aimer::portable::{
    AimerReflectionType, DecodeError, Decoder, EncodeError, Encoder, FieldDescriptor, FieldKind,
    PortableApply, PortableEncode, StableId128, TypeSchema,
};

/// The bounded provider value rendered by the proof screen.
#[derive(Clone, Debug, PartialEq, aimer::PortableValue)]
#[portable_value(
    id = "aimer.value:jaime::phase29::ProofSettings",
    version = "1.0",
    max_encoded_bytes = 128,
)]
pub struct ProofSettings {
    /// Label supplied by the current guest generation.
    pub label: String,
    /// Small revision marker used to make the guest change visible.
    pub revision: u32,
}

/// Builds the stable initial version of the proof screen.
#[inline]
pub fn proof_root() -> impl Widget {
    proof_root_with_label("INITIAL")
}

/// Builds one deterministic proof screen with a generation-specific label.
///
/// The provider snapshot, keyed collection children, retained state, and both
/// callback kinds are intentionally kept in this one public boundary so the
/// generated guest fixture can exercise them together.
#[inline]
pub fn proof_root_with_label(label: &'static str) -> impl Widget {
    Provider::<ProofSettings>::new()
        .create(move || ProofSettings {
            label: label.to_owned(),
            revision: if label == "INITIAL" { 1 } else { 2 },
        })
        .portable_codec(PortableProviderCodec::from_portable_value())
        .child(PortableProofPanel::new())
}

#[derive(StatefulWidget)]
struct PortableProofPanel {
    key: Option<aimer::Key>,
}

impl PortableProofPanel {
    #[inline]
    fn new() -> Self {
        Self {
            key: Some("jaime-proof-panel".into()),
        }
    }
}

struct PortableProofPanelState {
    count: u32,
    async_count: u32,
    updater: StateUpdater<Self>,
}

#[cfg(feature = "portable-guest")]
const PORTABLE_PROOF_PANEL_STATE_FIELDS: &[FieldDescriptor] = &[
    FieldDescriptor::new("count", "u32", FieldKind::Retained),
    FieldDescriptor::new("async_count", "u32", FieldKind::Retained),
    FieldDescriptor::new(
        "updater",
        "StateUpdater<PortableProofPanelState>",
        FieldKind::Fresh,
    ),
];

#[cfg(feature = "portable-guest")]
const PORTABLE_PROOF_PANEL_STATE_SCHEMA: TypeSchema = TypeSchema::new(
    "PortableProofPanelState",
    StableId128::from_path(
        "aimer.type.v1",
        "jaime::hot_reload_proof::PortableProofPanelState",
    ),
    PORTABLE_PROOF_PANEL_STATE_FIELDS,
);

#[cfg(feature = "portable-guest")]
impl AimerReflectionType for PortableProofPanelState {
    const TYPE_ID: StableId128 = StableId128::from_path(
        "aimer.type.v1",
        "jaime::hot_reload_proof::PortableProofPanelState",
    );

    fn schema() -> &'static TypeSchema {
        &PORTABLE_PROOF_PANEL_STATE_SCHEMA
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncode for PortableProofPanelState {
    fn encode(&self, encoder: &mut Encoder<'_>) -> Result<(), EncodeError> {
        encoder.nested(|encoder| {
            encoder.field(&PORTABLE_PROOF_PANEL_STATE_FIELDS[0], |encoder| {
                self.count.encode(encoder)
            })?;
            encoder.field(&PORTABLE_PROOF_PANEL_STATE_FIELDS[1], |encoder| {
                self.async_count.encode(encoder)
            })?;
            encoder.field(&PORTABLE_PROOF_PANEL_STATE_FIELDS[2], |_| Ok(()))
        })
    }
}

#[cfg(feature = "portable-guest")]
impl PortableApply for PortableProofPanelState {
    type Retained = (u32, u32);

    fn decode_retained(decoder: &mut Decoder<'_>) -> Result<Self::Retained, DecodeError> {
        decoder.nested(|decoder| {
            let count = decoder
                .field(&PORTABLE_PROOF_PANEL_STATE_FIELDS[0])?
                .unwrap();
            let async_count = decoder
                .field(&PORTABLE_PROOF_PANEL_STATE_FIELDS[1])?
                .unwrap();
            let _ = decoder.field::<u8>(&PORTABLE_PROOF_PANEL_STATE_FIELDS[2])?;
            Ok((count, async_count))
        })
    }

    fn apply_retained(&mut self, retained: Self::Retained) {
        self.count = retained.0;
        self.async_count = retained.1;
    }
}

impl StatefulWidget for PortableProofPanel {
    type State = PortableProofPanelState;

    fn create_state(self) -> Self::State {
        PortableProofPanelState {
            count: 0,
            async_count: 0,
            updater: StateUpdater::new(),
        }
    }
}

impl State<PortableProofPanel> for PortableProofPanelState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let settings = ProviderContext::read::<ProofSettings>(ctx);
        let sync_updater = self.updater.clone();
        let async_updater = self.updater.clone();

        Column::new().children([
            Text::new("route: /").boxed(),
            Text::new(format!(
                "provider: {} @ {}",
                settings.label, settings.revision
            ))
            .boxed(),
            Button::new()
                .key("jaime-proof-sync")
                .on_press(move || sync_updater.set_state(|state| state.count += 1))
                .child(Text::new(format!("sync: {}", self.count)))
                .boxed(),
            Button::new()
                .key("jaime-proof-async")
                .on_press_async(move || async move {
                    async_updater.set_state(|state| state.async_count += 1);
                })
                .child(Text::new(format!("async: {}", self.async_count)))
                .boxed(),
        ])
    }
}
