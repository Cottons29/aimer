//! Deterministic preparation and analysis of standalone hot-reload projects.

mod discovery;
mod fingerprint;
mod guest;
mod manifest;
mod mirror;
mod transform;

use std::fmt;
use std::path::{Path, PathBuf};

pub use discovery::{RootDiscovery, SourceSpan, SourceType, StableIdentity};
pub use fingerprint::{AstFingerprint, fingerprint_expression, fingerprint_source};

/// Canonical local crates linked into a generated portable guest shadow.
///
/// The caller supplies both paths so shadow generation never assumes a
/// repository layout. Each path must name a local Cargo package directory;
/// preparation canonicalizes it and rejects missing or non-directory values.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShadowGuestConfig {
    aimer_root: Option<PathBuf>,
    wasm_guest_root: Option<PathBuf>,
    portable_webbrowser_root: Option<PathBuf>,
    portable_reqwest_root: Option<PathBuf>,
}

impl ShadowGuestConfig {
    /// Creates an empty configuration completed through builder methods.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the local root of the umbrella `aimer` crate.
    #[inline]
    pub fn aimer_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.aimer_root = Some(root.into());
        self
    }

    /// Sets the local root of the `aimer_wasm_guest` crate.
    #[inline]
    pub fn wasm_guest_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.wasm_guest_root = Some(root.into());
        self
    }

    /// Sets the local guest-only replacement for the browser-opening crate.
    ///
    /// Portable guests cannot import browser globals directly. When an
    /// application depends on `webbrowser`, shadow generation can replace it
    /// with this no-browser implementation while native builds retain the
    /// application's original dependency.
    #[inline]
    pub fn portable_webbrowser_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.portable_webbrowser_root = Some(root.into());
        self
    }

    /// Sets the guest-only replacement for the network client crate.
    ///
    /// Native applications keep their ordinary `reqwest` dependency. A
    /// portable hot-reload guest instead routes its bounded GET surface
    /// through Aimer's single capability-call import, so wasm-bindgen browser
    /// imports cannot leak into the interpreted module.
    #[inline]
    pub fn portable_reqwest_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.portable_reqwest_root = Some(root.into());
        self
    }

    fn canonicalize(&self) -> Result<CanonicalGuestConfig, ShadowError> {
        Ok(CanonicalGuestConfig {
            aimer_root: canonical_crate_root(self.aimer_root.as_deref(), "aimer")?,
            wasm_guest_root: canonical_crate_root(
                self.wasm_guest_root.as_deref(),
                "aimer_wasm_guest",
            )?,
            portable_webbrowser_root: self
                .portable_webbrowser_root
                .as_deref()
                .map(|root| canonical_crate_root(Some(root), "aimer_portable_webbrowser"))
                .transpose()?,
            portable_reqwest_root: self
                .portable_reqwest_root
                .as_deref()
                .map(|root| canonical_crate_root(Some(root), "aimer_portable_reqwest"))
                .transpose()?,
        })
    }
}

pub(crate) struct CanonicalGuestConfig {
    pub aimer_root: PathBuf,
    pub wasm_guest_root: PathBuf,
    pub portable_webbrowser_root: Option<PathBuf>,
    pub portable_reqwest_root: Option<PathBuf>,
}

fn canonical_crate_root(root: Option<&Path>, name: &str) -> Result<PathBuf, ShadowError> {
    let root = root.ok_or_else(|| {
        ShadowError::new(
            ShadowErrorKind::Manifest,
            format!("local {name} crate root is required for guest generation"),
        )
    })?;
    let canonical = std::fs::canonicalize(root).map_err(|error| {
        ShadowError::new(
            ShadowErrorKind::Io,
            format!("failed to resolve local {name} crate {}: {error}", root.display()),
        )
    })?;
    if !canonical.is_dir() || !canonical.join("Cargo.toml").is_file() {
        return Err(ShadowError::new(
            ShadowErrorKind::Manifest,
            format!("local {name} crate root is not a Cargo package: {}", canonical.display()),
        ));
    }
    Ok(canonical)
}

/// Resource limits applied while preparing and analysing a shadow project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShadowLimits {
    /// Maximum number of regular files copied into the shadow.
    pub max_files: usize,
    /// Maximum total number of bytes copied into the shadow.
    pub max_total_bytes: u64,
    /// Maximum number of bytes accepted for one file.
    pub max_file_bytes: u64,
    /// Maximum number of direct local calls followed from `#[aimer::main]`.
    pub max_ast_call_depth: usize,
}

impl Default for ShadowLimits {
    fn default() -> Self {
        Self {
            max_files: 16_384,
            max_total_bytes: 512 * 1024 * 1024,
            max_file_bytes: 16 * 1024 * 1024,
            max_ast_call_depth: 64,
        }
    }
}

impl ShadowLimits {
    /// Sets the maximum number of mirrored files.
    #[inline]
    pub const fn max_files(mut self, max_files: usize) -> Self {
        self.max_files = max_files;
        self
    }

    /// Sets the maximum total mirrored byte count.
    #[inline]
    pub const fn max_total_bytes(mut self, max_total_bytes: u64) -> Self {
        self.max_total_bytes = max_total_bytes;
        self
    }

    /// Sets the maximum byte count accepted for one file.
    #[inline]
    pub const fn max_file_bytes(mut self, max_file_bytes: u64) -> Self {
        self.max_file_bytes = max_file_bytes;
        self
    }

