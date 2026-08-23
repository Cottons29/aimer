use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

pub(crate) const SOURCE_MAP_ENV: &str = "AIMER_CAPABILITY_PACKAGE_SOURCE_MAP";
const SOURCE_MAP_VERSION: u32 = 1;

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
    target_directory: String,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    manifest_path: String,
    source: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct PackageSourceMap {
    version: u32,
    target_directory: String,
    packages: Vec<PackageSource>,
}

impl PackageSourceMap {
    fn from_metadata_json(metadata: &str) -> Result<Self, String> {
        let metadata: CargoMetadata = serde_json::from_str(metadata)
            .map_err(|error| format!("failed to decode Cargo metadata: {error}"))?;
        let mut packages: Vec<_> = metadata
            .packages
            .into_iter()
            .map(|package| PackageSource {
                name: package.name,
                manifest_path: package.manifest_path,
                source: package.source.map(|source| canonical_source(&source)),
            })
            .collect();
        packages.sort_unstable_by(|left, right| {
            left.manifest_path
                .cmp(&right.manifest_path)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(Self {
            version: SOURCE_MAP_VERSION,
            target_directory: metadata.target_directory,
            packages,
        })
    }

    fn to_toml(&self) -> Result<String, String> {
        toml::to_string(self)
            .map_err(|error| format!("failed to encode capability package sources: {error}"))
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct PackageSource {
    name: String,
    manifest_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

/// Resolves Cargo's package graph and writes the source map consumed by
/// `#[aimer::capability]` expansions in the following application build.
pub(crate) fn prepare_source_map() -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .map_err(|error| format!("failed to run `cargo metadata`: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`cargo metadata` failed while resolving capability package sources: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata = String::from_utf8(output.stdout)
        .map_err(|_| "Cargo metadata output was not UTF-8".to_string())?;
    let map = PackageSourceMap::from_metadata_json(&metadata)?;
    write_source_map(&map)
}

/// Attaches the current workspace's canonical package-source map to one Cargo
/// application build command.
pub(crate) fn configure_command(command: &mut Command) -> Result<(), String> {
    command.env(SOURCE_MAP_ENV, prepare_source_map()?);
    Ok(())
}

fn write_source_map(map: &PackageSourceMap) -> Result<PathBuf, String> {
    let directory = Path::new(&map.target_directory).join("aimer");
    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "failed to create capability source-map directory `{}`: {error}",
            directory.display()
        )
    })?;
    let path = directory.join("capability-package-sources-v1.toml");
    fs::write(&path, map.to_toml()?).map_err(|error| {
        format!(
            "failed to write capability package sources `{}`: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

fn canonical_source(source: &str) -> String {
    if let Some(git) = source.strip_prefix("git+") {
        let end = git
            .find(['?', '#'])
            .unwrap_or(git.len());
        format!("git+{}", &git[..end])
    } else {
        source.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const METADATA: &str = r#"{
        "packages": [
            {
                "name": "registry-sdk",
                "manifest_path": "/cargo/registry/registry-sdk/Cargo.toml",
                "source": "registry+https://github.com/rust-lang/crates.io-index"
            },
            {
                "name": "alternate-sdk",
                "manifest_path": "/cargo/registry/alternate-sdk/Cargo.toml",
                "source": "registry+https://packages.example/index"
            },
            {
                "name": "git-sdk",
                "manifest_path": "/cargo/git/git-sdk/Cargo.toml",
                "source": "git+https://example.com/sdk.git?rev=main#0123456789abcdef"
            },
            {
                "name": "workspace-sdk",
                "manifest_path": "/workspace/sdk/Cargo.toml",
                "source": null
            }
        ],
        "target_directory": "/workspace/target"
    }"#;

    #[test]
    fn cargo_metadata_becomes_a_versioned_package_source_map() {
        let map = PackageSourceMap::from_metadata_json(METADATA).unwrap();

        assert_eq!(map.version, 1);
        assert_eq!(map.packages.len(), 4);
        assert_eq!(
            map.packages[0],
            PackageSource {
                name: "git-sdk".to_string(),
                manifest_path: "/cargo/git/git-sdk/Cargo.toml".to_string(),
                source: Some("git+https://example.com/sdk.git".to_string()),
            }
        );
        assert_eq!(map.target_directory, "/workspace/target");
    }

    #[test]
    fn package_source_map_serialization_is_deterministic() {
        let first = PackageSourceMap::from_metadata_json(METADATA)
            .unwrap()
            .to_toml()
            .unwrap();
        let second = PackageSourceMap::from_metadata_json(METADATA)
            .unwrap()
            .to_toml()
            .unwrap();

        assert_eq!(first, second);
        assert!(first.contains("version = 1"));
        assert!(first.contains("registry+https://github.com/rust-lang/crates.io-index"));
        assert!(!first.contains("0123456789abcdef"));
    }
}