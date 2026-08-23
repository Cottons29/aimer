use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aimer_cli::hot_reload::build::{HotReloadBuildPlan, load_guest_artifact};
use aimer_cli::hot_reload::generation::{GeneratedGuestPackage, prepare_automatic_guest};
use tempfile::TempDir;

const PACKAGE: &str = "jaime-phase29-hot-reload-app";

#[test]
fn jaime_application_guest_proof_covers_change_state_callbacks_recovery_and_cleanup() {
    let fixture = fixture_root();
    let original = snapshot(&fixture);
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("application");
    copy_tree(&fixture, &project);
    let session = project.join("target/aimer-hot-reload");
    let cleanup = SessionCleanup::new(session.clone());
    let state_path = temp.path().join("phase29-state.asta");
    let callback_ids_path = temp.path().join("phase29-callback-ids.bin");

    let initial = generate(&project);
    inject_generated_test(&initial);
    run_generated_test(
        &initial,
        &temp,
        &state_path,
        &callback_ids_path,
        "initial",
    );

    apply_variant(&project, "changed.rs");
    let changed = generate(&project);
    inject_generated_test(&changed);
    run_generated_test(
        &changed,
        &temp,
        &state_path,
        &callback_ids_path,
        "changed",
    );
    build_and_validate_wasm(&project, &changed);

    drop(cleanup);
    assert!(!session.exists(), "hot-reload staging output was not cleaned up");
    assert_eq!(snapshot(&fixture), original, "fixture source was modified");
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phase29_hot_reload_app")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

fn generate(project: &Path) -> GeneratedGuestPackage {
    let workspace = workspace_root();
    let project_lock = project.join("Cargo.lock");
    if !project_lock.is_file() {
        fs::copy(workspace.join("Cargo.lock"), &project_lock).unwrap();
    }
    prepare_automatic_guest(
        project,
        &workspace,
        &project.join("target/aimer-hot-reload"),
    )
    .unwrap()
}

fn inject_generated_test(generated: &GeneratedGuestPackage) {
    let tests = generated.application_root().join("tests");
    fs::create_dir_all(&tests).unwrap();
    fs::write(tests.join("phase29_generated.rs"), GENERATED_TEST_SOURCE).unwrap();
}

fn run_generated_test(
    generated: &GeneratedGuestPackage,
    temp: &TempDir,
    state_path: &Path,
    callback_ids_path: &Path,
    stage: &str,
) {
    let status = Command::new(env!("CARGO"))
        .args(["test", "--quiet", "--manifest-path"])
        .arg(generated.application_root().join("Cargo.toml"))
        .args(["--test", "phase29_generated", "--", "--test-threads=1"])
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_TARGET_DIR", temp.path().join("cargo-target"))
        .env("JAIME_PHASE29_STAGE", stage)
        .env("JAIME_PHASE29_STATE_PATH", state_path)
        .env("JAIME_PHASE29_CALLBACK_IDS_PATH", callback_ids_path)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "{stage} generated proof failed with status {status}"
    );
}

fn build_and_validate_wasm(project: &Path, generated: &GeneratedGuestPackage) {
    let target = project.join("target/aimer-hot-reload/guest");
    let plan = HotReloadBuildPlan::new(
        project.join("Cargo.toml"),
        generated.package(),
        PACKAGE,
        target.clone(),
    )
    .guest_manifest(generated.manifest().to_owned())
    .guest_source_remap(generated.application_root().to_owned(), project.to_owned());
    let command = plan.guest_command();
    let output = Command::new(command.program())
        .args(command.arguments())
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Jaime generated wasm build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let artifact = target.join("wasm32-unknown-unknown/debug/aimer_generated_guest.wasm");
    let module = load_guest_artifact(&artifact, plan.guest_target_dir(), 256 * 1024 * 1024)
        .unwrap();
    assert!(!module.is_empty());
}