    /// Sets the maximum direct-call analysis depth.
    #[inline]
    pub const fn max_ast_call_depth(mut self, max_ast_call_depth: usize) -> Self {
        self.max_ast_call_depth = max_ast_call_depth;
        self
    }
}

/// A fully prepared standalone application shadow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowProject {
    root: PathBuf,
    manifest: PathBuf,
    discovery: RootDiscovery,
    source_types: Vec<SourceType>,
}

impl ShadowProject {
    /// Returns the canonical output root containing the shadow project.
    #[inline]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the rewritten standalone Cargo manifest.
    #[inline]
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    /// Returns the unique application root discovered from `#[aimer::main]`.
    #[inline]
    pub fn discovery(&self) -> &RootDiscovery {
        &self.discovery
    }

    /// Returns source structs in stable package/module/type order.
    #[inline]
    pub fn source_types(&self) -> &[SourceType] {
        &self.source_types
    }
}

/// Stable category for a shadow preparation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShadowErrorKind {
    /// A filesystem operation failed.
    Io,
    /// A path or symbolic link escaped the accepted roots.
    PathEscape,
    /// The output overlaps the source project.
    OutputRecursion,
    /// A configured resource bound was exceeded.
    LimitExceeded,
    /// A Cargo manifest was missing or malformed.
    Manifest,
    /// Rust source could not be parsed.
    MalformedSource,
    /// Two module declarations own the same source or one module has two files.
    DuplicateModule,
    /// The application entry point or root flow is ambiguous.
    AmbiguousFlow,
    /// A direct local call could not be resolved statically.
    UnresolvedFlow,
    /// The root flow uses dynamic dispatch or another non-static call.
    DynamicFlow,
    /// No complete application builder chain was found.
    MissingFlow,
}

/// Failure to prepare or statically analyse a shadow project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowError {
    kind: ShadowErrorKind,
    message: String,
    span: Option<SourceSpan>,
}

impl ShadowError {
    pub(crate) fn new(kind: ShadowErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), span: None }
    }

    pub(crate) fn at(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Returns the stable error category.
    #[inline]
    pub const fn kind(&self) -> ShadowErrorKind {
        self.kind
    }

    /// Returns the source location associated with the failure, when available.
    #[inline]
    pub fn span(&self) -> Option<&SourceSpan> {
        self.span.as_ref()
    }

    /// Returns the human-readable diagnostic message without source context.
    #[inline]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ShadowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ShadowError {}

/// Mirrors, rewrites, and analyses one application as a standalone project.
///
/// `project_root` must contain `Cargo.toml`. `output_root` must not overlap the
/// source tree and is replaced deterministically after all source paths and
/// limits have been validated. Symbolic links are rejected rather than
/// followed. Relative dependency paths are canonicalized before the source is
/// copied, and workspace-inherited dependencies are materialized in the
/// output manifest.
///
/// After discovery, eligible source-defined structs in the copied project gain
/// hidden reflection and portable-state implementations. The application files
/// under `project_root` are never rewritten, and preparing the same inputs
/// repeatedly produces identical shadow bytes.
pub fn prepare_shadow_project(
    project_root: &Path,
    output_root: &Path,
    limits: ShadowLimits,
) -> Result<ShadowProject, ShadowError> {
    let rewritten = manifest::rewrite(project_root)?;
    let mirror = mirror::validate_and_copy(project_root, output_root, limits, &rewritten.bytes)?;
    let manifest = mirror.output_root.join("Cargo.toml");
    let (discovery, source_types) = discovery::discover(
        &mirror.source_root,
        &rewritten.package,
        &rewritten.value,
        limits.max_ast_call_depth,
    )?;
    transform::transform(&mirror.output_root, &rewritten.package, &rewritten.value)?;
    Ok(ShadowProject {
        root: mirror.output_root,
        manifest,
        discovery,
        source_types,
    })
}

/// Prepares a standalone, dependency-linkable portable guest shadow.
///
/// This performs ordinary bounded shadow preparation, then rewrites only the
/// copied package as a `cdylib`/`rlib`, links the caller-supplied local guest
/// crates, enables `aimer/portable-guest`, suppresses binary auto-discovery,
/// and emits the generated root factory and [`aimer_wasm_guest::GuestProgram`]
/// adapter. The source project remains byte-for-byte unchanged.
pub fn prepare_guest_shadow_project(
    project_root: &Path,
    output_root: &Path,
    limits: ShadowLimits,
    guest_config: ShadowGuestConfig,
) -> Result<ShadowProject, ShadowError> {
    let guest_config = guest_config.canonicalize()?;
    let source_root = std::fs::canonicalize(project_root).map_err(|error| {
        ShadowError::new(
            ShadowErrorKind::Io,
            format!("failed to resolve source project {}: {error}", project_root.display()),
        )
    })?;
    let mut rewritten = manifest::rewrite(&source_root)?;
    let (discovery, source_types) = discovery::discover(
        &source_root,
        &rewritten.package,
        &rewritten.value,
        limits.max_ast_call_depth,
    )?;
    manifest::enable_guest(
        &mut rewritten,
        &source_root,
        &discovery,
        &guest_config,
    )?;
    let mirror = mirror::validate_and_copy(&source_root, output_root, limits, &rewritten.bytes)?;
    let manifest = mirror.output_root.join("Cargo.toml");
    transform::transform(&mirror.output_root, &rewritten.package, &rewritten.value)?;
    guest::generate(
        &source_root,
        &mirror.output_root,
        &rewritten.package,
        &discovery,
        &source_types,
    )?;
    Ok(ShadowProject {
        root: mirror.output_root,
        manifest,
        discovery,
        source_types,
    })
}

#[cfg(test)]
mod tests {
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("aimer-shadow-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("app/src")).unwrap();
        Self(path)
    }

    fn app(&self) -> PathBuf { self.0.join("app") }
    fn output(&self) -> PathBuf { self.0.join("shadow") }
    fn write(&self, relative: &str, contents: &str) {
        let path = self.app().join(relative);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).unwrap(); }
        fs::write(path, contents).unwrap();
    }

    fn basic(&self, source: &str) {
        self.write("Cargo.toml", "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n");
        self.write("src/main.rs", source);
    }
}

