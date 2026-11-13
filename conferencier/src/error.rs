//! The [`Error`] type and the [`Result`] alias used throughout the crate.

use std::path::PathBuf;

use thiserror::Error;

/// Result type returned by conferencier operations.
///
/// Equivalent to `std::result::Result<T, conferencier::Error>`. Application code
/// and manual trait implementations can use this alias to propagate errors with `?`.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by [`ConfigStore`](crate::ConfigStore) and by derived
/// [`ConfigScope`](crate::ConfigScope) and [`ConfigTable`](crate::ConfigTable)
/// implementations.
///
/// `path` holds the formatted path of the table containing the key, such as
/// `App.upstream[1]`. For errors about a top-level entry that is not a table,
/// `path` is empty and `key` is the entry name.
///
/// # Inspecting errors
///
/// This enum is non-exhaustive: match the variants relevant to your application
/// and keep a fallback arm for future variants. [`std::error::Error::source`]
/// exposes the underlying I/O, TOML parsing or serialization error; field errors
/// carry their diagnostic context directly.
///
/// ```
/// use conferencier::{ConfigStore, Error};
///
/// let error = ConfigStore::new().get_integer("App", "port").unwrap_err();
/// match error {
///     Error::MissingKey { path, key } => {
///         assert_eq!((path.as_str(), key.as_str()), ("App", "port"));
///     }
///     other => panic!("unexpected error: {other}"),
/// }
/// ```
///
/// File methods attach the relevant path to [`Error::Io`]. Converting a plain
/// [`std::io::Error`] with `?` or `Error::from` leaves `path` as `None`; construct
/// the public `Io` variant to attach a path, as shown in [`ConfigStore::from_file`](crate::ConfigStore::from_file).
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
    ///
    /// The source contains the parser's location and diagnostic details.
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    /// The in-memory document could not be serialized to TOML.
    ///
    /// Serialization happens before file writes; this error leaves the file untouched.
    #[error("failed to serialize TOML: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// A required key is absent.
    #[error("missing key {}", display_key(.path, .key))]
    MissingKey {
        /// Formatted path of the table containing the key.
        path: String,
        /// The missing key.
        key: String,
    },
    /// A value has a different TOML type than the field expects.
    #[error(
        "expected {expected} at {} but found {found}",
        display_key(.path, .key)
    )]
    TypeMismatch {
        /// Formatted path of the table containing the key.
        path: String,
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
    #[error(
        "invalid value at {}: {message}",
        display_key(.path, .key)
    )]
    ValueParse {
        /// Formatted path of the table containing the key.
        path: String,
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

    /// Reports that `key` is required but absent from the table at `path`.
    ///
    /// Pass an already formatted diagnostic path, such as `App.server`. For a
    /// top-level entry itself, use an empty path and its name as the key. This
    /// constructs an error; it does not look in a store.
    pub fn missing_key(path: impl Into<String>, key: impl Into<String>) -> Self {
        Self::MissingKey {
            path: path.into(),
            key: key.into(),
        }
    }

    /// Reports a value whose TOML type differs from the expected type.
    ///
    /// `expected` and `found` are type labels such as `"integer"` or `"string"`;
    /// [`toml::Value::type_str`] supplies the latter. Pass a nested table's
    /// diagnostic path as `path` when appropriate.
    pub fn type_mismatch(
        path: impl Into<String>,
        key: impl Into<String>,
        expected: &'static str,
        found: &'static str,
    ) -> Self {
        Self::TypeMismatch {
            path: path.into(),
            key: key.into(),
            expected,
            found,
        }
    }

    /// Reports a value that cannot be converted despite having an acceptable TOML type.
    ///
    /// Use `message` to explain the reason, for example `"value out of range for u16"`.
    /// Both borrowed and owned strings are accepted for `path`, `key` and `message`;
    /// the path must already be formatted.
    pub fn value_parse(
        path: impl Into<String>,
        key: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ValueParse {
            path: path.into(),
            key: key.into(),
            message: message.into(),
        }
    }
}

fn display_key(path: &str, key: &str) -> String {
    crate::table_path::child_path(path, key)
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
    fn diagnostics_quote_final_keys_and_root_entries() {
        assert_eq!(
            Error::missing_key("App", "a.b").to_string(),
            "missing key App.\"a.b\""
        );
        assert_eq!(
            Error::type_mismatch("", "App.server", "table", "integer").to_string(),
            "expected table at \"App.server\" but found integer"
        );
        assert_eq!(Error::missing_key("", "").to_string(), "missing key \"\"");
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
