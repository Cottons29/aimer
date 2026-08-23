use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use wasm_bindgen_cli_support::Bindgen;
use wasmparser::{Parser, Payload, TypeRef};
use walrus::{FunctionBuilder, FunctionKind, ImportKind, Module};

use super::session::HOST_RELOAD_FEATURE;

const GUEST_RUSTFLAGS: &[&str] = &[
    "--cfg",
    "aimer_portable_guest",
    "--check-cfg=cfg(aimer_portable_guest)",
];

/// A process invocation generated without spawning the compiler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildCommand {
    program: &'static str,
    arguments: Vec<String>,
}

impl BuildCommand {
    /// Returns the compiler executable.
    #[inline]
    pub const fn program(&self) -> &'static str {
        self.program
    }

    /// Returns the exact non-secret compiler arguments.
    #[inline]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

/// Separate compiler plans for the portable guest and development host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HotReloadBuildPlan {
    workspace_manifest: PathBuf,
    guest_manifest: PathBuf,
    guest_package: String,
    host_package: String,
    guest_target_dir: PathBuf,
    guest_source_remap: Option<(PathBuf, PathBuf)>,
}

impl HotReloadBuildPlan {
    /// Creates a plan whose guest output cannot overlap Cargo's native target.
    pub fn new(
        workspace_manifest: PathBuf,
        guest_package: impl Into<String>,
        host_package: impl Into<String>,
        guest_target_dir: PathBuf,
    ) -> Self {
        Self {
            guest_manifest: workspace_manifest.clone(),
            workspace_manifest,
            guest_package: guest_package.into(),
            host_package: host_package.into(),
            guest_target_dir,
            guest_source_remap: None,
        }
    }

    /// Uses a standalone generated manifest for the portable guest build.
    #[inline]
    pub fn guest_manifest(mut self, guest_manifest: PathBuf) -> Self {
        self.guest_manifest = guest_manifest;
        self
    }

    /// Remaps the generated guest source root back to the application source.
    ///
    /// Rustc records the remapped path in panic-hook locations, so a guest
    /// diagnostic can point at the user's file even though the compiler read a
    /// shadow copy from the hot-reload staging directory.
    #[inline]
    pub fn guest_source_remap(
        mut self,
        generated_root: impl Into<PathBuf>,
        source_root: impl Into<PathBuf>,
    ) -> Self {
        self.guest_source_remap = Some((generated_root.into(), source_root.into()));
        self
    }

    /// Returns the portable guest compilation command.
    pub fn guest_command(&self) -> BuildCommand {
        BuildCommand {
            program: "cargo",
            arguments: vec![
                "build".to_owned(),
                "--manifest-path".to_owned(),
                self.guest_manifest.to_string_lossy().into_owned(),
                "--package".to_owned(),
                self.guest_package.clone(),
                "--target".to_owned(),
                "wasm32-unknown-unknown".to_owned(),
                "--target-dir".to_owned(),
                self.guest_target_dir.to_string_lossy().into_owned(),
                "--config".to_owned(),
                self.guest_compiler_config(),
            ],
        }
    }

    fn guest_compiler_config(&self) -> String {
        let mut flags = GUEST_RUSTFLAGS
            .iter()
            .map(|flag| (*flag).to_owned())
            .collect::<Vec<_>>();
        if let Some((generated_root, source_root)) = &self.guest_source_remap {
            flags.push("--remap-path-prefix".to_owned());
            flags.push(format!(
                "{}={}",
                generated_root.to_string_lossy(),
                source_root.to_string_lossy()
            ));
        }
        format!(
            "target.wasm32-unknown-unknown.rustflags = {}",
            serde_json::to_string(&flags).expect("guest rustflags are always serializable")
        )
    }

    /// Returns the native host compilation command with listener support.
    pub fn host_command(&self) -> BuildCommand {
        BuildCommand {
            program: "cargo",
            arguments: vec![
                "build".to_owned(),
                "--manifest-path".to_owned(),
                self.workspace_manifest.to_string_lossy().into_owned(),
                "--package".to_owned(),
                self.host_package.clone(),
                "--features".to_owned(),
                HOST_RELOAD_FEATURE.to_owned(),
            ],
        }
    }

    /// Returns the isolated guest compiler output root.
    #[inline]
    pub fn guest_target_dir(&self) -> &Path {
        &self.guest_target_dir
    }
}

/// A local guest artifact failed bounded preflight validation.
#[derive(Debug)]
pub struct ArtifactError(String);

impl fmt::Display for ArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ArtifactError {}