fn error_for(source: &str) -> ShadowError {
    let project = TempProject::new();
    project.basic(source);
    prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap_err()
}

fn compact(source: &str) -> String {
    source.chars().filter(|character| !character.is_whitespace()).collect()
}

fn guest_config() -> ShadowGuestConfig {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    ShadowGuestConfig::new()
        .aimer_root(&workspace)
        .wasm_guest_root(workspace.join("crates/aimer_wasm_guest"))
        .portable_webbrowser_root(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("portable_webbrowser"),
        )
        .portable_reqwest_root(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("portable_reqwest"),
        )
}

impl Drop for TempProject {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn discovers_direct_root_and_preserves_source_bytes() {
    let project = TempProject::new();
    let source = "#[aimer::main]\nfn main() { AimerApp::new().child(Text::new(\"hello\")).run(); }\n";
    project.basic(source);

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();

    assert_eq!(shadow.discovery().root_expression(), "Text :: new (\"hello\")");
    assert_eq!(fs::read(project.app().join("src/main.rs")).unwrap(), source.as_bytes());
    assert_eq!(fs::read(shadow.root().join("src/main.rs")).unwrap(), source.as_bytes());
}
#[cfg(feature = "hot-reload")]
#[test]
fn transforms_all_non_generic_structs_with_stable_private_module_identities() {
    let project = TempProject::new();
    project.basic(
        "mod outside;\nmod private { struct Inner { value: u32 } }\nstruct Root;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n",
    );
    project.write("src/outside.rs", "struct Hidden(String);\n");

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let main = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());
    let outside = compact(&fs::read_to_string(shadow.root().join("src/outside.rs")).unwrap());

    assert!(main.contains("demo-app::crate::private::Inner"));
    assert!(main.contains("demo-app::crate::Root"));
    assert!(outside.contains("demo-app::crate::outside::Hidden"));
    assert_eq!(main.matches("impl::aimer_widget::portable::AimerReflectionType").count(), 2);
    assert_eq!(outside.matches("impl::aimer_widget::portable::AimerReflectionType").count(), 1);
}

#[test]
fn reflection_transform_preserves_original_widget_source_coordinates() {
    let project = TempProject::new();
    let source = "struct Root { value: u32 }\n\nfn panic_site() {\n    let panic: Option<i32> = Option::None;\n    let _ = panic.unwrap();\n}\n\n#[aimer::main]\nfn main() { AimerApp::new().child(Root { value: 1 }).run(); }\n";
    project.basic(source);

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let transformed = fs::read_to_string(shadow.root().join("src/main.rs")).unwrap();
    let source_line = source
        .lines()
        .position(|line| line.contains("panic.unwrap()"))
        .unwrap();
    let transformed_line = transformed
        .lines()
        .position(|line| line.contains("panic.unwrap()"))
        .unwrap();

    assert_eq!(transformed_line, source_line);
    assert_eq!(
        transformed.lines().nth(transformed_line).unwrap(),
        source.lines().nth(source_line).unwrap(),
    );
}

#[test]
fn classifies_runtime_callbacks_and_adopted_configuration_as_fresh() {
    let project = TempProject::new();
    project.basic(
        "struct NativeSocket;\nstruct State { count: u32, updater: StateUpdater<State>, callback: fn(), label: String, socket: NativeSocket }\nimpl State for State { fn adopt_config_from(&mut self, candidate: Self) { self.label = candidate.label; } }\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n",
    );

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let source = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());

    assert!(source.contains("FieldDescriptor::new(\"count\",\"u32\",::aimer_widget::portable::FieldKind::Retained)"));
    assert!(source.contains("FieldDescriptor::new(\"updater\",\"StateUpdater<State>\",::aimer_widget::portable::FieldKind::Fresh)"));
    assert!(source.contains("FieldDescriptor::new(\"callback\",\"fn()\",::aimer_widget::portable::FieldKind::Fresh)"));
    assert!(source.contains("FieldDescriptor::new(\"label\",\"String\",::aimer_widget::portable::FieldKind::Fresh)"));
    assert!(source.contains("FieldDescriptor::new(\"socket\",\"NativeSocket\",::aimer_widget::portable::FieldKind::Retained)"));
}

#[test]
fn generic_structs_unions_and_enums_are_left_unmodified() {
    let project = TempProject::new();
    let source = "struct Generic<T> { value: T }\nunion Bits { integer: u32, float: f32 }\nenum Choice { One, Two }\nstruct Root;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n";
    project.basic(source);

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let transformed = fs::read_to_string(shadow.root().join("src/main.rs")).unwrap();

    assert!(transformed.contains("struct Generic<T>"));
    assert!(!transformed.contains("AimerReflectionType for Generic"));
    assert!(transformed.contains("union Bits"));
    assert!(transformed.contains("enum Choice"));
}

