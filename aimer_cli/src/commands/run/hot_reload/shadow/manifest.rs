use std::fs;
use std::path::{Path, PathBuf};

use super::{CanonicalGuestConfig, RootDiscovery, ShadowError, ShadowErrorKind};

pub(crate) struct RewrittenManifest {
    pub value: toml::Value,
    pub package: String,
    pub bytes: Vec<u8>,
}

pub(crate) fn rewrite(project_root: &Path) -> Result<RewrittenManifest, ShadowError> {
    let manifest_path = project_root.join("Cargo.toml");
    let source = fs::read_to_string(&manifest_path).map_err(|error| manifest_io(&manifest_path, error))?;
    let mut value = toml::from_str::<toml::Value>(&source).map_err(|error| {
        ShadowError::new(
            ShadowErrorKind::Manifest,
            format!("failed to parse {}: {error}", manifest_path.display()),
        )
    })?;
    let package = value
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| ShadowError::new(ShadowErrorKind::Manifest, "Cargo package.name is required"))?
        .to_owned();
    let workspace = find_workspace(project_root)?;
    materialize_package_fields(&mut value, workspace.as_ref())?;
    rewrite_dependency_tables(&mut value, project_root, workspace.as_ref())?;

    let resolver = value
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|workspace| workspace.get("resolver"))
        .cloned()
        .or_else(|| workspace.as_ref().and_then(|workspace| {
            workspace.value.get("workspace")?.get("resolver").cloned()
        }));
    let mut standalone = toml::map::Map::new();
    if let Some(resolver) = resolver {
        standalone.insert("resolver".to_owned(), resolver);
    }
    value.as_table_mut().expect("manifest root is a table").insert(
        "workspace".to_owned(),
        toml::Value::Table(standalone),
    );
    let mut text = toml::to_string(&value).map_err(|error| {
        ShadowError::new(ShadowErrorKind::Manifest, format!("failed to emit standalone manifest: {error}"))
    })?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(RewrittenManifest { value, package, bytes: text.into_bytes() })
}

pub(crate) fn enable_guest(
    rewritten: &mut RewrittenManifest,
    project_root: &Path,
    discovery: &RootDiscovery,
    config: &CanonicalGuestConfig,
) -> Result<(), ShadowError> {
    let crate_root = crate_root_for(project_root, &rewritten.value, discovery)?;
    let relative_root = crate_root.strip_prefix(project_root).map_err(|_| {
        ShadowError::new(ShadowErrorKind::PathEscape, "guest crate root escapes the application")
    })?;
    let root = rewritten.value.as_table_mut().expect("manifest root is a table");
    let package = root.get_mut("package")
        .and_then(toml::Value::as_table_mut)
        .expect("validated package table exists");
    package.insert("autobins".to_owned(), toml::Value::Boolean(false));
    root.remove("bin");

    let mut library = toml::map::Map::new();
    library.insert(
        "path".to_owned(),
        toml::Value::String(relative_root.to_string_lossy().replace('\\', "/")),
    );
    library.insert(
        "crate-type".to_owned(),
        toml::Value::Array(vec![
            toml::Value::String("cdylib".to_owned()),
            toml::Value::String("rlib".to_owned()),
        ]),
    );
    root.insert("lib".to_owned(), toml::Value::Table(library));
    enable_portable_guest_feature(root)?;

    let dependencies = root.entry("dependencies")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| ShadowError::new(ShadowErrorKind::Manifest, "Cargo dependencies must be a table"))?;
    let existing_aimer = dependencies.remove("aimer");
    dependencies.insert(
        "aimer".to_owned(),
        local_dependency(&config.aimer_root, true, existing_aimer.as_ref()),
    );
    dependencies.insert(
        "aimer_wasm_guest".to_owned(),
        local_dependency(&config.wasm_guest_root, false, None),
    );
    emit(rewritten)
}

