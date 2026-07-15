use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// File name of the Aimer project manifest.
pub const MANIFEST_FILE: &str = "aimer.toml";

/// Fallback package name used when neither `aimer.toml` nor `Cargo.toml`
/// provide one.
pub const DEFAULT_PACKAGE_NAME: &str = "aimer_template";

/// The `aimer.toml` project manifest. Persisted by `create` and consumed by
/// `run`/`build` so they don't have to re-parse `Cargo.toml` ad hoc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AimerManifest {
    #[serde(alias = "application")]
    pub package: PackageMeta,
    #[serde(default)]
    pub build: Option<BuildMeta>,
    #[serde(default)]
    pub assets: Option<AssetsMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub group: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildMeta {
    #[serde(default)]
    pub default_target: Option<String>,
}

/// The `[assets]` section of `aimer.toml`. Lists the files that should be
/// bundled into each platform package and made available to the running app
/// through [`ImageSource::Asset`](../../aimer_assets) and friends.
///
/// Paths are relative to the project root and are preserved verbatim when the
/// files are copied into a platform's asset root, so the same string used here
/// is also the runtime lookup key.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetsMeta {
    /// Files to bundle, e.g. `"assets/logo.png"`.
    #[serde(default)]
    pub files: Vec<String>,
}

impl AimerManifest {
    /// Build a manifest from the metadata collected during `create`.
    pub fn new(name: &str, version: &str, description: &str, author: &str, group: &str) -> Self {
        Self {
            package: PackageMeta {
                name: name.to_string(),
                version: version.to_string(),
                description: description.to_string(),
                author: author.to_string(),
                group: group.to_string(),
            },
            build: None,
            assets: None,
        }
    }

    /// The list of registered asset files, or an empty slice when the
    /// `[assets]` section is missing or empty.
    pub fn asset_files(&self) -> &[String] {
        self.assets
            .as_ref()
            .map(|a| a.files.as_slice())
            .unwrap_or(&[])
    }

    /// Load the manifest from `<dir>/aimer.toml`. Returns `Ok(None)` when the
    /// file does not exist, and an error only when it exists but cannot be
    /// read or parsed.
    pub fn load_from(dir: &Path) -> anyhow::Result<Option<Self>> {
        let path = dir.join(MANIFEST_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let manifest: AimerManifest =
            toml::from_str(&contents).with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(manifest))
    }

    /// Load the manifest from the current working directory.
    pub fn load() -> anyhow::Result<Option<Self>> {
        Self::load_from(Path::new("."))
    }

    /// Serialize and write the manifest to `<dir>/aimer.toml`.
    pub fn write_to(&self, dir: &Path) -> anyhow::Result<()> {
        let path = dir.join(MANIFEST_FILE);
        let contents = toml::to_string_pretty(self).context("serializing aimer.toml")?;
        std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Default build target declared in the manifest, if any.
    pub fn default_target(&self) -> Option<&str> {
        self.build
            .as_ref()
            .and_then(|b| b.default_target.as_deref())
    }
}

/// Resolve the project package name for the given project directory.
///
/// Prefers `aimer.toml`, falls back to a real TOML parse of `Cargo.toml`, and
/// finally to [`DEFAULT_PACKAGE_NAME`].
pub fn resolve_package_name(dir: &Path) -> String {
    if let Ok(Some(manifest)) = AimerManifest::load_from(dir)
        && !manifest.package.name.is_empty()
    {
        return manifest.package.name;
    }
    parse_cargo_package_name(dir).unwrap_or_else(|| DEFAULT_PACKAGE_NAME.to_string())
}

/// Parse the `[package].name` field from `<dir>/Cargo.toml` using a real TOML
/// parser instead of brittle string splitting.
pub fn parse_cargo_package_name(dir: &Path) -> Option<String> {
    #[derive(Deserialize)]
    struct CargoToml {
        package: Option<CargoPackage>,
    }
    #[derive(Deserialize)]
    struct CargoPackage {
        name: Option<String>,
    }

    let contents = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let parsed: CargoToml = toml::from_str(&contents).ok()?;
    parsed.package.and_then(|p| p.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let manifest =
            AimerManifest::new("my_app", "0.2.0", "a cool app", "alice", "com.example.myapp");
        manifest
            .write_to(dir.path())
            .unwrap();

        let loaded = AimerManifest::load_from(dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.package.name, "my_app");
        assert_eq!(loaded.package.version, "0.2.0");
        assert_eq!(loaded.package.description, "a cool app");
        assert_eq!(loaded.package.author, "alice");
        assert_eq!(loaded.package.group, "com.example.myapp");
    }

    #[test]
    fn load_from_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            AimerManifest::load_from(dir.path())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn default_target_is_read() {
        let mut manifest = AimerManifest::new("app", "0.1.0", "", "", "com.example.app");
        manifest.build = Some(BuildMeta { default_target: Some("web".to_string()) });
        let dir = tempfile::tempdir().unwrap();
        manifest
            .write_to(dir.path())
            .unwrap();
        let loaded = AimerManifest::load_from(dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.default_target(), Some("web"));
    }

    #[test]
    fn resolve_package_name_prefers_manifest() {
        let dir = tempfile::tempdir().unwrap();
        AimerManifest::new("from_manifest", "0.1.0", "", "", "com.example.from_manifest")
            .write_to(dir.path())
            .unwrap();
        assert_eq!(resolve_package_name(dir.path()), "from_manifest");
    }

    #[test]
    fn resolve_package_name_falls_back_to_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"from_cargo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        assert_eq!(resolve_package_name(dir.path()), "from_cargo");
    }

    #[test]
    fn resolve_package_name_defaults_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resolve_package_name(dir.path()), DEFAULT_PACKAGE_NAME);
    }

    #[test]
    fn parses_application_alias_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(MANIFEST_FILE),
            "[application]\nname = \"Jaime\"\nversion = \"0.0.1\"\ngroup = \"com.example.jaime\"\n\n[assets]\nfiles = [\"assets/logo.png\", \"assets/fonts/Inter.ttf\"]\n",
        )
        .unwrap();

        let loaded = AimerManifest::load_from(dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(loaded.package.name, "Jaime");
        assert_eq!(loaded.package.version, "0.0.1");
        assert_eq!(
            loaded.asset_files(),
            ["assets/logo.png".to_string(), "assets/fonts/Inter.ttf".to_string()]
        );
    }

    #[test]
    fn asset_files_empty_without_section() {
        let manifest = AimerManifest::new("app", "0.1.0", "", "", "com.example.app");
        assert!(manifest.asset_files().is_empty());
    }

    #[test]
    fn parse_cargo_package_name_works() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"pkg\"\n").unwrap();
        assert_eq!(parse_cargo_package_name(dir.path()), Some("pkg".to_string()));
    }
}