#[test]
fn non_generic_unit_enums_get_portable_state_codecs() {
    let project = TempProject::new();
    project.basic(
        "enum Choice { One, Two }\nstruct Root { choice: Choice }\n#[aimer::main]\nfn main() { AimerApp::new().child(Root { choice: Choice::One }).run(); }\n",
    );

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let transformed = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());

    assert!(transformed.contains("impl::aimer_widget::portable::PortableEncodeforChoice"));
    assert!(transformed.contains("impl::aimer_widget::portable::PortableDecodeforChoice"));
    assert!(transformed.contains("FieldDescriptor::new(\"choice\",\"Choice\",::aimer_widget::portable::FieldKind::Retained)"));
}

#[test]
fn external_runtime_controller_fields_are_fresh_state() {
    let project = TempProject::new();
    project.basic(
        "struct State { controller: ScrollController, updater: StateUpdater<State> }\nstruct Root;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n",
    );

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let transformed = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());

    assert!(transformed.contains("FieldDescriptor::new(\"controller\",\"ScrollController\",::aimer_widget::portable::FieldKind::Fresh)"));
}

#[test]
fn interior_mutable_shared_runtime_fields_are_fresh_state() {
    let project = TempProject::new();
    project.basic(
        "struct PointerState; struct State { current_state: std::rc::Rc<std::cell::Cell<PointerState>> }\nstruct Root;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n",
    );

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let transformed = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());

    assert!(transformed.contains(
        "FieldDescriptor::new(\"current_state\",\"std::rc::Rc<std::cell::Cell<PointerState>>\",::aimer_widget::portable::FieldKind::Fresh)",
    ));
}

    #[cfg(feature = "hot-reload")]
