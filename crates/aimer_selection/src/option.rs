use std::error::Error;
use std::fmt::{Display, Formatter};

/// An option shared by radio groups, selects, and autocompletes.
///
/// `key` is the stable identity used by pointer and overlay adapters. Labels
/// are presentation text and may intentionally repeat.
#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceOption<T> {
    key: String,
    label: String,
    value: T,
    disabled: bool,
}

impl<T> ChoiceOption<T> {
    /// Creates an enabled option with a stable key, display label, and value.
    #[inline]
    pub fn new(key: impl Into<String>, label: impl Into<String>, value: T) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            value,
            disabled: false,
        }
    }

    /// Returns the stable option key.
    #[inline]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the display label. Labels do not have to be unique.
    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the caller-owned option value.
    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns whether this option cannot be activated.
    #[inline]
    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Returns a copy of this option with its disabled state set.
    #[inline]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// An invalid option collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionError {
    /// An option key was empty and could not provide stable identity.
    EmptyKey,
    /// Two options used the same stable key.
    DuplicateKey(String),
}

impl Display for OptionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyKey => formatter.write_str("choice option keys cannot be empty"),
            Self::DuplicateKey(key) => write!(formatter, "duplicate choice option key: {key}"),
        }
    }
}

impl Error for OptionError {}

pub(crate) fn validate_options<T>(options: &[ChoiceOption<T>]) -> Result<(), OptionError> {
    for (index, option) in options.iter().enumerate() {
        if option.key.is_empty() {
            return Err(OptionError::EmptyKey);
        }
        if options[..index]
            .iter()
            .any(|previous| previous.key == option.key)
        {
            return Err(OptionError::DuplicateKey(option.key.clone()));
        }
    }
    Ok(())
}
