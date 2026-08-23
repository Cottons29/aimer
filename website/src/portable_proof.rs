//! Website-owned proof of the guest-to-host portable hot-reload boundary.
//!
//! The fixture deliberately stays small and uses only widgets that the real
//! website uses in its pages. Keeping it in the website crate makes this a
//! compile-and-materialize proof for the application's dependency graph rather
//! than another framework-only codec test.

#[cfg(all(test, feature = "hot-reload-proof"))]
use std::sync::OnceLock;

#[cfg(all(test, feature = "hot-reload-proof"))]
use aimer::anteros::{
    EVENT_BUTTON_PRESS, ModelLimits, WIDGET_BUTTON, WIDGET_COLUMN, WIDGET_ROW,
    WIDGET_SIZED_BOX, WIDGET_TEXT, WidgetDocumentView,
};
#[cfg(all(test, feature = "hot-reload-proof"))]
use aimer::quiver::hot_reload::materialize_aimer_widget_tree;
#[cfg(all(test, feature = "hot-reload-proof"))]
use aimer::widget::portable::{
    PortableBuildContext, PortableLimits, PortableWidget, PortableWidgetLimits, SourceFingerprint,
    StableId128,
};
use aimer::provider::PortableProviderCodec;
use aimer::{
    BuildContext, Button, Column, Provider, ProviderContext, State, StateUpdater, StatefulWidget,
    Text, Widget,
};
#[cfg(all(test, feature = "hot-reload-proof"))]
use aimer::{Row, SizedBox, StatelessWidget, widget};

#[cfg(all(test, feature = "hot-reload-proof"))]
const GENERATION: u64 = 29;
#[cfg(all(test, feature = "hot-reload-proof"))]
const REVISION: u64 = 1;
#[cfg(all(test, feature = "hot-reload-proof"))]
const SOURCE: SourceFingerprint =
    SourceFingerprint::new(StableId128::from_bytes([0x29; 16]));

/// A compact website showcase used to prove the actual portable boundary.
///
/// The composition mirrors the site's page vocabulary: text, flex layout,
/// fixed spacing, and a button. The button intentionally uses the async press
/// API so the generated callback binding also crosses the same AWIR image.
#[cfg(all(test, feature = "hot-reload-proof"))]
#[widget(Stateless)]
pub(crate) struct WebsitePortableShowcase;

#[cfg(all(test, feature = "hot-reload-proof"))]
impl StatelessWidget for WebsitePortableShowcase {
    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Column::new().children(vec![
            Text::new("Aimer website").boxed(),
            Row::new()
                .children(vec![
                    SizedBox::new().width(240).height(48).boxed(),
                    Button::new()
                        .on_press_async(|| async {})
                        .child(Text::new("Reload"))
                        .boxed(),
                ])
                .boxed(),
        ])
    }
}

/// Provider value used by the website-owned two-generation proof.
#[derive(Clone, Debug, PartialEq, aimer::PortableValue)]
#[portable_value(
    id = "aimer.value:website::phase29::ProofSettings",
    version = "1.0",
    max_encoded_bytes = 128,
)]
pub struct ProofSettings {
    /// Label supplied by the current generated guest source.
    pub label: String,
    /// Small revision marker that makes the source change observable.
    pub revision: u32,
}

/// Builds the initial website-owned hot-reload proof root.
#[inline]
pub fn hot_reload_proof_root() -> impl Widget {
    hot_reload_proof_root_with_label("INITIAL")
}

/// Builds one deterministic website proof root for a generated source version.
///
/// The root intentionally uses the same provider, flex, text, and button
/// vocabulary as the website while keeping the generated fixture independent
/// of platform assets and network-backed screens. Its keyed state panel is the
/// public application boundary exercised by the two-generation test.
#[inline]
pub fn hot_reload_proof_root_with_label(label: &'static str) -> impl Widget {
    Provider::<ProofSettings>::new()
        .create(move || ProofSettings {
            label: label.to_owned(),
            revision: if label == "INITIAL" { 1 } else { 2 },
        })
        .portable_codec(PortableProviderCodec::from_portable_value())
        .child(WebsiteHotReloadProofPanel::new())
}

#[derive(StatefulWidget)]
struct WebsiteHotReloadProofPanel {
    key: Option<aimer::Key>,
}

impl WebsiteHotReloadProofPanel {
    #[inline]
    fn new() -> Self {
        Self {
            key: Some("website-phase29-proof-panel".into()),
        }
    }
}

struct WebsiteHotReloadProofPanelState {
    count: u32,
    async_count: u32,
    updater: StateUpdater<Self>,
}

#[cfg(feature = "portable-guest")]
const WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS: &[aimer::portable::FieldDescriptor] = &[
    aimer::portable::FieldDescriptor::new("count", "u32", aimer::portable::FieldKind::Retained),
    aimer::portable::FieldDescriptor::new(
        "async_count",
        "u32",
        aimer::portable::FieldKind::Retained,
    ),
    aimer::portable::FieldDescriptor::new(
        "updater",
        "StateUpdater<WebsiteHotReloadProofPanelState>",
        aimer::portable::FieldKind::Fresh,
    ),
];

#[cfg(feature = "portable-guest")]
const WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_SCHEMA: aimer::portable::TypeSchema =
    aimer::portable::TypeSchema::new(
        "WebsiteHotReloadProofPanelState",
        aimer::portable::StableId128::from_path(
            "aimer.type.v1",
            "website::portable_proof::WebsiteHotReloadProofPanelState",
        ),
        WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS,
    );

#[cfg(feature = "portable-guest")]
impl aimer::portable::AimerReflectionType for WebsiteHotReloadProofPanelState {
    const TYPE_ID: aimer::portable::StableId128 = aimer::portable::StableId128::from_path(
        "aimer.type.v1",
        "website::portable_proof::WebsiteHotReloadProofPanelState",
    );