#[test]
fn transformed_mini_crate_round_trips_and_reports_active_unsupported_fields() {
    let project = TempProject::new();
    let widget = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/aimer_widget");
    project.write(
        "Cargo.toml",
        &format!(
            "[package]\nname = \"portable-shadow-test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\naimer_widget = {{ path = {:?} }}\n",
            widget
        ),
    );
    project.write(
        "src/main.rs",
        r#"
use aimer_widget::portable::{AimerReflectionType, PortableApply, PortableEncode, PortableLimits, StableId128, StateRegistry, StateRegistryError, decode_from_slice, encode_to_vec};

#[derive(Debug, PartialEq)]
struct Inner { values: [u16; 2] }
#[derive(Debug, PartialEq)]
struct Nested {
    enabled: bool,
    inner: Box<Inner>,
    labels: Vec<String>,
    optional: Option<(u32, bool)>,
    #[cfg(any())]
    disabled_native: std::rc::Rc<NativeSocket>,
}
#[derive(Debug, PartialEq)]
struct Pair(u32, String);
#[derive(Debug, PartialEq)]
struct Marker;
#[derive(Debug, PartialEq)]
enum Choice { One, Two }
pub struct PublicState { choice: Choice }

struct NativeSocket;
struct Active { count: u32, socket: std::rc::Rc<NativeSocket> }
struct Fresh { count: u32, label: String, callback: fn() -> u8 }
impl Fresh {
    fn adopt_config_from(&mut self, candidate: Self) { self.label = candidate.label; }
}
fn old_callback() -> u8 { 1 }
fn new_callback() -> u8 { 2 }

#[cfg(any())]
#[aimer::main]
fn entry() { AimerApp::new().child(Root).run(); }

fn main() {
    let limits = PortableLimits::new(16, 64, 256, 256, 4096);
    let value = Nested {
        enabled: true,
        inner: Box::new(Inner { values: [4, 9] }),
        labels: vec!["one".into(), "two".into()],
        optional: Some((7, false)),
    };
    let bytes = encode_to_vec(&value, limits).unwrap();
    assert_eq!(decode_from_slice::<Nested>(&bytes, limits).unwrap(), value);
    let pair = Pair(11, "tuple".into());
    assert_eq!(decode_from_slice::<Pair>(&encode_to_vec(&pair, limits).unwrap(), limits).unwrap(), pair);
    assert_eq!(decode_from_slice::<Marker>(&encode_to_vec(&Marker, limits).unwrap(), limits).unwrap(), Marker);
    assert_ne!(Nested::TYPE_ID, Active::TYPE_ID);
    assert_ne!(Nested::schema_id(), Active::schema_id());

    let old = Fresh { count: 41, label: "old config".into(), callback: old_callback };
    let bytes = encode_to_vec(&old, limits).unwrap();
    let mut decoder = aimer_widget::portable::Decoder::new(&bytes, limits).unwrap();
    let retained = Fresh::decode_retained(&mut decoder).unwrap();
    decoder.finish().unwrap();
    let mut candidate = Fresh { count: 0, label: "new config".into(), callback: new_callback };
    candidate.apply_retained(retained);
    assert_eq!(candidate.count, 41);
    assert_eq!(candidate.label, "new config");
    assert_eq!((candidate.callback)(), 2);

    let slot = StableId128::from_path("test.slot", "nested");
    let mut source = StateRegistry::new(limits);
    source.insert(slot, 1, &value).unwrap();
    let mut document = source.export().unwrap();
    document[44] ^= 1;
    let mut target = StateRegistry::new(limits);
    target.insert(slot, 0, &value).unwrap();
    assert!(matches!(target.import(&document), Err(StateRegistryError::SchemaMismatch { .. })));

    let active = Active { count: 3, socket: std::rc::Rc::new(NativeSocket) };
    let error = encode_to_vec(&active, limits).unwrap_err();
    assert_eq!(
        error.to_string(),
        "active field `socket` of type `std::rc::Rc<NativeSocket>` is not portable; use a hot restart or mark the field fresh"
    );
    let mut bytes = Vec::new();
    3_u32.encode(&mut aimer_widget::portable::Encoder::new(&mut bytes, limits)).unwrap();
    let mut decoder = aimer_widget::portable::Decoder::new(&bytes, limits).unwrap();
    assert!(matches!(Active::decode_retained(&mut decoder), Err(aimer_widget::portable::DecodeError::UnsupportedField { field: "socket", rust_type: "std::rc::Rc<NativeSocket>" })));
    let _ = <Active as AimerReflectionType>::schema();
}
"#,
    );

    let original = fs::read(project.app().join("src/main.rs")).unwrap();
    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    assert_eq!(fs::read(project.app().join("src/main.rs")).unwrap(), original);
    let status = Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--manifest-path"])
        .arg(shadow.manifest())
        .env("CARGO_TARGET_DIR", PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/shadow-tests"))
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn direct_second_transformation_is_idempotent() {
    let project = TempProject::new();
    project.basic(
        "struct Root { count: u32 }\n#[aimer::main]\nfn main() { AimerApp::new().child(Root { count: 1 }).run(); }\n",
    );
    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let first = fs::read(shadow.root().join("src/main.rs")).unwrap();
    let manifest = toml::from_str::<toml::Value>(
        &fs::read_to_string(shadow.manifest()).unwrap(),
    ).unwrap();

    transform::transform(shadow.root(), "demo-app", &manifest).unwrap();

    assert_eq!(fs::read(shadow.root().join("src/main.rs")).unwrap(), first);
}

#[test]
fn fingerprints_ignore_formatting_and_callback_literals_but_not_structure() {
    let first = fingerprint_source("Button::new().on_click(|| println!(\"one\")).size(2)", None).unwrap();
    let formatting = fingerprint_source("Button :: new ( ) . on_click ( || println!(\"two\") ) . size(2)", None).unwrap();
    let moved = fingerprint_source("Button::new().size(2).on_click(|| println!(\"one\"))", None).unwrap();
    let different_property = fingerprint_source("Button::new().on_click(|| println!(\"one\")).size(3)", None).unwrap();

    assert_eq!(first, formatting);
    assert_ne!(first, moved);
    assert_ne!(first, different_property);
}

#[test]
fn portable_key_overrides_fallback_identity() {
    let first = fingerprint_source("Text::new(\"first\")", Some("\"header\"")).unwrap();
    let second = fingerprint_source("Button::new()", Some("\"header\"")).unwrap();
    assert_eq!(first, second);
    assert_eq!(fingerprint_source("Text::new(\"x\")", Some("runtime() ")).unwrap_err().kind(), ShadowErrorKind::DynamicFlow);
    assert_eq!(fingerprint_source("Text::new(\"x\")", Some("runtime_key")).unwrap_err().kind(), ShadowErrorKind::DynamicFlow);
}

#[test]
fn follows_alias_to_helper_in_nested_out_of_line_module() {
    let project = TempProject::new();
    project.basic("mod flow;\nuse crate::flow::launch as go;\n#[aimer::main]\nfn main() { go(); }\n");
    project.write("src/flow.rs", "mod screen;\npub fn launch() { AimerApp::new().child(screen::Home::new()).run(); }\n");
    project.write("src/flow/screen.rs", "pub struct Home;\nimpl Home { pub fn new() -> Self { Self } }\n");

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();

    assert_eq!(shadow.discovery().root_expression(), "screen :: Home :: new ()");
    assert_eq!(shadow.discovery().call_path().len(), 2);
    assert_eq!(shadow.source_types()[0].identity().portable_name(), "demo-app::crate::flow::screen::Home");
}

#[test]
fn discovers_lib_only_source_root_and_inline_module() {
    let project = TempProject::new();
    project.write("Cargo.toml", "[package]\nname = \"demo-lib\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\npath = \"source/app.rs\"\n");
    project.write("source/app.rs", "mod ui { pub struct Root; }\n#[aimer::main]\npub fn boot() { AimerApp::new().child(ui::Root).run(); }\n");

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    assert_eq!(shadow.discovery().entry().type_name(), "boot");
    assert_eq!(shadow.source_types()[0].identity().module(), "crate::ui");
}

#[cfg(feature = "hot-reload")]
#[test]
fn reports_ambiguous_unresolved_dynamic_and_missing_flows_with_spans() {
    let ambiguous = error_for("#[aimer::main]\nfn main() { AimerApp::new().child(One).run(); AimerApp::new().child(Two).run(); }\n");
    let unresolved = error_for("#[aimer::main]\nfn main() { missing_helper(); }\n");
    let dynamic = error_for("fn helper() {}\n#[aimer::main]\nfn main() { let callback = helper; callback(); }\n");
    let missing = error_for("#[aimer::main]\nfn main() { let value = 1; }\n");

    assert_eq!(ambiguous.kind(), ShadowErrorKind::AmbiguousFlow);
    assert_eq!(unresolved.kind(), ShadowErrorKind::UnresolvedFlow);
    assert_eq!(dynamic.kind(), ShadowErrorKind::DynamicFlow);
    assert_eq!(missing.kind(), ShadowErrorKind::MissingFlow);
    assert!(ambiguous.span().is_some() && unresolved.span().is_some() && dynamic.span().is_some() && missing.span().is_some());
}

    #[cfg(feature = "hot-reload")]
#[test]
fn reports_duplicate_module_ownership_and_malformed_source() {
    let project = TempProject::new();
    project.basic("mod duplicated;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n");
    project.write("src/duplicated.rs", "pub struct First;\n");
    project.write("src/duplicated/mod.rs", "pub struct Second;\n");
    let duplicate = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap_err();

    assert_eq!(duplicate.kind(), ShadowErrorKind::DuplicateModule);
    assert_eq!(error_for("#[aimer::main]\nfn main( {\n").kind(), ShadowErrorKind::MalformedSource);
}

    #[cfg(feature = "hot-reload")]
#[test]
fn rewrites_workspace_and_relative_path_dependencies_and_copies_assets() {
    let project = TempProject::new();
    fs::create_dir_all(project.0.join("shared/src")).unwrap();
    fs::write(project.0.join("shared/Cargo.toml"), "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\npath = \"src/lib.rs\"\n").unwrap();
    fs::write(project.0.join("shared/src/lib.rs"), "pub struct Shared;\n").unwrap();
    fs::write(project.0.join("Cargo.toml"), "[workspace]\nmembers = [\"app\", \"shared\"]\n[workspace.dependencies]\nshared = { path = \"shared\" }\n").unwrap();
    project.write("Cargo.toml", "[package]\nname = \"demo-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nshared.workspace = true\nlocal = { package = \"shared\", path = \"../shared\" }\n");
    project.write("src/main.rs", "#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n");
    project.write("assets/logo.txt", "asset bytes");
    let original_manifest = fs::read(project.app().join("Cargo.toml")).unwrap();

    let shadow = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let manifest = fs::read_to_string(shadow.manifest()).unwrap();

    assert!(!manifest.contains("workspace = true"));
    assert!(manifest.contains(project.0.join("shared").to_str().unwrap()));
    assert_eq!(fs::read(shadow.root().join("assets/logo.txt")).unwrap(), b"asset bytes");
    assert_eq!(fs::read(project.app().join("Cargo.toml")).unwrap(), original_manifest);
}

    #[cfg(feature = "hot-reload")]
#[test]
fn repeated_preparation_is_byte_deterministic() {
    let project = TempProject::new();
    project.basic("#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n");
    let first = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    let first_manifest = fs::read(first.manifest()).unwrap();
    let first_source = fs::read(first.root().join("src/main.rs")).unwrap();
    let second = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap();
    assert_eq!(fs::read(second.manifest()).unwrap(), first_manifest);
    assert_eq!(fs::read(second.root().join("src/main.rs")).unwrap(), first_source);
}

#[test]
fn excludes_generated_build_and_target_directories() {
    let project = TempProject::new();
    project.basic("#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n");
    project.write("assets/keep.txt", "asset");
    let generated = "x".repeat(2_048);
    project.write("build/generated.bin", &generated);
    project.write("builds/macos/generated.bin", &generated);
    project.write("target/debug/generated.bin", &generated);

    let shadow = prepare_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default().max_file_bytes(1_024),
    )
    .unwrap();

    assert_eq!(fs::read(shadow.root().join("assets/keep.txt")).unwrap(), b"asset");
    assert!(!shadow.root().join("build").exists());
    assert!(!shadow.root().join("builds").exists());
    assert!(!shadow.root().join("target").exists());
}

#[test]
fn rejects_output_recursion_and_all_resource_bounds() {
    let project = TempProject::new();
    project.basic("#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n");
    project.write("asset.bin", "0123456789");

    let recursion = prepare_shadow_project(&project.app(), &project.app().join("target/shadow"), ShadowLimits::default()).unwrap_err();
    let individual = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default().max_file_bytes(5)).unwrap_err();
    let total = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default().max_total_bytes(5)).unwrap_err();
    let files = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default().max_files(1)).unwrap_err();

    assert_eq!(recursion.kind(), ShadowErrorKind::OutputRecursion);
    assert_eq!(individual.kind(), ShadowErrorKind::LimitExceeded);
    assert_eq!(total.kind(), ShadowErrorKind::LimitExceeded);
    assert_eq!(files.kind(), ShadowErrorKind::LimitExceeded);
}