fn apply_variant(project: &Path, variant: &str) {
    fs::copy(project.join("variants").join(variant), project.join("src/main.rs")).unwrap();
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
    if source == fixture_root() {
        let manifest_path = destination.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        let workspace = workspace_root();
        let jaime = workspace.join("jaime");
        fs::write(
            manifest_path,
            manifest
                .replace("../../../..", &workspace.to_string_lossy())
                .replace("../../..", &jaime.to_string_lossy()),
        )
        .unwrap();
    }
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn collect(root: &Path, current: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(current).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                collect(root, &entry.path(), files);
            } else {
                files.push((
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                ));
            }
        }
    }

    let mut files = Vec::new();
    collect(root, root, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

struct SessionCleanup {
    path: PathBuf,
}

impl SessionCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for SessionCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const GENERATED_TEST_SOURCE: &str = r###"
use std::fs;
use std::path::PathBuf;

use aimer::anteros::{
    AbiStatus, CallbackBinding, CallbackEvent, ModelLimits, PropertyValue, StateBundleView,
    WidgetDocumentView, EVENT_BUTTON_PRESS, PROPERTY_TEXT_CONTENT,
};
use aimer_wasm_guest::GuestAdapter;
use jaime_phase29_hot_reload_app::{
    __AIMER_GENERATED_GUEST_LIMITS, __AimerGeneratedGuestProgram,
};

const ACTIVE_GENERATION: u64 = 11;
const CANDIDATE_GENERATION: u64 = 12;

fn limits() -> ModelLimits {
    ModelLimits::new(16_777_216, 65_536, 1_048_576, 16_777_216)
}

#[test]
fn generated_jaime_guest_proves_the_public_boundary() {
    if std::env::var("JAIME_PHASE29_STAGE").as_deref() == Ok("initial") {
        prove_initial_generation();
    } else {
        prove_changed_generation_and_recovery();
    }
}

fn prove_initial_generation() {
    let mut active = adapter(ACTIVE_GENERATION);
    active.manifest().unwrap();
    let initial_bytes = active.build().unwrap();
    let initial = decode(&initial_bytes);
    assert_eq!(initial.generation_id(), ACTIVE_GENERATION);
    assert_contains(&initial, "route: /");
    assert_contains(&initial, "provider: INITIAL @ 1");
    assert_contains(&initial, "sync: 0");
    assert_contains(&initial, "async: 0");

    let bindings = press_bindings(&initial);
    assert_eq!(bindings.len(), 2, "expected one sync and one async proof button");
    let sync = bindings.iter().find(|binding| !binding.is_async()).unwrap();
    let asynchronous = bindings.iter().find(|binding| binding.is_async()).unwrap();
    let mut callback_ids = Vec::with_capacity(32);
    callback_ids.extend_from_slice(sync.callback_id().as_bytes());
    callback_ids.extend_from_slice(asynchronous.callback_id().as_bytes());
    fs::write(callback_ids_path(), callback_ids).unwrap();

    let sync_event = event(ACTIVE_GENERATION, &initial, *sync);
    let sync_output = active.dispatch_event(&sync_event).unwrap().unwrap();
    let after_sync = decode(&sync_output);
    assert_contains(&after_sync, "sync: 1");

    let async_event = event(ACTIVE_GENERATION, &after_sync, *asynchronous);
    assert!(active.dispatch_event(&async_event).unwrap().is_none());
    assert!(active.has_async_work());
    let async_output = active.poll_async().unwrap().unwrap();
    let after_async = decode(&async_output);
    assert_contains(&after_async, "async: 1");

    let state = active.export_state().unwrap();
    let state_view = StateBundleView::decode(&state, limits()).unwrap();
    assert_eq!(state_view.source_generation(), ACTIVE_GENERATION);
    assert_eq!(state_view.entry_count(), 1);
    fs::write(state_path(), state).unwrap();
}

fn prove_changed_generation_and_recovery() {
    let mut candidate = adapter(CANDIDATE_GENERATION);
    candidate.manifest().unwrap();
    let initial_bytes = candidate.build().unwrap();
    let initial = decode(&initial_bytes);
    assert_eq!(initial.generation_id(), CANDIDATE_GENERATION);
    assert_contains(&initial, "provider: UPDATED @ 2");
    assert_contains(&initial, "sync: 0");
    assert_contains(&initial, "async: 0");

    let bindings = press_bindings(&initial);
    let sync = bindings.iter().find(|binding| !binding.is_async()).unwrap();
    let asynchronous = bindings.iter().find(|binding| binding.is_async()).unwrap();
    let expected_ids = fs::read(callback_ids_path()).unwrap();
    assert_eq!(&expected_ids[..16], sync.callback_id().as_bytes());
    assert_eq!(&expected_ids[16..], asynchronous.callback_id().as_bytes());

    let malformed = candidate.import_state(&[0xff]);
    assert_eq!(malformed.unwrap_err().status(), AbiStatus::MalformedMessage);
    let recovered_bytes = candidate.build().unwrap();
    let recovered = decode(&recovered_bytes);
    assert_contains(&recovered, "provider: UPDATED @ 2");
    assert_contains(&recovered, "sync: 0");

    let stale = event(ACTIVE_GENERATION, &recovered, *sync);
    assert_eq!(
        candidate.dispatch_event(&stale).unwrap_err().status(),
        AbiStatus::RetiredGeneration,
    );

    let state = fs::read(state_path()).unwrap();
    candidate.import_state(&state).unwrap();
    let restored_bytes = candidate.build().unwrap();
    let restored = decode(&restored_bytes);
    assert_contains(&restored, "provider: UPDATED @ 2");
    assert_contains(&restored, "sync: 1");
    assert_contains(&restored, "async: 1");
}

fn adapter(generation: u64) -> GuestAdapter<__AimerGeneratedGuestProgram> {
    let mut adapter = GuestAdapter::new(
        __AimerGeneratedGuestProgram::default(),
        __AIMER_GENERATED_GUEST_LIMITS,
    )
    .unwrap();
    adapter.initialize(generation).unwrap();
    adapter
}

fn decode(bytes: &[u8]) -> WidgetDocumentView<'_> {
    WidgetDocumentView::decode(bytes, limits()).unwrap()
}