/// Reads and validates a bounded module emitted under the isolated guest root.
pub fn load_guest_artifact(
    path: &Path,
    guest_target_dir: &Path,
    max_module_bytes: usize,
) -> Result<Vec<u8>, ArtifactError> {
    let guest_target_dir = guest_target_dir
        .canonicalize()
        .map_err(|error| ArtifactError(format!("guest target directory is unavailable: {error}")))?;
    let path = path
        .canonicalize()
        .map_err(|error| ArtifactError(format!("guest artifact is unavailable: {error}")))?;
    if !path.starts_with(&guest_target_dir) {
        return Err(ArtifactError(
            "artifact is outside the isolated guest target directory".to_owned(),
        ));
    }

    let file = File::open(&path)
        .map_err(|error| ArtifactError(format!("failed to open guest artifact: {error}")))?;
    let length = file
        .metadata()
        .map_err(|error| ArtifactError(format!("failed to inspect guest artifact: {error}")))?
        .len();
    if length == 0 || length > max_module_bytes as u64 {
        return Err(ArtifactError(format!(
            "guest artifact is {length} bytes; expected 1..={max_module_bytes}"
        )));
    }
    let read_limit = u64::try_from(max_module_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| ArtifactError(format!("failed to read guest artifact: {error}")))?;
    if bytes.len() > max_module_bytes {
        return Err(ArtifactError(
            "guest artifact grew beyond the configured limit while reading".to_owned(),
        ));
    }
    validate_complete_module(&bytes)?;
    if has_wasm_bindgen_descriptors(&bytes)? {
        bytes = process_wasm_bindgen_descriptors(&path, max_module_bytes)?;
        validate_complete_module(&bytes)?;
    }
    Ok(bytes)
}

fn validate_complete_module(bytes: &[u8]) -> Result<(), ArtifactError> {
    let mut reached_end = false;
    for payload in Parser::new(0).parse_all(bytes) {
        if matches!(
            payload.map_err(|error| {
                ArtifactError(format!("guest artifact is not valid WebAssembly: {error}"))
            })?,
            Payload::End(_)
        ) {
            reached_end = true;
        }
    }
    if reached_end {
        Ok(())
    } else {
        Err(ArtifactError(
            "guest artifact has no complete WebAssembly module".to_owned(),
        ))
    }
}