#[test]
fn missing_entry_and_rewritten_manifest_limit_have_diagnostics() {
    let missing = error_for("fn ordinary() {}\n");
    assert_eq!(missing.kind(), ShadowErrorKind::MissingFlow);
    assert!(missing.span().is_some());

    let project = TempProject::new();
    fs::create_dir_all(project.0.join("dependency/src")).unwrap();
    fs::write(project.0.join("dependency/Cargo.toml"), "[package]\nname=\"dependency\"\nversion=\"0.1.0\"\n").unwrap();
    project.write("Cargo.toml", "[package]\nname=\"demo-app\"\nversion=\"0.1.0\"\n[dependencies]\ndependency={path=\"../dependency\"}\n");
    project.write("src/main.rs", "#[aimer::main]\nfn main(){AimerApp::new().child(Root).run();}\n");
    let original = fs::metadata(project.app().join("Cargo.toml")).unwrap().len();
    let error = prepare_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default().max_file_bytes(original),
    ).unwrap_err();
    assert_eq!(error.kind(), ShadowErrorKind::LimitExceeded);
}

#[test]
fn rejects_excessive_direct_call_depth() {
    let project = TempProject::new();
    project.basic("fn a() { b(); }\nfn b() { AimerApp::new().child(Root).run(); }\n#[aimer::main]\nfn main() { a(); }\n");
    let error = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default().max_ast_call_depth(1)).unwrap_err();
    assert_eq!(error.kind(), ShadowErrorKind::LimitExceeded);
}