    fn schema() -> &'static aimer::portable::TypeSchema {
        &WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_SCHEMA
    }
}

#[cfg(feature = "portable-guest")]
impl aimer::portable::PortableEncode for WebsiteHotReloadProofPanelState {
    fn encode(
        &self,
        encoder: &mut aimer::portable::Encoder<'_>,
    ) -> Result<(), aimer::portable::EncodeError> {
        encoder.nested(|encoder| {
            encoder.field(&WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS[0], |encoder| {
                self.count.encode(encoder)
            })?;
            encoder.field(&WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS[1], |encoder| {
                self.async_count.encode(encoder)
            })?;
            encoder.field(&WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS[2], |_| Ok(()))
        })
    }
}

#[cfg(feature = "portable-guest")]
impl aimer::portable::PortableApply for WebsiteHotReloadProofPanelState {
    type Retained = (u32, u32);

    fn decode_retained(
        decoder: &mut aimer::portable::Decoder<'_>,
    ) -> Result<Self::Retained, aimer::portable::DecodeError> {
        decoder.nested(|decoder| {
            let count = decoder
                .field(&WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS[0])?
                .unwrap();
            let async_count = decoder
                .field(&WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS[1])?
                .unwrap();
            let _ = decoder.field::<u8>(&WEBSITE_HOT_RELOAD_PROOF_PANEL_STATE_FIELDS[2])?;
            Ok((count, async_count))
        })
    }

    fn apply_retained(&mut self, retained: Self::Retained) {
        self.count = retained.0;
        self.async_count = retained.1;
    }
}

impl StatefulWidget for WebsiteHotReloadProofPanel {
    type State = WebsiteHotReloadProofPanelState;

    fn create_state(self) -> Self::State {
        WebsiteHotReloadProofPanelState {
            count: 0,
            async_count: 0,
            updater: StateUpdater::new(),
        }
    }
}

impl State<WebsiteHotReloadProofPanel> for WebsiteHotReloadProofPanelState {
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
                "website: {} @ {}",
                settings.label, settings.revision
            ))
            .boxed(),
            Button::new()
                .key("website-phase29-sync")
                .on_press(move || sync_updater.set_state(|state| state.count += 1))
                .child(Text::new(format!("sync: {}", self.count)))
                .boxed(),
            Button::new()
                .key("website-phase29-async")
                .on_press_async(move || async move {
                    async_updater.set_state(|state| state.async_count += 1);
                })
                .child(Text::new(format!("async: {}", self.async_count)))
                .boxed(),
        ])
    }
}

#[cfg(all(test, feature = "hot-reload-proof"))]
fn portable_image() -> (Vec<u8>, ModelLimits) {
    let widget_limits = PortableWidgetLimits::new(32, 64, 64, 64, 4_096, 32_768)
        .with_max_blob_bytes(4_096);
    let state_limits = PortableLimits::new(16, 128, 4_096, 4_096, 32_768);
    let mut context = PortableBuildContext::new(GENERATION, REVISION, widget_limits, state_limits)
        .expect("website portable proof limits are valid");
    let root = WebsitePortableShowcase
        .to_portable_node(&mut context, SOURCE)
        .expect("website showcase must lower to portable Widget IR");
    let document = context
        .finish_document(root)
        .expect("website showcase must produce a complete portable document");
    let limits = document.model_limits();
    let image = document
        .encode()
        .expect("website showcase document must encode canonically");
    (image, limits)
}

#[cfg(all(test, feature = "hot-reload-proof"))]
fn host_context() -> BuildContext<'static> {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("website portable proof runtime must initialize")
    });
    let _entered = runtime.enter();
    let canvas = {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        aimer_canvas::Canvas::new(inner)
    };
    BuildContext::new(
        canvas,
        Default::default(),
        1.0,
        Default::default(),
        Default::default(),
        aimer_widget::base::WindowHandle::headless(Default::default(), 1.0),
        runtime.handle().clone(),
    )
}

#[cfg(all(test, feature = "hot-reload-proof"))]
mod tests {
    use super::*;

    #[test]
    fn website_showcase_crosses_guest_awir_and_native_materializer() {
        let (image, limits) = portable_image();
        let view = WidgetDocumentView::decode(&image, limits)
            .expect("host must decode the guest-produced website image");

        assert_eq!(view.generation_id(), GENERATION);
        assert_eq!(view.document_revision(), REVISION);
        assert_eq!(view.root_node(), 5);
        assert_eq!(view.node_count(), 6);
        assert_eq!(view.node(0).unwrap().widget_type(), WIDGET_TEXT);
        assert_eq!(view.node(1).unwrap().widget_type(), WIDGET_SIZED_BOX);
        assert_eq!(view.node(2).unwrap().widget_type(), WIDGET_TEXT);
        assert_eq!(view.node(3).unwrap().widget_type(), WIDGET_BUTTON);
        assert_eq!(view.node(4).unwrap().widget_type(), WIDGET_ROW);
        assert_eq!(view.node(5).unwrap().widget_type(), WIDGET_COLUMN);

        let button = view.node(3).unwrap();
        let press = button
            .callbacks()
            .find(|callback| callback.event_kind() == EVENT_BUTTON_PRESS)
            .expect("website async button must publish its press callback");
        assert!(press.is_async());
        assert_eq!(button.children().collect::<Vec<_>>(), vec![2]);
        assert_eq!(view.node(4).unwrap().children().collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(view.node(5).unwrap().children().collect::<Vec<_>>(), vec![0, 4]);

        let _native_root = materialize_aimer_widget_tree(&image, limits, &host_context(), |_| {})
            .expect("the website showcase must have a native host materializer");
    }
}
