use thiserror::Error;

/// Domain-specific errors for the Aimer CLI.
///
/// These are typed, matchable errors for predictable failure cases. For
/// general propagation with context, prefer `anyhow::Result` at the command
/// boundary and convert into these variants only when a caller may want to
/// match on the failure.
#[derive(Debug, Error)]
pub enum AimerError {
    /// The supplied project name is not a valid directory name.
    #[error("invalid project name '{0}': {1}")]
    InvalidProjectName(String, String),

    /// A required external toolchain could not be found on the PATH.
    #[error("required tool not found: {0}")]
    MissingToolchain(String),

    /// The target string could not be parsed into a known `Targets` variant.
    #[error("unknown target: {0}")]
    UnknownTarget(String),

    /// A requested device could not be located among the available devices.
    #[error("device not found: {0}")]
    DeviceNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_project_name_message() {
        let e = AimerError::InvalidProjectName("a b".to_string(), "contains spaces".to_string());
        assert_eq!(e.to_string(), "invalid project name 'a b': contains spaces");
    }

    #[test]
    fn missing_toolchain_message() {
        let e = AimerError::MissingToolchain("adb".to_string());
        assert_eq!(e.to_string(), "required tool not found: adb");
    }

    #[test]
    fn unknown_target_message() {
        let e = AimerError::UnknownTarget("foo".to_string());
        assert_eq!(e.to_string(), "unknown target: foo");
    }

    #[test]
    fn device_not_found_message() {
        let e = AimerError::DeviceNotFound("xyz".to_string());
        assert_eq!(e.to_string(), "device not found: xyz");
    }
}
