use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use quote::ToTokens;

use crate::config::AimerManifest;

use super::shadow::{ShadowGuestConfig, ShadowLimits, prepare_guest_shadow_project};

const GENERATED_PACKAGE: &str = "aimer_generated_guest";
const GENERATED_PROGRAM: &str = "__AimerGeneratedGuestProgram";
const GENERATED_LIMITS: &str = "__AIMER_GENERATED_GUEST_LIMITS";
const STAGING_PREFIX: &str = ".aimer-hot-reload-shadow-";
const TEMPORARY_MARKER: &str = ".aimer-tmp-";
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static GENERATED_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct StagingDirectory(PathBuf);

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if self.0.is_dir() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// Source preparation selected for one hot-reload guest build.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuestGenerationMode {
    /// Discover and transform the application's portable root automatically.
    Automatic,
    /// Use the exact package and symbols declared in `Aimer.toml`.
    Manual,
}

impl GuestGenerationMode {
    /// Selects manual generation only when all explicit metadata is present.
    #[inline]
    pub fn select(manifest: Option<&AimerManifest>) -> Self {
        if manifest.and_then(AimerManifest::hot_reload_guest).is_some() {
            Self::Manual
        } else {
            Self::Automatic
        }
    }
}

/// Explicit application symbols used by the generated guest wrapper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestPackageSpec {
    package: String,
    program: String,
    limits: String,
}

impl GuestPackageSpec {
    /// Creates a wrapper specification from validated project metadata.
    #[inline]
    pub fn new(
        package: impl Into<String>,
        program: impl Into<String>,
        limits: impl Into<String>,
    ) -> Self {
        Self {
            package: package.into(),
            program: program.into(),
            limits: limits.into(),
        }
    }
}

/// Files emitted for one isolated application guest package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedGuestPackage {
    manifest: PathBuf,
    wrapper_root: PathBuf,
    application_root: PathBuf,
    portable_source_root: Option<PathBuf>,
}

impl GeneratedGuestPackage {
    /// Returns the stable generated Cargo package name.
    #[inline]
    pub const fn package(&self) -> &'static str {
        GENERATED_PACKAGE
    }

    /// Returns the generated standalone Cargo manifest.
    #[inline]
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    /// Returns the standalone outer wrapper package root.
    #[inline]
    pub fn wrapper_root(&self) -> &Path {
        &self.wrapper_root
    }

    /// Returns the application package linked by the outer wrapper.
    #[inline]
    pub fn application_root(&self) -> &Path {
        &self.application_root
    }

    /// Returns the original source boundary selected by automatic discovery.
    #[inline]
    pub fn portable_source_root(&self) -> Option<&Path> {
        self.portable_source_root.as_deref()
    }
}

/// Failure while creating an isolated application guest package.
#[derive(Debug)]
pub enum GuestGenerationError {
    /// A manifest value cannot safely identify a Cargo package or Rust path.
    InvalidMetadata(&'static str),
    /// The application package cannot be linked as a Rust dependency.
    ApplicationLibraryUnavailable {
        manifest: PathBuf,
        package: String,
    },
    /// The application Cargo manifest is malformed.
    InvalidCargoManifest {
        manifest: PathBuf,
        source: toml::de::Error,
    },
    /// A configured framework or output path escaped its accepted root.
    PathEscape(String),
    /// Automatic source discovery or transformation failed.
    Shadow(super::shadow::ShadowError),
    /// The generated package could not be persisted.
    Io(io::Error),
}

impl fmt::Display for GuestGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMetadata(field) => {
                write!(formatter, "invalid hot-reload guest metadata in `{field}`")
            }
            Self::ApplicationLibraryUnavailable { manifest, package } => write!(
                formatter,
                "hot-reload package `{package}` must emit an `rlib` from its Rust library target; add `\"rlib\"` to `[lib].crate-type` in {}",
                manifest.display()
            ),
            Self::InvalidCargoManifest { manifest, source } => write!(
                formatter,
                "failed to parse application Cargo manifest {}: {source}",
                manifest.display()
            ),
            Self::PathEscape(message) => formatter.write_str(message),
            Self::Shadow(error) => write!(formatter, "failed to prepare the hot-reload application shadow: {error}"),
            Self::Io(error) => {
                write!(formatter, "failed to generate the hot-reload guest package: {error}")
            }
        }
    }
}

impl std::error::Error for GuestGenerationError {}