#[cfg(unix)]
#[test]
fn rejects_symbolic_link_escape() {
    use std::os::unix::fs::symlink;

    let project = TempProject::new();
    project.basic("#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n");
    fs::write(project.0.join("outside.txt"), "outside").unwrap();
    symlink(project.0.join("outside.txt"), project.app().join("assets-link")).unwrap();

    let error = prepare_shadow_project(&project.app(), &project.output(), ShadowLimits::default()).unwrap_err();
    assert_eq!(error.kind(), ShadowErrorKind::PathEscape);
}

    #[cfg(feature = "hot-reload")]
#[test]
fn generates_linkable_guest_and_exact_helper_root_factory() {
    let project = TempProject::new();
    project.write(
        "Cargo.toml",
        "[package]\nname = \"guest-demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\naimer = { version = \"0.1\", features = [\"existing-feature\"], default-features = false }\n",
    );
    project.write(
        "src/main.rs",
        "mod flow;\n#[aimer::main]\nfn main() { flow::launch(); }\n",
    );
    project.write(
        "src/flow.rs",
        "use aimer::Text;\npub fn launch() { AimerApp::new().child(Text::new(\"guest root\")).run(); }\n",
    );
    let original_main = fs::read(project.app().join("src/main.rs")).unwrap();

    let shadow = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();
    let main = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());
    let flow = compact(&fs::read_to_string(shadow.root().join("src/flow.rs")).unwrap());
    let manifest = fs::read_to_string(shadow.manifest()).unwrap();

    assert!(flow.contains("pubfn__aimer_generated_root_factory()->impl::aimer::Widget{Text::new(\"guestroot\")}"));
    assert!(main.contains("pubstruct__AimerGeneratedGuestProgram"));
    assert!(main.contains("impl::aimer_wasm_guest::GuestProgramfor__AimerGeneratedGuestProgram"));
    assert!(main.contains("::aimer_wasm_guest::export_guest!(__AimerGeneratedGuestProgram,__AIMER_GENERATED_GUEST_LIMITS)"));
    assert!(main.contains("thread_local!"));
    assert!(main.contains("flow::__aimer_generated_root_factory()"));
    assert!(!main.contains("#[aimer::main]"));
    assert!(manifest.contains("autobins = false"));
    assert!(manifest.contains("crate-type = [\"cdylib\", \"rlib\"]"));
    assert!(manifest.contains("portable-guest"));
    assert!(manifest.contains("existing-feature"));
    assert!(manifest.contains("default-features = false"));
    assert!(manifest.contains("aimer_wasm_guest"));
    assert!(manifest.contains(fs::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap().to_str().unwrap()));
    assert_eq!(fs::read(project.app().join("src/main.rs")).unwrap(), original_main);
}

    #[cfg(feature = "hot-reload")]
#[test]
fn guest_shadow_activates_portable_guest_for_application_cfgs() {
    let project = TempProject::new();
    project.basic(
        "struct Root;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n",
    );

    let shadow = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(shadow.manifest()).unwrap()).unwrap();
    let features = manifest
        .get("features")
        .and_then(toml::Value::as_table)
        .expect("guest shadow must define application features");
    assert!(features.contains_key("portable-guest"));
    assert!(features
        .get("default")
        .and_then(toml::Value::as_array)
        .is_some_and(|default| default.iter().any(|feature| {
            feature.as_str() == Some("portable-guest")
        })));
}

    #[cfg(feature = "hot-reload")]
#[test]
fn generated_guest_contains_transactional_state_and_callback_rebuild_flow() {
    let project = TempProject::new();
    project.basic(
        "struct Root;\n#[aimer::main]\nfn main() { AimerApp::new().child(Root).run(); }\n",
    );

    let shadow = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();
    let source = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());

    assert!(source.contains("state_registry_mut().import(entry.payload())"));
    assert!(source.contains("AbiStatus::StateIncompatible"));
    assert!(source.contains("callback_registry().dispatch"));
    assert!(source.contains("take_rebuild_request()"));
    assert!(source.contains("StateBundle::new"));
    assert!(source.contains("__AIMER_GENERATED_APPLICATION_ID,self.generation_id,&entries"));
    assert!(source.contains("fnmigrate_state"));
}

    #[cfg(feature = "hot-reload")]
#[test]
fn generated_guest_output_is_byte_deterministic() {
    let project = TempProject::new();
    project.basic(
        "#[aimer::main]\nfn main() { helper(); }\nfn helper() { AimerApp::new().child(Text::new(\"stable\")).run(); }\n",
    );

    let first = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();
    let first_manifest = fs::read(first.manifest()).unwrap();
    let first_source = fs::read(first.root().join("src/main.rs")).unwrap();
    let second = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();

    assert_eq!(fs::read(second.manifest()).unwrap(), first_manifest);
    assert_eq!(fs::read(second.root().join("src/main.rs")).unwrap(), first_source);
}

    #[cfg(feature = "hot-reload")]
#[test]
fn out_of_line_entry_emits_adapter_at_the_actual_crate_root() {
    let project = TempProject::new();
    project.basic("mod boot;\n");
    project.write(
        "src/boot.rs",
        "#[aimer::main]\npub fn start() { AimerApp::new().child(Text::new(\"nested entry\")).run(); }\n",
    );

    let shadow = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();
    let crate_root = compact(&fs::read_to_string(shadow.root().join("src/main.rs")).unwrap());
    let entry = compact(&fs::read_to_string(shadow.root().join("src/boot.rs")).unwrap());

    assert!(crate_root.contains("pubstruct__AimerGeneratedGuestProgram"));
    assert!(crate_root.contains("crate::boot::__aimer_generated_root_factory()"));
    assert!(!entry.contains("aimer::main"));
    assert!(!entry.contains("fnstart"));
    assert!(entry.contains("pubfn__aimer_generated_root_factory"));
}

    #[cfg(feature = "hot-reload")]