fn enable_portable_guest_feature(
    root: &mut toml::map::Map<String, toml::Value>,
) -> Result<(), ShadowError> {
    let features = root
        .entry("features")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or_else(|| {
            ShadowError::new(
                ShadowErrorKind::Manifest,
                "Cargo features must be a table",
            )
        })?;
    features
        .entry("portable-guest")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let default = features
        .entry("default")
        .or_insert_with(|| toml::Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| {
            ShadowError::new(
                ShadowErrorKind::Manifest,
                "Cargo feature default must be an array",
            )
        })?;
    if !default
        .iter()
        .any(|feature| feature.as_str() == Some("portable-guest"))
    {
        default.push(toml::Value::String("portable-guest".to_owned()));
    }
    Ok(())
}

fn crate_root_for(
    project_root: &Path,
    _manifest: &toml::Value,
    discovery: &RootDiscovery,
) -> Result<PathBuf, ShadowError> {
    let crate_source = discovery.crate_source();
    if !crate_source.starts_with(project_root) {
        return Err(ShadowError::new(
            ShadowErrorKind::PathEscape,
            "guest crate root escapes the application",
        ));
    }
    Ok(crate_source.to_owned())
}

fn local_dependency(
    path: &Path,
    portable_guest: bool,
    existing: Option<&toml::Value>,
) -> toml::Value {
    let mut dependency = toml::map::Map::new();
    dependency.insert("path".to_owned(), toml::Value::String(path.to_string_lossy().into_owned()));
    let mut features = existing
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("features"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if portable_guest && !features.iter().any(|feature| feature.as_str() == Some("portable-guest")) {
        features.push(toml::Value::String("portable-guest".to_owned()));
    }
    if !features.is_empty() {
        dependency.insert(
            "features".to_owned(),
            toml::Value::Array(features),
        );
    }
    if let Some(default_features) = existing
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("default-features"))
        .and_then(toml::Value::as_bool)
    {
        dependency.insert("default-features".to_owned(), toml::Value::Boolean(default_features));
    }
    toml::Value::Table(dependency)
}

fn emit(rewritten: &mut RewrittenManifest) -> Result<(), ShadowError> {
    let mut text = toml::to_string(&rewritten.value).map_err(|error| {
        ShadowError::new(ShadowErrorKind::Manifest, format!("failed to emit guest manifest: {error}"))
    })?;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    rewritten.bytes = text.into_bytes();
    Ok(())
}

struct WorkspaceManifest {
    root: PathBuf,
    value: toml::Value,
}

fn find_workspace(project_root: &Path) -> Result<Option<WorkspaceManifest>, ShadowError> {
    for directory in project_root.ancestors() {
        let path = directory.join("Cargo.toml");
        if !path.is_file() {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|error| manifest_io(&path, error))?;
        let value = toml::from_str::<toml::Value>(&source).map_err(|error| {
            ShadowError::new(ShadowErrorKind::Manifest, format!("failed to parse {}: {error}", path.display()))
        })?;
        if value.get("workspace").is_some() {
            return Ok(Some(WorkspaceManifest {
                root: fs::canonicalize(directory).map_err(|error| manifest_io(directory, error))?,
                value,
            }));
        }
    }
    Ok(None)
}

fn materialize_package_fields(
    manifest: &mut toml::Value,
    workspace: Option<&WorkspaceManifest>,
) -> Result<(), ShadowError> {
    let Some(package) = manifest.get_mut("package").and_then(toml::Value::as_table_mut) else {
        return Err(ShadowError::new(ShadowErrorKind::Manifest, "Cargo [package] table is required"));
    };
    let inherited = package
        .iter()
        .filter_map(|(name, value)| {
            match value.as_table()?.get("workspace")?.as_bool()? {
                true => Some(name.clone()),
                false => None,
            }
        })
        .collect::<Vec<_>>();
    for name in inherited {
        let replacement = workspace
            .and_then(|workspace| workspace.value.get("workspace"))
            .and_then(|workspace| workspace.get("package"))
            .and_then(|package| package.get(&name))
            .cloned()
            .ok_or_else(|| {
                ShadowError::new(ShadowErrorKind::Manifest, format!("workspace.package.{name} is not defined"))
            })?;
        package.insert(name, replacement);
    }
    Ok(())
}