/// Freshly prepares an automatically discovered application and its outer export wrapper.
///
/// The completed shadow is always installed at
/// `target/aimer-hot-reload/application`. Preparation itself occurs in an
/// ephemeral sibling directory because the shadow validator intentionally
/// rejects an output nested below its input tree; only changed files are then
/// synchronized into the persistent generated project. The original
/// application is never modified.
pub fn prepare_automatic_guest(
    project_root: &Path,
    workspace_root: &Path,
    session_root: &Path,
) -> Result<GeneratedGuestPackage, GuestGenerationError> {
    let project_root = fs::canonicalize(project_root).map_err(GuestGenerationError::Io)?;
    let workspace_root = fs::canonicalize(workspace_root).map_err(GuestGenerationError::Io)?;
    ensure_cargo_package(&workspace_root, "Aimer workspace root")?;
    let adapter_root = fs::canonicalize(workspace_root.join("crates/aimer_wasm_guest"))
        .map_err(GuestGenerationError::Io)?;
    if !adapter_root.starts_with(&workspace_root) {
        return Err(GuestGenerationError::PathEscape(format!(
            "local aimer_wasm_guest crate {} escapes the Aimer workspace {}",
            adapter_root.display(),
            workspace_root.display()
        )));
    }
    ensure_cargo_package(&adapter_root, "aimer_wasm_guest crate")?;

    let expected_session = project_root.join("target/aimer-hot-reload");
    let session_root = canonical_destination(session_root)?;
    if session_root != expected_session {
        return Err(GuestGenerationError::PathEscape(format!(
            "hot-reload output {} escapes the required session root {}",
            session_root.display(),
            expected_session.display()
        )));
    }

    let parent = project_root.parent().ok_or_else(|| {
        GuestGenerationError::PathEscape("the application root has no safe staging parent".to_owned())
    })?;
    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    cleanup_stale_generation_artifacts(&project_root, &session_root)?;
    let staging_root = parent.join(format!(
        "{STAGING_PREFIX}{}-{sequence}",
        std::process::id()
    ));
    let staging = StagingDirectory(staging_root);
    let shadow = prepare_guest_shadow_project(
        &project_root,
        &staging.0,
        ShadowLimits::default(),
        ShadowGuestConfig::new()
            .aimer_root(&workspace_root)
            .wasm_guest_root(&adapter_root)
            .portable_webbrowser_root(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("portable_webbrowser"),
            )
            .portable_reqwest_root(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("portable_reqwest"),
            ),
    )
    .map_err(GuestGenerationError::Shadow)?;
    let package = shadow.discovery().entry().package().to_owned();
    let portable_source_root = shadow
        .discovery()
        .crate_source()
        .parent()
        .unwrap_or(&project_root)
        .to_owned();
    remove_inner_guest_export(&project_root, &shadow)?;
    let application_root = session_root.join("application");
    let wrapper_root = session_root.join("generated");
    fs::create_dir_all(&session_root).map_err(GuestGenerationError::Io)?;
    sync_generated_directory(shadow.root(), &application_root)?;
    let mut generated = generate_guest_package(
        &application_root,
        &adapter_root,
        &wrapper_root,
        &GuestPackageSpec::new(package, GENERATED_PROGRAM, GENERATED_LIMITS),
    )?;
    generated.portable_source_root = Some(portable_source_root);
    Ok(generated)
}