fn has_wasm_bindgen_descriptors(bytes: &[u8]) -> Result<bool, ArtifactError> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(imports) = payload.map_err(|error| {
            ArtifactError(format!("guest artifact is not valid WebAssembly: {error}"))
        })? {
            for import in imports {
                let import = import.map_err(|error| {
                    ArtifactError(format!("guest artifact has an invalid import: {error}"))
                })?;
                if import.module == "__wbindgen_placeholder__"
                    && import.name == "__wbindgen_describe"
                    && matches!(import.ty, TypeRef::Func(_))
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn process_wasm_bindgen_descriptors(
    artifact: &Path,
    max_module_bytes: usize,
) -> Result<Vec<u8>, ArtifactError> {
    let mut bindgen = Bindgen::new();
    bindgen.input_path(artifact);
    bindgen.emit_start(false);
    bindgen
        .web(true)
        .map_err(|error| ArtifactError(format!("failed to configure guest post-processing: {error}")))?;
    bindgen.typescript(false);
    let mut output = bindgen
        .generate_output()
        .map_err(|error| ArtifactError(format!("failed to process guest WebAssembly descriptors: {error}")))?;
    localize_wasm_bindgen_runtime(output.wasm_mut())?;
    let bytes = output.wasm_mut().emit_wasm();
    if bytes.is_empty() || bytes.len() > max_module_bytes {
        return Err(ArtifactError(format!(
            "processed guest artifact is {} bytes; expected 1..={max_module_bytes}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn localize_wasm_bindgen_runtime(module: &mut Module) -> Result<(), ArtifactError> {
    let generated_exports = module.exports.iter()
        .filter(|export| matches!(
            export.name.as_str(),
            "__abort_handler" | "__instance_terminated" | "__wbindgen_externrefs" | "__wbindgen_start"
        ))
        .map(|export| export.id())
        .collect::<Vec<_>>();
    for export in generated_exports {
        module.exports.delete(export);
    }
    let imports = module.imports.iter().filter_map(|import| {
        let ImportKind::Function(function) = import.kind else { return None };
        let generated_module = import.module.starts_with("./") && import.module.ends_with("_bg.js");
        let trap = import.name.starts_with("__wbg___wbindgen_throw_");
        let initialize = import.name == "__wbindgen_init_externref_table";
        (generated_module && (trap || initialize)).then(|| (import.id(), function, trap))
    }).collect::<Vec<_>>();
    for (import, function, trap) in imports {
        let ty = module.funcs.get(function).ty();
        let params = module.types.get(ty).params().to_vec();
        let results = module.types.get(ty).results().to_vec();
        if !results.is_empty() {
            return Err(ArtifactError("wasm-bindgen runtime helper returned an unexpected value".to_owned()));
        }
        let args = params.iter().map(|ty| module.locals.add(*ty)).collect::<Vec<_>>();
        let mut builder = FunctionBuilder::new(&mut module.types, &params, &results);
        if trap {
            builder.func_body().unreachable();
        }
        let replacement = builder.finish(args, &mut module.funcs);
        let replacement_kind = std::mem::replace(
            &mut module.funcs.get_mut(replacement).kind,
            FunctionKind::Uninitialized(ty),
        );
        module.funcs.get_mut(function).kind = replacement_kind;
        module.funcs.delete(replacement);
        module.imports.delete(import);
    }
    walrus::passes::gc::run(module);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn build_plan_separates_guest_and_host_and_validates_only_guest_wasm() {
        let project = tempdir().unwrap();
        let manifest = project.path().join("Cargo.toml");
        let guest_target = project.path().join("target/aimer-hot-reload/guest");
        let plan = HotReloadBuildPlan::new(
            manifest.clone(),
            "app_wasm_guest",
            "app",
            guest_target.clone(),
        );

        let guest = plan.guest_command();
        assert_eq!(guest.program(), "cargo");
        assert_eq!(
            guest.arguments(),
            [
                "build",
                "--manifest-path",
                manifest.to_str().unwrap(),
                "--package",
                "app_wasm_guest",
                "--target",
                "wasm32-unknown-unknown",
                "--target-dir",
                guest_target.to_str().unwrap(),
                "--config",
                r#"target.wasm32-unknown-unknown.rustflags = ["--cfg","aimer_portable_guest","--check-cfg=cfg(aimer_portable_guest)"]"#,
            ]
        );
        let host = plan.host_command();
        assert_eq!(
            host.arguments(),
            [
                "build",
                "--manifest-path",
                manifest.to_str().unwrap(),
                "--package",
                "app",
                "--features",
                "aimer/wasm-hot-reload",
            ]
        );

        let artifact_dir = guest_target.join("wasm32-unknown-unknown/debug");
        fs::create_dir_all(&artifact_dir).unwrap();
        let artifact = artifact_dir.join("app_wasm_guest.wasm");
        fs::write(&artifact, b"\0asm\x01\0\0\0").unwrap();
        assert_eq!(
            load_guest_artifact(&artifact, plan.guest_target_dir(), 64).unwrap(),
            b"\0asm\x01\0\0\0"
        );

        let native = project.path().join("target/debug/app");
        fs::create_dir_all(native.parent().unwrap()).unwrap();
        fs::write(&native, b"native").unwrap();
        assert!(load_guest_artifact(&native, plan.guest_target_dir(), 64).is_err());
        fs::write(&artifact, b"not wasm").unwrap();
        assert!(load_guest_artifact(&artifact, plan.guest_target_dir(), 64).is_err());
    }

    #[test]
    fn generated_guest_manifest_does_not_replace_the_host_workspace_manifest() {
        let plan = HotReloadBuildPlan::new(
            PathBuf::from("/app/Cargo.toml"),
            "aimer_generated_guest",
            "app",
            PathBuf::from("/app/target/aimer-hot-reload/guest"),
        )
        .guest_manifest(PathBuf::from(
            "/app/target/aimer-hot-reload/generated-guest/Cargo.toml",
        ));

        assert_eq!(
            plan.guest_command().arguments()[2],
            "/app/target/aimer-hot-reload/generated-guest/Cargo.toml"
        );
        assert_eq!(plan.host_command().arguments()[2], "/app/Cargo.toml");
    }

    #[test]
    fn guest_compiler_remaps_shadow_sources_and_keeps_abort_fallback_explicit() {
        let plan = HotReloadBuildPlan::new(
            PathBuf::from("/app/Cargo.toml"),
            "aimer_generated_guest",
            "app",
            PathBuf::from("/app/target/aimer-hot-reload/guest"),
        )
        .guest_source_remap(
            "/app/target/aimer-hot-reload/application",
            "/app",
        );

        let config = plan.guest_command().arguments()[10].clone();
        assert!(
            !config.contains("panic=unwind"),
            "the stable wasm target must retain its abort-policy fallback: {config}"
        );
        assert!(
            config.contains("\"--remap-path-prefix\",\"/app/target/aimer-hot-reload/application=/app\""),
            "{config}"
        );
    }

    #[test]
    fn descriptor_processing_localizes_only_generated_wasm_bindgen_runtime_helpers() {
        let mut module = Module::default();
        let throw_type = module.types.add(&[walrus::ValType::I32, walrus::ValType::I32], &[]);
        let init_type = module.types.add(&[], &[]);
        let (throw, _) = module.add_import_func(
            "./guest_bg.js",
            "__wbg___wbindgen_throw_0123456789abcdef",
            throw_type,
        );
        let (initialize, _) = module.add_import_func(
            "./guest_bg.js",
            "__wbindgen_init_externref_table",
            init_type,
        );
        let (untrusted, retained) = module.add_import_func(
            "untrusted",
            "__wbindgen_init_externref_table",
            init_type,
        );
        module.exports.add("__abort_handler", throw);
        module.exports.add("aimer_build", throw);
        module.exports.add("untrusted_helper", untrusted);

        localize_wasm_bindgen_runtime(&mut module).unwrap();

        assert!(matches!(module.funcs.get(throw).kind, FunctionKind::Local(_)));
        let _ = initialize;
        assert_eq!(module.imports.iter().map(|import| import.id()).collect::<Vec<_>>(), [retained]);
        assert_eq!(
            module.exports.iter().map(|export| export.name.as_str()).collect::<Vec<_>>(),
            ["aimer_build", "untrusted_helper"]
        );
    }
}