fn rewrite_dependency_tables(
    manifest: &mut toml::Value,
    project_root: &Path,
    workspace: Option<&WorkspaceManifest>,
) -> Result<(), ShadowError> {
    let root = manifest.as_table_mut().expect("manifest root is a table");
    for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = root.get_mut(name).and_then(toml::Value::as_table_mut) {
            rewrite_dependencies(table, project_root, workspace)?;
        }
    }
    if let Some(targets) = root.get_mut("target").and_then(toml::Value::as_table_mut) {
        for target in targets.iter_mut().filter_map(|(_, value)| value.as_table_mut()) {
            for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(table) = target.get_mut(name).and_then(toml::Value::as_table_mut) {
                    rewrite_dependencies(table, project_root, workspace)?;
                }
            }
        }
    }
    Ok(())
}

fn rewrite_dependencies(
    dependencies: &mut toml::map::Map<String, toml::Value>,
    project_root: &Path,
    workspace: Option<&WorkspaceManifest>,
) -> Result<(), ShadowError> {
    for (name, dependency) in dependencies {
        let inherited = dependency
            .as_table()
            .and_then(|table| table.get("workspace"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        let mut path_base = project_root;
        if inherited {
            let workspace = workspace.ok_or_else(|| {
                ShadowError::new(ShadowErrorKind::Manifest, format!("dependency {name} inherits without a workspace"))
            })?;
            let base = workspace
                .value
                .get("workspace")
                .and_then(|workspace| workspace.get("dependencies"))
                .and_then(|dependencies| dependencies.get(name))
                .cloned()
                .ok_or_else(|| {
                    ShadowError::new(ShadowErrorKind::Manifest, format!("workspace dependency {name} is not defined"))
                })?;
            let overrides = dependency.as_table().expect("inherited dependency is a table").clone();
            *dependency = merge_dependency(base, overrides);
            path_base = &workspace.root;
        }
        if let Some(table) = dependency.as_table_mut() {
            table.remove("workspace");
            if let Some(relative) = table.get("path").and_then(toml::Value::as_str) {
                let path = Path::new(relative);
                if path.is_relative() {
                    let resolved = fs::canonicalize(path_base.join(path)).map_err(|error| {
                        manifest_io(&path_base.join(path), error)
                    })?;
                    table.insert("path".to_owned(), toml::Value::String(resolved.to_string_lossy().into_owned()));
                }
            }
        }
    }
    Ok(())
}

fn merge_dependency(base: toml::Value, overrides: toml::map::Map<String, toml::Value>) -> toml::Value {
    let mut table = match base {
        toml::Value::String(version) => {
            let mut table = toml::map::Map::new();
            table.insert("version".to_owned(), toml::Value::String(version));
            table
        }
        toml::Value::Table(table) => table,
        other => return other,
    };
    for (name, value) in overrides {
        if name != "workspace" {
            if name == "features"
                && let (Some(inherited), Some(additional)) = (
                    table.get_mut("features").and_then(toml::Value::as_array_mut),
                    value.as_array(),
                )
            {
                for feature in additional {
                    if !inherited.contains(feature) {
                        inherited.push(feature.clone());
                    }
                }
                continue;
            }
            table.insert(name, value);
        }
    }
    toml::Value::Table(table)
}

fn manifest_io(path: &Path, error: std::io::Error) -> ShadowError {
    ShadowError::new(
        ShadowErrorKind::Manifest,
        format!("failed to read manifest path {}: {error}", path.display()),
    )
}