#[test]
fn compiled_guest_builds_and_round_trips_state_with_incompatible_rejection() {
    let project = TempProject::new();
    project.write(
        "Cargo.toml",
        "[package]\nname = \"compiled-guest\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    project.write(
        "src/main.rs",
        r#"
use aimer::{AimerApp, Button, Text};
use std::sync::atomic::{AtomicUsize, Ordering};

static PRESSES: AtomicUsize = AtomicUsize::new(0);

#[aimer::main]
fn main() {
    AimerApp::new()
        .child(Button::new().on_press(|| { PRESSES.fetch_add(1, Ordering::Relaxed); }).child(Text::new("compiled guest")))
        .run();
}

#[cfg(test)]
mod generated_tests {
    use super::*;
    use aimer_wasm_guest::GuestProgram;

    fn limits() -> aimer::anteros::ModelLimits {
        aimer::anteros::ModelLimits::new(16_777_216, 65_536, 1_048_576, 16_777_216)
    }

    #[test]
    fn generated_program_builds_and_transfers_state() {
        let mut old = __AimerGeneratedGuestProgram::default();
        old.initialize(7).unwrap();
        let first = old.build(limits()).unwrap();
        let first = aimer::anteros::WidgetDocumentView::decode(&first, limits()).unwrap();
        assert_eq!(first.generation_id(), 7);
        assert_eq!(first.document_revision(), 0);
        let second = old.build(limits()).unwrap();
        let second = aimer::anteros::WidgetDocumentView::decode(&second, limits()).unwrap();
        assert_eq!(second.document_revision(), 1);

        let binding = (0..second.node_count())
            .find_map(|index| {
                second.node(index).unwrap().callbacks()
                    .find(|binding| binding.event_kind() == aimer::anteros::EVENT_BUTTON_PRESS)
            })
            .unwrap();
        let event = aimer::anteros::CallbackEvent::new(
            7,
            1,
            binding.callback_id(),
            binding.event_kind(),
            binding.event_schema(),
            0,
            &[],
        ).encode(limits()).unwrap();
        let event = aimer::anteros::CallbackEventView::decode(&event, limits()).unwrap();
        let rebuilt = old.dispatch_event(&event, limits()).unwrap().unwrap();
        let rebuilt = aimer::anteros::WidgetDocumentView::decode(&rebuilt, limits()).unwrap();
        assert_eq!(rebuilt.document_revision(), 2);
        assert_eq!(PRESSES.load(Ordering::Relaxed), 1);
        let rebound = old.dispatch_event(&event, limits()).unwrap().unwrap();
        let rebound = aimer::anteros::WidgetDocumentView::decode(&rebound, limits()).unwrap();
        assert_eq!(rebound.document_revision(), 3);
        assert_eq!(PRESSES.load(Ordering::Relaxed), 2);

        let retired = aimer::anteros::CallbackEvent::new(
            6,
            2,
            binding.callback_id(),
            binding.event_kind(),
            binding.event_schema(),
            0,
            &[],
        ).encode(limits()).unwrap();
        let retired = aimer::anteros::CallbackEventView::decode(&retired, limits()).unwrap();
        assert_eq!(
            old.dispatch_event(&retired, limits()).unwrap_err().status(),
            aimer::anteros::AbiStatus::RetiredGeneration,
        );

        let state = old.export_state(limits()).unwrap();
        let state_view = aimer::anteros::StateBundleView::decode(&state, limits()).unwrap();
        let mut candidate = __AimerGeneratedGuestProgram::default();
        candidate.initialize(8).unwrap();
        candidate.import_state(&state_view).unwrap();
        let restored = candidate.export_state(limits()).unwrap();
        let restored = aimer::anteros::StateBundleView::decode(&restored, limits()).unwrap();
        assert_eq!(restored.application_id(), state_view.application_id());
        assert_eq!(restored.entry(0).unwrap().payload(), state_view.entry(0).unwrap().payload());

        let payload = state_view.entry(0).unwrap().payload();
        let incompatible_entries = [aimer::anteros::StateEntry::new(
            __AIMER_GENERATED_STATE_ID,
            aimer::anteros::StableId128::from_bytes([0; 16]),
            aimer::anteros::Version::new(1, 0),
            aimer::anteros::StatePolicy::Required,
            payload,
        )];
        let incompatible = aimer::anteros::StateBundle::new(
            __AIMER_GENERATED_APPLICATION_ID,
            7,
            &incompatible_entries,
        ).encode(limits()).unwrap();
        let incompatible = aimer::anteros::StateBundleView::decode(&incompatible, limits()).unwrap();
        assert_eq!(
            candidate.import_state(&incompatible).unwrap_err().status(),
            aimer::anteros::AbiStatus::StateIncompatible,
        );
    }
}
"#,
    );

    let shadow = prepare_guest_shadow_project(
        &project.app(),
        &project.output(),
        ShadowLimits::default(),
        guest_config(),
    )
    .unwrap();
    let status = Command::new(env!("CARGO"))
        .args(["test", "--quiet", "--manifest-path"])
        .arg(shadow.manifest())
        .arg("--lib")
        .env("CARGO_TARGET_DIR", PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/shadow-guest-tests"))
        .status()
        .unwrap();
    assert!(status.success());
}
}