fn remove_inner_guest_export(
    project_root: &Path,
    shadow: &super::shadow::ShadowProject,
) -> Result<(), GuestGenerationError> {
    let relative_source = shadow
        .discovery()
        .crate_source()
        .strip_prefix(project_root)
        .map_err(|_| {
            GuestGenerationError::PathEscape(format!(
                "discovered crate source {} escapes the application root {}",
                shadow.discovery().crate_source().display(),
                project_root.display()
            ))
        })?;
    let source_path = shadow.root().join(relative_source);
    let source = fs::read_to_string(&source_path).map_err(GuestGenerationError::Io)?;
    let mut syntax = syn::parse_file(&source).map_err(|error| {
        GuestGenerationError::Io(io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    let item_count = syntax.items.len();
    syntax.items.retain(|item| {
        !matches!(item, syn::Item::Macro(item_macro)
            if item_macro.mac.path.segments.last().is_some_and(|segment| segment.ident == "export_guest")
                && item_macro.mac.tokens.to_string().contains(GENERATED_PROGRAM)
                && item_macro.mac.tokens.to_string().contains(GENERATED_LIMITS))
    });
    if syntax.items.len() == item_count {
        return Err(GuestGenerationError::PathEscape(format!(
            "transformed crate source {} did not contain its generated guest export",
            source_path.display()
        )));
    }
    let mut transformed = syntax.into_token_stream().to_string();
    transformed.push('\n');
    fs::write(source_path, transformed).map_err(GuestGenerationError::Io)
}

fn ensure_cargo_package(root: &Path, label: &str) -> Result<(), GuestGenerationError> {
    if root.is_dir() && root.join("Cargo.toml").is_file() {
        Ok(())
    } else {
        Err(GuestGenerationError::PathEscape(format!(
            "{label} is not a local Cargo package: {}",
            root.display()
        )))
    }
}

fn canonical_destination(path: &Path) -> Result<PathBuf, GuestGenerationError> {
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        let name = ancestor.file_name().ok_or_else(|| {
            GuestGenerationError::PathEscape(format!("invalid output path: {}", path.display()))
        })?;
        missing.push(name.to_owned());
        ancestor = ancestor.parent().ok_or_else(|| {
            GuestGenerationError::PathEscape(format!("invalid output path: {}", path.display()))
        })?;
    }
    let mut resolved = fs::canonicalize(ancestor).map_err(GuestGenerationError::Io)?;
    for name in missing.into_iter().rev() {
        if name == "." || name == ".." {
            return Err(GuestGenerationError::PathEscape(format!(
                "output path contains unresolved traversal: {}",
                path.display()
            )));
        }
        resolved.push(name);
    }
    Ok(resolved)
}

/// Removes artifacts abandoned by an earlier process without touching a live
/// generation or files outside the run-owned output roots.
fn cleanup_stale_generation_artifacts(
    project_root: &Path,
    session_root: &Path,
) -> Result<(), GuestGenerationError> {
    let parent = project_root.parent().ok_or_else(|| {
        GuestGenerationError::PathEscape(format!(
            "the application root has no safe staging parent: {}",
            project_root.display()
        ))
    })?;
    cleanup_stale_staging_directories(parent)?;
    cleanup_stale_temporary_files(session_root)
}

fn cleanup_stale_staging_directories(parent: &Path) -> Result<(), GuestGenerationError> {
    let current_pid = std::process::id();
    let entries = fs::read_dir(parent).map_err(GuestGenerationError::Io)?;
    for entry in entries {
        let entry = entry.map_err(GuestGenerationError::Io)?;
        let name = entry.file_name();
        let Some(pid) = artifact_owner_pid(&name.to_string_lossy(), STAGING_PREFIX) else {
            continue;
        };
        if pid == current_pid || process_is_alive(pid) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(GuestGenerationError::Io)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            fs::remove_dir_all(path).map_err(GuestGenerationError::Io)?;
        } else if metadata.is_file() {
            fs::remove_file(path).map_err(GuestGenerationError::Io)?;
        }
    }
    Ok(())
}

fn cleanup_stale_temporary_files(root: &Path) -> Result<(), GuestGenerationError> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(GuestGenerationError::Io(error)),
    };
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(GuestGenerationError::PathEscape(format!(
            "hot-reload session output is not a regular directory: {}",
            root.display()
        )));
    }

    for entry in fs::read_dir(root).map_err(GuestGenerationError::Io)? {
        let entry = entry.map_err(GuestGenerationError::Io)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(GuestGenerationError::Io)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            cleanup_stale_temporary_files(&path)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = artifact_owner_pid(&name, TEMPORARY_MARKER) else {
            continue;
        };
        if pid != std::process::id() && !process_is_alive(pid) {
            fs::remove_file(path).map_err(GuestGenerationError::Io)?;
        }
    }
    Ok(())
}

fn artifact_owner_pid(name: &str, marker: &str) -> Option<u32> {
    let suffix = name.strip_prefix(marker)?;
    suffix.split('-').next()?.parse().ok()
}

fn process_is_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }

    #[cfg(unix)]
    {
        return Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
    }

    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        return Command::new("tasklist")
            .args(["/FI", &filter, "/NH"])
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            })
            .unwrap_or(false);
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SyncReport {
    changed_files: usize,
    removed_entries: usize,
}

#[derive(Default)]
struct GeneratedTree {
    files: BTreeSet<PathBuf>,
    directories: BTreeSet<PathBuf>,
}

/// Synchronizes a validated staging tree into a dedicated generated output.
///
/// The staging tree is read and validated before the destination is touched.
/// Each changed file is compared by bytes and replaced through a temporary
/// sibling file, so unchanged generated files retain their timestamps and
/// Cargo can reuse its incremental fingerprints.
fn sync_generated_directory(
    source_root: &Path,
    destination_root: &Path,
) -> Result<SyncReport, GuestGenerationError> {
    let source_tree = collect_generated_tree(source_root, true)?;
    // Read every staged file before removing stale destination entries. This
    // keeps ordinary staging/read failures from leaving a partially updated
    // live tree.
    for relative in &source_tree.files {
        fs::read(source_root.join(relative)).map_err(GuestGenerationError::Io)?;
    }
    let destination_tree = collect_generated_tree(destination_root, false)?;
    let mut report = synchronize_tree_layout(
        destination_root,
        &destination_tree,
        &source_tree,
    )?;

    for relative in &source_tree.files {
        let contents = fs::read(source_root.join(relative)).map_err(GuestGenerationError::Io)?;
        if write_generated_file(&destination_root.join(relative), &contents)? {
            report.changed_files += 1;
        }
    }
    Ok(report)
}

/// Synchronizes in-memory generated files into a dedicated output directory.
fn sync_generated_files<I>(
    destination_root: &Path,
    files: I,
) -> Result<SyncReport, GuestGenerationError>
where
    I: IntoIterator<Item = (PathBuf, Vec<u8>)>,
{
    let mut desired_files = BTreeMap::new();
    for (relative, contents) in files {
        validate_generated_relative_path(&relative)?;
        if desired_files.insert(relative.clone(), contents).is_some() {
            return Err(GuestGenerationError::PathEscape(format!(
                "generated output contains duplicate file: {}",
                relative.display()
            )));
        }
    }
    let desired_tree = tree_for_files(desired_files.keys());
    let destination_tree = collect_generated_tree(destination_root, false)?;
    let mut report = synchronize_tree_layout(
        destination_root,
        &destination_tree,
        &desired_tree,
    )?;
    for (relative, contents) in desired_files {
        if write_generated_file(&destination_root.join(relative), &contents)? {
            report.changed_files += 1;
        }
    }
    Ok(report)
}

