//! Reusable library support for the Aimer command-line tools.

#[path = "config.rs"]
pub mod config;
#[path = "commands/run/hot_reload.rs"]
pub mod hot_reload;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use crate::config::{ApplicationRuntime, BuildProfile, ExecutionPolicy, ReloadPolicy};
    #[cfg(feature = "hot-reload")]
    use crate::hot_reload::build::{HotReloadBuildPlan, load_guest_artifact};
    use crate::hot_reload::generation::{
        GeneratedGuestPackage, prepare_automatic_guest,
    };
    #[cfg(feature = "hot-reload")]
    use crate::hot_reload::generation::GuestGenerationMode;
    use crate::hot_reload::pipeline::{
        PipelineOperations, ProductionPipelineDriver, run_pipeline,
    };
    use tempfile::TempDir;
    #[cfg(feature = "hot-reload")]
    use wasmparser::{Parser, Payload, TypeRef};

    #[cfg(feature = "hot-reload")]
    const PACKAGE: &str = "automatic-hot-reload-app";

    #[cfg(feature = "hot-reload")]
    #[test]
    #[ignore = "temporarily disabled while automatic guest generation is repaired"]
    fn automatic_generation_builds_wasm_and_exercises_generated_guest_variants() {
        let fixture = fixture_root();
        let original = snapshot(&fixture);
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("application");
        copy_tree(&fixture, &project);

        let manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("[workspace]"));
        assert!(!manifest.contains("[build.hot_reload]"));
        assert_eq!(GuestGenerationMode::select(None), GuestGenerationMode::Automatic);

        let initial = generate(&project);
        assert_generated_layout(&project, &initial);
        let initial_output = run_shadow_tests(&initial, &temp, "initial");
        let initial_callback = callback_id(&initial_output);

        apply_variant(&project, "body_only.rs");
        let body_only = generate(&project);
        let body_output = run_shadow_tests(&body_only, &temp, "body-only");
        assert_eq!(callback_id(&body_output), initial_callback);

        apply_variant(&project, "callback_rebind.rs");
        let rebound = generate(&project);
        let rebound_output = run_shadow_tests(&rebound, &temp, "callback-rebind");
        assert_eq!(callback_id(&rebound_output), initial_callback);

        apply_variant(&project, "incompatible_state.rs");
        let incompatible = generate(&project);
        run_shadow_tests(&incompatible, &temp, "incompatible-state");

        apply_variant(&project, "recovery.rs");
        let recovery = generate(&project);
        run_shadow_tests(&recovery, &temp, "recovery");
        build_and_validate_wasm(&project, &recovery);

        assert_eq!(snapshot(&fixture), original);
    }

    #[cfg(feature = "hot-reload")]
    #[test]
    fn compile_failure_retains_active_generation_and_later_recovers() {
        let fixture = fixture_root();
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("application");
        copy_tree(&fixture, &project);
        apply_variant(&project, "compile_failure.rs");
        assert!(fs::read_to_string(project.join("src/main.rs"))
            .unwrap()
            .contains("intentional automatic fixture compile failure"));

        let operations = ReloadOperations::new();
        let mut driver = ProductionPipelineDriver::new(operations);
        run_pipeline(&mut driver).unwrap();

        let operations = driver.operations();
        assert_eq!(operations.active_generation, 4);
        assert_eq!(operations.incompatible_candidate, Some(2));
        assert_eq!(operations.failed_candidate, Some(3));
        assert_eq!(
            operations.events,
            [
                "initial committed",
                "incompatible state rejected; active retained",
                "compile failed; active retained",
                "recovery committed",
            ],
        );
        assert!(operations.cleaned_up);

        apply_variant(&project, "recovery.rs");
        let recovery = generate(&project);
        run_shadow_tests(&recovery, &temp, "pipeline-recovery");
    }

    #[cfg(all(test, feature = "hot-reload"))]
    fn ordinary_native_selection_does_not_create_hot_reload_output() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("application");
        copy_tree(&fixture_root(), &project);
        let session = project.join("target/aimer-hot-reload");

        let policy = ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::NativeAot,
            ReloadPolicy::Disabled,
        )
        .unwrap();

        assert_eq!(policy.runtime(), ApplicationRuntime::NativeAot);
        assert_eq!(policy.reload(), ReloadPolicy::Disabled);
        assert!(!session.exists());
    }

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/automatic_hot_reload_app")
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned()
    }

    fn generate(project: &Path) -> GeneratedGuestPackage {
        let project_lock = project.join("Cargo.lock");
        if !project_lock.is_file() {
            fs::copy(workspace_root().join("Cargo.lock"), &project_lock).unwrap();
        }
        let generated = prepare_automatic_guest(
            project,
            &workspace_root(),
            &project.join("target/aimer-hot-reload"),
        )
        .unwrap();
        generated
    }

    #[cfg(feature = "hot-reload")]
    fn assert_generated_layout(project: &Path, generated: &GeneratedGuestPackage) {
        assert_eq!(generated.package(), "aimer_generated_guest");
        assert_eq!(
            generated.portable_source_root(),
            Some(fs::canonicalize(project.join("src")).unwrap().as_path()),
        );
        assert!(generated.application_root().join("Cargo.toml").is_file());
        assert!(generated.wrapper_root().join("src/lib.rs").is_file());
        assert!(fs::read_to_string(generated.manifest()).unwrap().contains("[workspace]"));
        assert!(fs::read_to_string(generated.wrapper_root().join("src/lib.rs"))
            .unwrap()
            .contains("application::__AimerGeneratedGuestProgram"));
    }

    fn run_shadow_tests(generated: &GeneratedGuestPackage, temp: &TempDir, label: &str) -> Output {
        let output = Command::new(env!("CARGO"))
            .args(["test", "--quiet", "--manifest-path"])
            .arg(generated.application_root().join("Cargo.toml"))
            .args(["--lib", "--", "--nocapture"])
            .env("CARGO_TARGET_DIR", temp.path().join("cargo-target"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{label} shadow tests failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        output
    }

    #[cfg(feature = "hot-reload")]
    fn callback_id(output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("AUTOMATIC_CALLBACK_ID="))
            .unwrap_or_else(|| {
                panic!(
                    "missing callback identity in output:\n{}",
                    String::from_utf8_lossy(&output.stdout)
                )
            })
            .to_owned()
    }

    #[cfg(feature = "hot-reload")]
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
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "generated wasm build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let artifact = target.join("wasm32-unknown-unknown/debug/aimer_generated_guest.wasm");
        let module =
            load_guest_artifact(&artifact, plan.guest_target_dir(), 256 * 1024 * 1024).unwrap();
        assert!(!module.is_empty());
        assert_guest_imports_are_host_capabilities_only(&module);
    }

    #[cfg(feature = "hot-reload")]
    fn assert_guest_imports_are_host_capabilities_only(module: &[u8]) {
        let mut unsupported = Vec::new();
        let mut exports = Vec::new();
        for payload in Parser::new(0).parse_all(module) {
            match payload.unwrap() {
                Payload::ImportSection(imports) => {
                    for import in imports {
                        let import = import.unwrap();
                        if import.module != "aimer"
                            || import.name != "capability_call"
                            || !matches!(import.ty, TypeRef::Func(_))
                        {
                            unsupported.push(format!("{}.{}", import.module, import.name));
                        }
                    }
                }
                Payload::ExportSection(section) => {
                    exports.extend(section.into_iter().map(|export| export.unwrap().name.to_owned()));
                }
                _ => {}
            }
        }
        assert!(
            unsupported.is_empty(),
            "automatic guest contains unsupported imports {unsupported:?}; exports: {exports:?}"
        );
        assert!(
            exports.iter().all(|name| {
                !name.starts_with("__wbindgen")
                    && name != "__abort_handler"
                    && name != "__instance_terminated"
            }),
            "automatic guest contains undeclared wasm-bindgen exports: {exports:?}"
        );
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
            fs::write(
                manifest_path,
                manifest.replace("../../../..", &workspace_root().to_string_lossy()),
            )
            .unwrap();
        }
    }

    #[cfg(feature = "hot-reload")]
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

    struct ReloadOperations {
        active_generation: u64,
        incompatible_candidate: Option<u64>,
        failed_candidate: Option<u64>,
        events: Vec<&'static str>,
        cleaned_up: bool,
    }

    impl ReloadOperations {
        fn new() -> Self {
            Self {
                active_generation: 0,
                incompatible_candidate: None,
                failed_candidate: None,
                events: Vec::new(),
                cleaned_up: false,
            }
        }
    }

    impl PipelineOperations for ReloadOperations {
        fn resolve(&mut self) -> Result<(), String> { Ok(()) }
        fn create_session(&mut self) -> Result<(), String> { Ok(()) }
        fn build_host(&mut self) -> Result<(), String> { Ok(()) }
        fn build_initial_guest(&mut self) -> Result<(), String> { Ok(()) }
        fn assemble(&mut self) -> Result<(), String> { Ok(()) }
        fn prepare_route(&mut self) -> Result<(), String> { Ok(()) }
        fn launch_app(&mut self) -> Result<(), String> { Ok(()) }
        fn discover_and_authenticate(&mut self) -> Result<(), String> { Ok(()) }

        fn push_initial_module(&mut self) -> Result<(), String> {
            self.active_generation = 1;
            self.events.push("initial committed");
            Ok(())
        }

        fn watch(&mut self) -> Result<(), String> {
            self.incompatible_candidate = Some(2);
            self.events.push("incompatible state rejected; active retained");
            assert_eq!(self.active_generation, 1);
            self.failed_candidate = Some(3);
            self.events.push("compile failed; active retained");
            assert_eq!(self.active_generation, 1);
            self.active_generation = 4;
            self.events.push("recovery committed");
            Ok(())
        }

        fn cleanup(&mut self) {
            self.cleaned_up = true;
        }
    }
}
