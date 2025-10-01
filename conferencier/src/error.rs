use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ConferError>;

/// Errors that can be raised while interacting with a [`Confer`](crate::store::Confer) store.
#[derive(Debug, Error)]
pub enum ConferError {
    #[error("I/O error (path: {path:?}): {source}")]
    Io {
        path: Option<PathBuf>,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize TOML: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("missing key {section}.{key}")]
    MissingKey { section: String, key: String },
    #[error("expected {expected} at {section}.{key} but found {found}")]
    TypeMismatch {
        section: String,
        key: String,
        expected: &'static str,
        found: &'static str,
    },
    #[error("invalid value at {section}.{key}: {message}")]
    ValueParse {
        section: String,
        key: String,
        message: String,
    },
}

impl ConferError {
    /// Creates a [`ConferError::Io`] with the provided optional path context.
    pub(crate) fn io_error(path: Option<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path, source }
    }

    /// Convenience constructor for [`ConferError::MissingKey`].
    pub fn missing_key(section: impl Into<String>, key: impl Into<String>) -> Self {
        Self::MissingKey {
            section: section.into(),
            key: key.into(),
        }
    }

    /// Convenience constructor for [`ConferError::TypeMismatch`].
    pub fn type_mismatch(
        section: impl Into<String>,
        key: impl Into<String>,
        expected: &'static str,
        found: &'static str,
    ) -> Self {
        Self::TypeMismatch {
            section: section.into(),
            key: key.into(),
            expected,
            found,
        }
    }

    /// Convenience constructor for [`ConferError::ValueParse`].
    pub fn value_parse(
        section: impl Into<String>,
        key: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::value_parse_owned(section, key, message.into())
    }

    /// Variant of [`ConferError::value_parse`] that accepts an owned [`String`].
    pub fn value_parse_owned(
        section: impl Into<String>,
        key: impl Into<String>,
        message: String,
    ) -> Self {
        Self::ValueParse {
            section: section.into(),
            key: key.into(),
            message,
        }
    }
}

impl From<std::io::Error> for ConferError {
    /// Converts a plain [`std::io::Error`] into [`ConferError::Io`] without path context.
    fn from(source: std::io::Error) -> Self {
        Self::Io { path: None, source }
    }
}