fn tree_for_files<'a, I>(files: I) -> GeneratedTree
where
    I: IntoIterator<Item = &'a PathBuf>,
{
    let mut tree = GeneratedTree::default();
    for file in files {
        tree.files.insert(file.clone());
        let mut parent = file.parent();
        while let Some(relative) = parent {
            if relative.as_os_str().is_empty() {
                break;
            }
            tree.directories.insert(relative.to_owned());
            parent = relative.parent();
        }
    }
    tree
}

fn collect_generated_tree(
    root: &Path,
    required: bool,
) -> Result<GeneratedTree, GuestGenerationError> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound && !required => {
            return Ok(GeneratedTree::default());
        }
        Err(error) => return Err(GuestGenerationError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GuestGenerationError::PathEscape(format!(
            "generated output is not a regular directory: {}",
            root.display()
        )));
    }

    let mut tree = GeneratedTree::default();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative_directory) = pending.pop() {
        let directory = root.join(&relative_directory);
        let mut entries = fs::read_dir(&directory)
            .map_err(GuestGenerationError::Io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(GuestGenerationError::Io)?;
        entries.sort_unstable_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("generated entry is below its root")
                .to_owned();
            let metadata = fs::symlink_metadata(&path).map_err(GuestGenerationError::Io)?;
            if metadata.file_type().is_symlink() {
                return Err(GuestGenerationError::PathEscape(format!(
                    "symbolic links are not allowed in generated output: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                tree.directories.insert(relative.clone());
                pending.push(relative);
            } else if metadata.is_file() {
                tree.files.insert(relative);
            } else {
                return Err(GuestGenerationError::PathEscape(format!(
                    "unsupported filesystem entry in generated output: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(tree)
}

fn synchronize_tree_layout(
    destination_root: &Path,
    existing: &GeneratedTree,
    desired: &GeneratedTree,
) -> Result<SyncReport, GuestGenerationError> {
    let mut report = SyncReport::default();
    for relative in &existing.files {
        if desired.files.contains(relative) {
            continue;
        }
        fs::remove_file(destination_root.join(relative)).map_err(GuestGenerationError::Io)?;
        report.removed_entries += 1;
    }

    let mut existing_directories = existing.directories.iter().collect::<Vec<_>>();
    existing_directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for relative in existing_directories {
        if desired.directories.contains(relative) {
            continue;
        }
        let path = destination_root.join(relative);
        if desired.files.contains(relative) {
            fs::remove_dir_all(&path).map_err(GuestGenerationError::Io)?;
        } else {
            fs::remove_dir(&path).map_err(GuestGenerationError::Io)?;
        }
        report.removed_entries += 1;
    }

    if !destination_root.exists() {
        fs::create_dir_all(destination_root).map_err(GuestGenerationError::Io)?;
    }
    let mut desired_directories = desired.directories.iter().collect::<Vec<_>>();
    desired_directories.sort_by_key(|path| path.components().count());
    for relative in desired_directories {
        fs::create_dir_all(destination_root.join(relative)).map_err(GuestGenerationError::Io)?;
    }
    Ok(report)
}

fn validate_generated_relative_path(path: &Path) -> Result<(), GuestGenerationError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(GuestGenerationError::PathEscape(format!(
            "invalid generated output path: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Writes one generated file only when its bytes differ from the live file.
/// The replacement is made through a temporary sibling file to avoid exposing
/// a partially written Rust source or Cargo manifest to another process.
fn write_generated_file(path: &Path, contents: &[u8]) -> Result<bool, GuestGenerationError> {
    let existing = match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(GuestGenerationError::PathEscape(format!(
                    "generated file is not a regular file: {}",
                    path.display()
                )));
            }
            Some(fs::read(path).map_err(GuestGenerationError::Io)?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(GuestGenerationError::Io(error)),
    };
    if existing.as_deref() == Some(contents) {
        return Ok(false);
    }

    let parent = path.parent().ok_or_else(|| {
        GuestGenerationError::PathEscape(format!("generated file has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(GuestGenerationError::Io)?;
    let file_name = path.file_name().ok_or_else(|| {
        GuestGenerationError::PathEscape(format!("generated file has no name: {}", path.display()))
    })?;
    let (temporary, mut file) = loop {
        let sequence = GENERATED_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{}.aimer-tmp-{}-{sequence}",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                break (candidate, file);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(GuestGenerationError::Io(error)),
        }
    };
    let result = (|| {
        file.write_all(contents).map_err(GuestGenerationError::Io)?;
        file.sync_all().map_err(GuestGenerationError::Io)?;
        drop(file);
        #[cfg(windows)]
        if fs::symlink_metadata(path).is_ok() {
            fs::remove_file(path).map_err(GuestGenerationError::Io)?;
        }
        fs::rename(&temporary, path).map_err(GuestGenerationError::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| true)
}

/// Generates a standalone `cdylib` wrapper for the declared portable program.
///
/// The generated package depends only on the application's library target and
/// `aimer_wasm_guest`. The application must emit a Rust library that can be
/// statically linked into `wasm32-unknown-unknown`; native dynamic library
/// outputs do not satisfy that contract. The package is placed outside the
/// application source tree so the watcher cannot observe its own output, and
/// `[workspace]` prevents Cargo from treating it as an undeclared member of the
/// application's workspace.
pub fn generate_guest_package(
    project_root: &Path,
    adapter_root: &Path,
    output_root: &Path,
    spec: &GuestPackageSpec,
) -> Result<GeneratedGuestPackage, GuestGenerationError> {
    validate_package(&spec.package)?;
    validate_rust_path("program", &spec.program)?;
    validate_rust_path("limits", &spec.limits)?;
    validate_application_library(project_root, &spec.package)?;
    cleanup_stale_temporary_files(output_root)?;
    let project_path = toml::Value::String(project_root.to_string_lossy().into_owned()).to_string();
    let adapter_path = toml::Value::String(adapter_root.to_string_lossy().into_owned()).to_string();
    let package = toml::Value::String(spec.package.clone()).to_string();
    let manifest = format!(
        "[package]\nname = \"{GENERATED_PACKAGE}\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\naimer_wasm_guest = {{ path = {adapter_path} }}\napplication = {{ path = {project_path}, package = {package} }}\n\n[workspace]\n\n[profile.dev]\ndebug = 0\n"
    );
    let source = format!(
        "aimer_wasm_guest::export_guest!(\n    application::{},\n    application::{},\n);\n",
        spec.program, spec.limits
    );
    let manifest_path = output_root.join("Cargo.toml");
    let mut generated_files = vec![
        (PathBuf::from("Cargo.toml"), manifest.into_bytes()),
        (PathBuf::from("src/lib.rs"), source.into_bytes()),
    ];
    let lockfile = project_root.join("Cargo.lock");
    if lockfile.is_file() {
        generated_files.push((
            PathBuf::from("Cargo.lock"),
            fs::read(lockfile).map_err(GuestGenerationError::Io)?,
        ));
    }
    sync_generated_files(output_root, generated_files)?;
    Ok(GeneratedGuestPackage {
        manifest: manifest_path,
        wrapper_root: output_root.to_owned(),
        application_root: project_root.to_owned(),
        portable_source_root: None,
    })
}

fn validate_application_library(
    project_root: &Path,
    package: &str,
) -> Result<(), GuestGenerationError> {
    let manifest_path = project_root.join("Cargo.toml");
    let contents = fs::read_to_string(&manifest_path).map_err(GuestGenerationError::Io)?;
    let manifest: toml::Value =
        toml::from_str(&contents).map_err(|source| GuestGenerationError::InvalidCargoManifest {
            manifest: manifest_path.clone(),
            source,
        })?;
    let library = manifest.get("lib");
    let implicit_library = library.is_none() && project_root.join("src/lib.rs").is_file();
    let emits_rlib = match library.and_then(|value| value.get("crate-type")) {
        Some(crate_types) => crate_types.as_array().is_some_and(|crate_types| {
            crate_types.iter().any(|crate_type| {
                matches!(crate_type.as_str(), Some("lib" | "rlib"))
            })
        }),
        None => library.is_some() || implicit_library,
    };

    if emits_rlib {
        Ok(())
    } else {
        Err(GuestGenerationError::ApplicationLibraryUnavailable {
            manifest: manifest_path,
            package: package.to_owned(),
        })
    }
}

fn validate_package(package: &str) -> Result<(), GuestGenerationError> {
    let valid = !package.is_empty()
        && package.len() <= 128
        && package.as_bytes()[0].is_ascii_alphanumeric()
        && package
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(GuestGenerationError::InvalidMetadata("package"))
    }
}

fn validate_rust_path(field: &'static str, path: &str) -> Result<(), GuestGenerationError> {
    let valid = !path.is_empty()
        && path.len() <= 256
        && path.split("::").all(|segment| {
            let mut bytes = segment.bytes();
            matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic() || byte == b'_')
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(GuestGenerationError::InvalidMetadata(field))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;

    use crate::config::AimerManifest;

    use super::*;

    fn automatic_project(root: &Path) -> (PathBuf, PathBuf) {
        let workspace = root.join("framework");
        let project = root.join("counter");
        fs::create_dir_all(workspace.join("crates/aimer_wasm_guest")).unwrap();
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(workspace.join("Cargo.toml"), "[package]\nname = \"aimer\"\nversion = \"0.0.0\"\n").unwrap();
        fs::write(
            workspace.join("crates/aimer_wasm_guest/Cargo.toml"),
            "[package]\nname = \"aimer_wasm_guest\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"counter\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\naimer = \"0\"\n",
        )
        .unwrap();
        fs::write(
            project.join("src/main.rs"),
            "#[aimer::main]\nfn main() { AimerApp::new().child(Text::new(\"one\")).run(); }\n",
        )
        .unwrap();
        (workspace, project)
    }

    #[test]
    fn absent_manifest_or_metadata_selects_automatic_but_explicit_metadata_stays_manual() {
        assert_eq!(GuestGenerationMode::select(None), GuestGenerationMode::Automatic);
        let without_metadata: AimerManifest = toml::from_str(
            "[package]\nname = \"counter\"\ngroup = \"dev.example\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(
            GuestGenerationMode::select(Some(&without_metadata)),
            GuestGenerationMode::Automatic
        );
        let explicit: AimerManifest = toml::from_str(
            "[package]\nname = \"counter\"\ngroup = \"dev.example\"\nversion = \"0.1.0\"\n\n[build.hot_reload]\npackage = \"manual_guest\"\nprogram = \"guest::Program\"\nlimits = \"guest::LIMITS\"\n",
        )
        .unwrap();
        assert_eq!(
            GuestGenerationMode::select(Some(&explicit)),
            GuestGenerationMode::Manual
        );
    }

    #[test]
    fn automatic_preparation_is_fresh_and_leaves_the_application_unchanged() {
        let root = tempdir().unwrap();
        let (workspace, project) = automatic_project(root.path());
        let original = fs::read(project.join("src/main.rs")).unwrap();
        let session = project.join("target/aimer-hot-reload");

        let first = prepare_automatic_guest(&project, &workspace, &session).unwrap();
        assert_eq!(
            first.application_root(),
            fs::canonicalize(&project).unwrap().join("target/aimer-hot-reload/application")
        );
        assert_eq!(first.package(), "aimer_generated_guest");
        let portable_source_root = fs::canonicalize(project.join("src")).unwrap();
        assert_eq!(
            first.portable_source_root(),
            Some(portable_source_root.as_path())
        );
        assert!(first.application_root().join("src/main.rs").is_file());
        fs::write(first.application_root().join("stale.rs"), "stale").unwrap();
        fs::write(
            project.join("src/main.rs"),
            "#[aimer::main]\nfn main() { AimerApp::new().child(Text::new(\"two\")).run(); }\n",
        )
        .unwrap();

        let second = prepare_automatic_guest(&project, &workspace, &session).unwrap();
        assert!(!second.application_root().join("stale.rs").exists());
        let wrapper = fs::read_to_string(second.manifest()).unwrap();
        assert!(wrapper.contains("name = \"aimer_generated_guest\""));
        assert!(wrapper.contains("package = \"counter\""));
        let source = fs::read_to_string(second.wrapper_root().join("src/lib.rs")).unwrap();
        assert!(source.contains("application::__AimerGeneratedGuestProgram"));
        assert!(source.contains("application::__AIMER_GENERATED_GUEST_LIMITS"));
        assert_ne!(fs::read(project.join("src/main.rs")).unwrap(), original);
        assert_eq!(
            fs::read_to_string(project.join("src/main.rs")).unwrap(),
            "#[aimer::main]\nfn main() { AimerApp::new().child(Text::new(\"two\")).run(); }\n"
        );
        assert!(
            root.path()
                .read_dir()
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().starts_with(".aimer-hot-reload-shadow-"))
        );
    }

    #[test]
    fn generated_tree_sync_updates_only_changed_files_and_removes_stale_entries() {
        let root = tempdir().unwrap();
        let staged = root.path().join("staged");
        let live = root.path().join("live");
        fs::create_dir_all(staged.join("src")).unwrap();
        fs::create_dir_all(live.join("src")).unwrap();
        fs::create_dir_all(live.join("old")).unwrap();
        fs::write(staged.join("src/lib.rs"), "new").unwrap();
        fs::write(staged.join("src/unchanged.rs"), "same").unwrap();
        fs::write(live.join("src/lib.rs"), "old").unwrap();
        fs::write(live.join("src/unchanged.rs"), "same").unwrap();
        fs::write(live.join("old/stale.rs"), "stale").unwrap();

        let report = sync_generated_directory(&staged, &live).unwrap();

        assert_eq!(report.changed_files, 1);
        assert_eq!(report.removed_entries, 2);
        assert_eq!(fs::read_to_string(live.join("src/lib.rs")).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(live.join("src/unchanged.rs")).unwrap(),
            "same"
        );
        assert!(!live.join("old/stale.rs").exists());
        assert!(!live.join("old").exists());
    }

    #[test]
    fn generated_tree_sync_rejects_invalid_staging_before_touching_live_output() {
        let root = tempdir().unwrap();
        let staged = root.path().join("staged");
        let live = root.path().join("live");
        fs::write(&staged, "not a directory").unwrap();
        fs::create_dir_all(&live).unwrap();
        fs::write(live.join("existing.rs"), "keep").unwrap();

        assert!(sync_generated_directory(&staged, &live).is_err());
        assert_eq!(
            fs::read_to_string(live.join("existing.rs")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn generated_file_write_skips_identical_bytes() {
        let root = tempdir().unwrap();
        let path = root.path().join("generated.rs");

        assert!(write_generated_file(&path, b"same").unwrap());
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert!(!write_generated_file(&path, b"same").unwrap());
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert!(write_generated_file(&path, b"changed").unwrap());
        assert_eq!(fs::read_to_string(path).unwrap(), "changed");
    }

    #[test]
    fn stale_generation_artifacts_are_reclaimed_without_touching_unrelated_files() {
        let root = tempdir().unwrap();
        let (workspace, project) = automatic_project(root.path());
        let session = project.join("target/aimer-hot-reload");

        let stale_shadow = root.path().join(".aimer-hot-reload-shadow-999999-1");
        fs::create_dir_all(&stale_shadow).unwrap();
        fs::write(stale_shadow.join("partial.rs"), "partial").unwrap();
        let current_shadow = root
            .path()
            .join(format!(".aimer-hot-reload-shadow-{}-2", std::process::id()));
        fs::create_dir_all(&current_shadow).unwrap();

        let stale_temporary = session.join("generated/src/.lib.rs.aimer-tmp-999999-1");
        fs::create_dir_all(stale_temporary.parent().unwrap()).unwrap();
        fs::write(&stale_temporary, "partial").unwrap();
        let live_file = session.join("generated/src/lib.rs");
        fs::write(&live_file, "live").unwrap();
        let unrelated = root.path().join("keep.txt");
        fs::write(&unrelated, "keep").unwrap();

        prepare_automatic_guest(&project, &workspace, &session).unwrap();

        assert!(!stale_shadow.exists());
        assert!(current_shadow.exists());
        assert!(!stale_temporary.exists());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "keep");
        assert!(live_file.exists());
    }

    #[test]
    fn automatic_preparation_rejects_framework_crates_that_escape_the_workspace() {
        let root = tempdir().unwrap();
        let (workspace, project) = automatic_project(root.path());
        let outside = root.path().join("outside");
        fs::rename(workspace.join("crates/aimer_wasm_guest"), &outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, workspace.join("crates/aimer_wasm_guest")).unwrap();

        let error = prepare_automatic_guest(
            &project,
            &workspace,
            &project.join("target/aimer-hot-reload"),
        )
        .unwrap_err()
        .to_string();

        #[cfg(unix)]
        assert!(error.contains("escapes the Aimer workspace"), "{error}");
    }

    #[test]
    fn automatic_outer_wrapper_links_the_transformed_application() {
        let root = tempdir().unwrap();
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let project = root.path().join("application");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            format!(
                "[package]\nname = \"automatic-link-test\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\naimer = {{ path = {:?} }}\n",
                workspace
            ),
        )
        .unwrap();
        fs::write(
            project.join("src/main.rs"),
            "use aimer::*;\n#[aimer::main]\nfn main() { AimerApp::new().child(Text::new(\"hello\")).run(); }\n",
        )
        .unwrap();
        let generated = prepare_automatic_guest(
            &project,
            &workspace,
            &project.join("target/aimer-hot-reload"),
        )
        .unwrap();

        let status = Command::new(env!("CARGO"))
            .args(["build", "--manifest-path"])
            .arg(generated.manifest())
            .args([
                "--package",
                generated.package(),
                "--target",
                "wasm32-unknown-unknown",
                "--target-dir",
            ])
            .arg(project.join("target/aimer-hot-reload/guest"))
            .status()
            .unwrap();

        assert!(status.success());
    }

    #[test]
    fn generated_guest_package_wraps_the_explicit_portable_entry() {
        let root = tempdir().unwrap();
        let project = root.path().join("counter-app");
        let adapter = root.path().join("aimer_wasm_guest");
        let output = root.path().join("target/aimer-hot-reload/generated-guest");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"counter-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"lib\"]\n",
        )
        .unwrap();
        fs::create_dir_all(&adapter).unwrap();
        let spec = GuestPackageSpec::new(
            "counter-app",
            "guest::CounterProgram",
            "guest::HOT_RELOAD_LIMITS",
        );

        let generated = generate_guest_package(&project, &adapter, &output, &spec).unwrap();

        assert_eq!(generated.package(), "aimer_generated_guest");
        assert_eq!(generated.manifest(), output.join("Cargo.toml"));
        assert_eq!(
            fs::read_to_string(generated.manifest()).unwrap(),
            format!(
                "[package]\nname = \"aimer_generated_guest\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\naimer_wasm_guest = {{ path = {:?} }}\napplication = {{ path = {:?}, package = \"counter-app\" }}\n\n[workspace]\n\n[profile.dev]\ndebug = 0\n",
                adapter.to_string_lossy(),
                project.to_string_lossy(),
            )
        );
        assert_eq!(
            fs::read_to_string(output.join("src/lib.rs")).unwrap(),
            "aimer_wasm_guest::export_guest!(\n    application::guest::CounterProgram,\n    application::guest::HOT_RELOAD_LIMITS,\n);\n"
        );
    }

    #[test]
    fn generated_guest_disables_debug_information_to_keep_wasm_bounded() {
        let root = tempdir().unwrap();
        let project = root.path().join("counter-app");
        let adapter = root.path().join("aimer_wasm_guest");
        let output = root.path().join("target/aimer-hot-reload/generated-guest");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"counter-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"lib\"]\n",
        )
        .unwrap();
        fs::create_dir_all(&adapter).unwrap();
        let spec = GuestPackageSpec::new("counter-app", "Program", "LIMITS");

        let generated = generate_guest_package(&project, &adapter, &output, &spec).unwrap();
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(generated.manifest()).unwrap()).unwrap();

        assert_eq!(manifest["profile"]["dev"]["debug"].as_integer(), Some(0));
    }

    #[test]
    fn generated_guest_preserves_the_application_lockfile() {
        let root = tempdir().unwrap();
        let project = root.path().join("counter-app");
        let adapter = root.path().join("aimer_wasm_guest");
        let output = root.path().join("target/aimer-hot-reload/generated-guest");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"counter-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"lib\"]\n",
        )
        .unwrap();
        fs::write(project.join("Cargo.lock"), "application-lock").unwrap();
        fs::create_dir_all(&adapter).unwrap();

        generate_guest_package(
            &project,
            &adapter,
            &output,
            &GuestPackageSpec::new("counter-app", "Program", "LIMITS"),
        )
        .unwrap();

        assert_eq!(fs::read_to_string(output.join("Cargo.lock")).unwrap(), "application-lock");
    }

    #[test]
    fn generated_guest_removes_a_lockfile_when_the_application_no_longer_has_one() {
        let root = tempdir().unwrap();
        let project = root.path().join("counter-app");
        let adapter = root.path().join("aimer_wasm_guest");
        let output = root.path().join("target/aimer-hot-reload/generated-guest");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"counter-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"lib\"]\n",
        )
        .unwrap();
        fs::write(project.join("Cargo.lock"), "application-lock").unwrap();
        fs::create_dir_all(&adapter).unwrap();
        let spec = GuestPackageSpec::new("counter-app", "Program", "LIMITS");

        generate_guest_package(&project, &adapter, &output, &spec).unwrap();
        fs::remove_file(project.join("Cargo.lock")).unwrap();
        generate_guest_package(&project, &adapter, &output, &spec).unwrap();

        assert!(!output.join("Cargo.lock").exists());
    }

    #[test]
    fn invalid_or_empty_metadata_is_rejected_before_writing_source() {
        let root = tempdir().unwrap();
        let output = root.path().join("generated");
        let cases = [
            GuestPackageSpec::new("", "Program", "LIMITS"),
            GuestPackageSpec::new("app", "", "LIMITS"),
            GuestPackageSpec::new("app", "guest::Program; fn injected() {}", "LIMITS"),
            GuestPackageSpec::new("app", "guest::Program", "guest::"),
        ];

        for spec in cases {
            let error = generate_guest_package(root.path(), root.path(), &output, &spec)
                .unwrap_err()
                .to_string();

            assert!(error.contains("invalid hot-reload guest metadata"));
            assert!(!output.join("src/lib.rs").exists());
        }
    }

    #[test]
    fn application_library_must_emit_an_rlib_for_the_generated_dependency() {
        let root = tempdir().unwrap();
        let project = root.path().join("counter-app");
        let output = root.path().join("generated");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"counter-app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\", \"staticlib\"]\n",
        )
        .unwrap();

        let error = generate_guest_package(
            &project,
            root.path(),
            &output,
            &GuestPackageSpec::new("counter-app", "Program", "LIMITS"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("must emit an `rlib`"), "{error}");
        assert!(error.contains("crate-type"), "{error}");
        assert!(!output.join("src/lib.rs").exists());
    }

    #[test]
    fn cargo_default_library_forms_are_linkable_guest_dependencies() {
        let root = tempdir().unwrap();
        let explicit = root.path().join("explicit");
        let implicit = root.path().join("implicit");
        fs::create_dir_all(explicit.join("src")).unwrap();
        fs::create_dir_all(implicit.join("src")).unwrap();
        fs::write(
            explicit.join("Cargo.toml"),
            "[package]\nname = \"explicit\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/application.rs\"\n",
        )
        .unwrap();
        fs::write(explicit.join("src/application.rs"), "").unwrap();
        fs::write(
            implicit.join("Cargo.toml"),
            "[package]\nname = \"implicit\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        fs::write(implicit.join("src/lib.rs"), "").unwrap();

        assert!(validate_application_library(&explicit, "explicit").is_ok());
        assert!(validate_application_library(&implicit, "implicit").is_ok());
    }
}