fn press_bindings(document: &WidgetDocumentView<'_>) -> Vec<CallbackBinding> {
    let mut bindings = Vec::new();
    for index in 0..document.node_count() {
        for binding in document.node(index).unwrap().callbacks() {
            if binding.event_kind() == EVENT_BUTTON_PRESS {
                bindings.push(binding);
            }
        }
    }
    bindings
}

fn event(generation: u64, document: &WidgetDocumentView<'_>, binding: CallbackBinding) -> Vec<u8> {
    CallbackEvent::new(
        generation,
        document.document_revision(),
        binding.callback_id(),
        binding.event_kind(),
        binding.event_schema(),
        0,
        &[],
    )
    .encode(limits())
    .unwrap()
}

fn assert_contains(document: &WidgetDocumentView<'_>, expected: &str) {
    let labels = text_labels(document);
    assert!(
        labels.iter().any(|label| label == expected),
        "missing {expected:?} in text labels {labels:?}"
    );
}

fn text_labels(document: &WidgetDocumentView<'_>) -> Vec<String> {
    (0..document.node_count())
        .filter_map(|index| {
            document.node(index).unwrap().properties().find_map(|property| {
                if property.property_id() != PROPERTY_TEXT_CONTENT {
                    return None;
                }
                match property.value() {
                    PropertyValue::StringRef(index) => Some(document.string(index).unwrap().to_owned()),
                    _ => None,
                }
            })
        })
        .collect()
}

fn state_path() -> PathBuf {
    PathBuf::from(std::env::var("JAIME_PHASE29_STATE_PATH").unwrap())
}

fn callback_ids_path() -> PathBuf {
    PathBuf::from(std::env::var("JAIME_PHASE29_CALLBACK_IDS_PATH").unwrap())
}
"###;
