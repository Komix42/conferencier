//! The [`Error`] type and the [`Result`] alias used throughout the crate.

use std::path::PathBuf;

use thiserror::Error;

/// Result type returned by conferencier operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the [`ConfigStore`](crate::store::ConfigStore) store and by derived
/// [`ConfigScope`](crate::ConfigScope) and [`ConfigTable`](crate::ConfigTable)
/// implementations.
///
/// For keys inside nested tables, `section` holds the path of the table, such as
/// `App.upstream[1]`. When a section exists but is not a table, `key` is `<section>`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// Reading or writing a file failed.
    #[error("I/O error (path: {path:?}): {source}")]
    Io {
        /// The file involved, if known.
        path: Option<PathBuf>,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The input is not a valid TOML document.
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    /// The store could not be serialized to TOML.
    #[error("failed to serialize TOML: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// A required key is absent.
    #[error("missing key {section}.{key}")]
    MissingKey {
        /// Section or table path containing the key.
        section: String,
        /// The missing key.
        key: String,
    },
    /// A value has a different TOML type than the field expects.
    #[error("expected {expected} at {section}.{key} but found {found}")]
    TypeMismatch {
        /// Section or table path containing the key.
        section: String,
        /// The key holding the value.
        key: String,
        /// Expected TOML type, such as `integer` or `table`.
        expected: &'static str,
        /// TOML type found in the document.
        found: &'static str,
    },
    /// A value has an acceptable TOML type but cannot be converted, for example an
    /// integer out of range for the field type, an invalid datetime string or an
    /// array element of the wrong type.
    #[error("invalid value at {section}.{key}: {message}")]
    ValueParse {
        /// Section or table path containing the key.
        section: String,
        /// The key holding the value.
        key: String,
        /// Description of the problem.
        message: String,
    },
}

impl Error {
    /// Creates a [`Error::Io`] with the provided optional path context.
    pub(crate) fn io_error(path: Option<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path, source }
    }

    /// Convenience constructor for [`Error::MissingKey`].
    pub fn missing_key(section: impl Into<String>, key: impl Into<String>) -> Self {
        Self::MissingKey {
            section: section.into(),
            key: key.into(),
        }
    }

    /// Convenience constructor for [`Error::TypeMismatch`].
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

    /// Convenience constructor for [`Error::ValueParse`].
    pub fn value_parse(
        section: impl Into<String>,
        key: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ValueParse {
            section: section.into(),
            key: key.into(),
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for Error {
    /// Converts a plain [`std::io::Error`] into [`Error::Io`] without path context.
    fn from(source: std::io::Error) -> Self {
        Self::Io { path: None, source }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::io::{Error as IoError, ErrorKind};

    use super::*;

    #[test]
    fn value_errors_preserve_messages_and_have_no_source() {
        let cases = [
            (Error::missing_key("App", "port"), "missing key App.port"),
            (
                Error::type_mismatch("App", "port", "integer", "string"),
                "expected integer at App.port but found string",
            ),
            (
                Error::value_parse("App", "port", "out of range"),
                "invalid value at App.port: out of range",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn io_errors_preserve_path_and_source() {
        for path in [None, Some(PathBuf::from("config.toml"))] {
            let source = IoError::new(ErrorKind::NotFound, "missing config");
            let expected = format!("I/O error (path: {path:?}): missing config");
            let error = match path {
                Some(path) => Error::io_error(Some(path), source),
                None => Error::from(source),
            };

            assert_eq!(error.to_string(), expected);
            let source = error.source().unwrap().downcast_ref::<IoError>().unwrap();
            assert_eq!(source.kind(), ErrorKind::NotFound);
            assert_eq!(source.to_string(), "missing config");
        }
    }

    #[test]
    fn parse_error_conversion_preserves_source() {
        let source = toml::from_str::<toml::Table>("[broken").unwrap_err();
        let message = source.to_string();
        let error = Error::from(source);

        assert!(matches!(error, Error::Parse(_)));
        assert_eq!(
            error.to_string(),
            format!("failed to parse TOML: {message}")
        );
        assert_eq!(error.source().unwrap().to_string(), message);
    }

    #[test]
    fn serialize_error_conversion_preserves_source() {
        let source = toml::to_string(&"not a table").unwrap_err();
        let message = source.to_string();
        let error = Error::from(source);

        assert!(matches!(error, Error::Serialize(_)));
        assert_eq!(
            error.to_string(),
            format!("failed to serialize TOML: {message}")
        );
        assert_eq!(error.source().unwrap().to_string(), message);
    }
}
