use std::fs;
use std::path::{Component, Path, PathBuf};

use super::{ShadowError, ShadowErrorKind, ShadowLimits};

pub(crate) struct ValidatedMirror {
    pub source_root: PathBuf,
    pub output_root: PathBuf,
}

pub(crate) fn validate_and_copy(
    source: &Path,
    output: &Path,
    limits: ShadowLimits,
    manifest_bytes: &[u8],
) -> Result<ValidatedMirror, ShadowError> {
    let source_root = fs::canonicalize(source).map_err(|error| io_error(source, error))?;
    if !source_root.is_dir() {
        return Err(ShadowError::new(
            ShadowErrorKind::PathEscape,
            format!("shadow source is not a directory: {}", source.display()),
        ));
    }
    let output_root = canonical_destination(output)?;
    if output_root.starts_with(&source_root) || source_root.starts_with(&output_root) {
        return Err(ShadowError::new(
            ShadowErrorKind::OutputRecursion,
            format!(
                "shadow output {} overlaps source {}",
                output_root.display(),
                source_root.display()
            ),
        ));
    }

    let mut files = Vec::new();
    let mut directories = vec![PathBuf::new()];
    let mut pending = vec![PathBuf::new()];
    let mut total_bytes = 0_u64;
    while let Some(relative_directory) = pending.pop() {
        let directory = source_root.join(&relative_directory);
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| io_error(&directory, error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| io_error(&directory, error))?;
        entries.sort_unstable_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(&source_root).expect("entry is below source").to_owned();
            let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
            if metadata.file_type().is_symlink() {
                return Err(ShadowError::new(
                    ShadowErrorKind::PathEscape,
                    format!("symbolic links are not allowed in shadow input: {}", path.display()),
                ));
            }
            if metadata.is_dir() {
                if is_generated_directory(&relative) {
                    continue;
                }
                directories.push(relative.clone());
                pending.push(relative);
                continue;
            }
            if !metadata.is_file() {
                return Err(ShadowError::new(
                    ShadowErrorKind::PathEscape,
                    format!("unsupported filesystem entry in shadow input: {}", path.display()),
                ));
            }
            let canonical = fs::canonicalize(&path).map_err(|error| io_error(&path, error))?;
            if !canonical.starts_with(&source_root) {
                return Err(ShadowError::new(
                    ShadowErrorKind::PathEscape,
                    format!("source path escapes project root: {}", path.display()),
                ));
            }
            let copied_bytes = if relative == Path::new("Cargo.toml") {
                manifest_bytes.len() as u64
            } else {
                metadata.len()
            };
            if copied_bytes > limits.max_file_bytes {
                return Err(limit_error(format!(
                    "file {} contains {} bytes, exceeding the {} byte limit",
                    relative.display(),
                    copied_bytes,
                    limits.max_file_bytes
                )));
            }
            if files.len() == limits.max_files {
                return Err(limit_error(format!("shadow contains more than {} files", limits.max_files)));
            }
            total_bytes = total_bytes.checked_add(copied_bytes).ok_or_else(|| {
                limit_error("shadow byte count overflowed".to_owned())
            })?;
            if total_bytes > limits.max_total_bytes {
                return Err(limit_error(format!(
                    "shadow contains {total_bytes} bytes, exceeding the {} byte limit",
                    limits.max_total_bytes
                )));
            }
            files.push(relative);
        }
    }

    directories.sort_unstable();
    files.sort_unstable();
    if output_root.exists() {
        let metadata = fs::symlink_metadata(&output_root).map_err(|error| io_error(&output_root, error))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ShadowError::new(
                ShadowErrorKind::PathEscape,
                format!("shadow output is not a regular directory: {}", output_root.display()),
            ));
        }
        fs::remove_dir_all(&output_root).map_err(|error| io_error(&output_root, error))?;
    }
    fs::create_dir_all(&output_root).map_err(|error| io_error(&output_root, error))?;
    for relative in directories.into_iter().skip(1) {
        let destination = output_root.join(relative);
        fs::create_dir(&destination).map_err(|error| io_error(&destination, error))?;
    }
    for relative in files {
        let source_file = source_root.join(&relative);
        let destination = output_root.join(relative);
        if source_file.file_name().is_some_and(|name| name == "Cargo.toml")
            && source_file.parent() == Some(source_root.as_path())
        {
            fs::write(&destination, manifest_bytes).map_err(|error| io_error(&destination, error))?;
        } else {
            fs::copy(&source_file, &destination).map_err(|error| io_error(&destination, error))?;
        }
    }

    Ok(ValidatedMirror { source_root, output_root })
}

fn is_generated_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| matches!(name.to_str(), Some("build" | "builds" | "target")))
}

fn canonical_destination(path: &Path) -> Result<PathBuf, ShadowError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
        if metadata.file_type().is_symlink() {
            return Err(ShadowError::new(
                ShadowErrorKind::PathEscape,
                format!("shadow output may not be a symbolic link: {}", path.display()),
            ));
        }
        return fs::canonicalize(path).map_err(|error| io_error(path, error));
    }
    let mut missing = Vec::new();
    let mut ancestor = path;
    while !ancestor.exists() {
        let name = ancestor.file_name().ok_or_else(|| {
            ShadowError::new(ShadowErrorKind::PathEscape, format!("invalid output path: {}", path.display()))
        })?;
        if name == ".." || name == "." {
            return Err(ShadowError::new(
                ShadowErrorKind::PathEscape,
                format!("output path contains unresolved traversal: {}", path.display()),
            ));
        }
        missing.push(name.to_owned());
        ancestor = ancestor.parent().ok_or_else(|| {
            ShadowError::new(ShadowErrorKind::PathEscape, format!("invalid output path: {}", path.display()))
        })?;
    }
    let mut destination = fs::canonicalize(ancestor).map_err(|error| io_error(ancestor, error))?;
    for component in missing.into_iter().rev() {
        if Path::new(&component).components().any(|part| !matches!(part, Component::Normal(_))) {
            return Err(ShadowError::new(ShadowErrorKind::PathEscape, "invalid output path component"));
        }
        destination.push(component);
    }
    Ok(destination)
}

fn io_error(path: &Path, error: std::io::Error) -> ShadowError {
    ShadowError::new(ShadowErrorKind::Io, format!("filesystem operation failed for {}: {error}", path.display()))
}

fn limit_error(message: String) -> ShadowError {
    ShadowError::new(ShadowErrorKind::LimitExceeded, message)
}