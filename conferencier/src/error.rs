//! The [`Error`] type and the [`Result`] alias used throughout the crate.

use std::fmt;
use std::io::ErrorKind;
use std::path::PathBuf;

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
/// and [`UnknownKey`](Self::UnknownKey) carry their diagnostic context directly.
/// For [`SaveFileCleanup`](Self::SaveFileCleanup), the source is the original
/// save error; the cleanup error and temporary path are available as fields.
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
/// When saving and cleanup both fail, inspect the original error through
/// `save_error`; its I/O kind and optional OS code remain available:
///
/// ```
/// use conferencier::Error;
/// use std::io::{Error as IoError, ErrorKind};
///
/// let error = Error::SaveFileCleanup {
///     save_error: Box::new(Error::Io {
///         path: Some("config.toml".into()),
///         source: IoError::new(ErrorKind::PermissionDenied, "save failed"),
///     }),
///     temporary_path: ".conferencier.123.0.tmp".into(),
///     cleanup_error: IoError::new(ErrorKind::PermissionDenied, "cleanup failed"),
/// };
///
/// if let Error::SaveFileCleanup { save_error, .. } = error {
///     if let Error::Io { source, .. } = save_error.as_ref() {
///         assert_eq!(source.kind(), ErrorKind::PermissionDenied);
///         if let Some(code) = source.raw_os_error() {
///             eprintln!("save failed with OS error {code}");
///         }
///     }
/// }
/// ```
///
/// File methods attach the relevant path to [`Error::Io`]. Converting a plain
/// [`std::io::Error`] with `?` or `Error::from` leaves `path` as `None`; construct
/// the public `Io` variant to attach a path, as shown in the
/// [guide](https://docs.rs/conferencier/latest/conferencier/guide/concepts/index.html#reading-a-file-before-the-runtime-starts).
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Reading or writing a file failed.
    Io {
        /// The file involved, if known.
        path: Option<PathBuf>,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// Saving failed and removing its temporary file also failed.
    ///
    /// The original save failure remains the error source. The cleanup failure
    /// and temporary path are available separately.
    SaveFileCleanup {
        /// The original save failure, including its path and underlying cause.
        save_error: Box<Error>,
        /// The temporary file path that cleanup attempted to remove.
        temporary_path: PathBuf,
        /// The failure to remove the temporary file.
        cleanup_error: std::io::Error,
    },
    /// The input is not a valid TOML document.
    ///
    /// The source contains the parser's location and diagnostic details.
    Parse(toml::de::Error),
    /// The in-memory document could not be serialized to TOML.
    ///
    /// Serialization happens before file writes; this error leaves the file untouched.
    Serialize(toml::ser::Error),
    /// A required key is absent.
    MissingKey {
        /// Formatted path of the table containing the key.
        path: String,
        /// The missing key, without path formatting.
        key: String,
    },
    /// A value has a different TOML type than the field expects.
    TypeMismatch {
        /// Formatted path of the table containing the key.
        path: String,
        /// The key holding the value, without path formatting.
        key: String,
        /// Expected TOML type, such as `integer` or `table`.
        expected: &'static str,
        /// TOML type found in the document.
        found: &'static str,
    },
    /// A value has an acceptable TOML type but cannot be converted, for example an
    /// integer out of range for the field type, an invalid datetime string or an
    /// array element of the wrong type.
    ValueParse {
        /// Formatted path of the table containing the key.
        path: String,
        /// The key holding the value, without path formatting.
        key: String,
        /// Description of the problem.
        message: String,
    },
    /// A strict struct found a key that no loaded field maps.
    UnknownKey {
        /// Formatted path of the table containing the key.
        path: String,
        /// The unknown TOML key, without formatting.
        key: String,
    },
}

impl Error {
    /// Creates a [`Error::Io`] with the provided optional path context.
    pub(crate) fn io_error(path: Option<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path, source }
    }

    pub(crate) fn with_save_file_cleanup(
        save_error: Self,
        temporary_path: PathBuf,
        cleanup_result: std::io::Result<()>,
    ) -> Self {
        match cleanup_result {
            Ok(()) => save_error,
            Err(error) if error.kind() == ErrorKind::NotFound => save_error,
            Err(cleanup_error) => Self::SaveFileCleanup {
                save_error: Box::new(save_error),
                temporary_path,
                cleanup_error,
            },
        }
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

    /// Reports an unknown key in a strict table.
    ///
    /// Pass an already formatted diagnostic path and the raw TOML key. This
    /// constructs an error; it does not inspect a table.
    pub fn unknown_key(path: impl Into<String>, key: impl Into<String>) -> Self {
        Self::UnknownKey {
            path: path.into(),
            key: key.into(),
        }
    }
}

fn display_key(path: &str, key: &str) -> String {
    crate::table_path::child_path(path, key)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "I/O error (path: {path:?}): {source}"),
            Self::SaveFileCleanup {
                save_error,
                temporary_path,
                cleanup_error,
            } => write!(
                f,
                "failed to save file: {save_error}; failed to remove temporary file {temporary_path:?}: {cleanup_error}"
            ),
            Self::Parse(source) => write!(f, "failed to parse TOML: {source}"),
            Self::Serialize(source) => write!(f, "failed to serialize TOML: {source}"),
            Self::MissingKey { path, key } => {
                write!(f, "missing key {}", display_key(path, key))
            }
            Self::TypeMismatch {
                path,
                key,
                expected,
                found,
            } => write!(
                f,
                "expected {expected} at {} but found {found}",
                display_key(path, key)
            ),
            Self::ValueParse { path, key, message } => {
                write!(f, "invalid value at {}: {message}", display_key(path, key))
            }
            Self::UnknownKey { path, key } => {
                write!(f, "unknown key {}", display_key(path, key))
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::SaveFileCleanup { save_error, .. } => Some(save_error.as_ref()),
            Self::Parse(source) => Some(source),
            Self::Serialize(source) => Some(source),
            Self::MissingKey { .. }
            | Self::TypeMismatch { .. }
            | Self::ValueParse { .. }
            | Self::UnknownKey { .. } => None,
        }
    }
}

impl From<std::io::Error> for Error {
    /// Converts a plain [`std::io::Error`] into [`Error::Io`] without path context.
    fn from(source: std::io::Error) -> Self {
        Self::Io { path: None, source }
    }
}

impl From<toml::de::Error> for Error {
    fn from(source: toml::de::Error) -> Self {
        Self::Parse(source)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(source: toml::ser::Error) -> Self {
        Self::Serialize(source)
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
        assert_eq!(
            Error::unknown_key("App", "a.b").to_string(),
            "unknown key App.\"a.b\""
        );
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
    fn successful_save_file_cleanup_returns_original_error() {
        let error = Error::io_error(
            Some(PathBuf::from("config.toml")),
            IoError::new(ErrorKind::PermissionDenied, "save failed"),
        );
        let error =
            Error::with_save_file_cleanup(error, PathBuf::from(".conferencier.123.0.tmp"), Ok(()));

        match error {
            Error::Io { path, source } => {
                assert_eq!(path, Some(PathBuf::from("config.toml")));
                assert_eq!(source.kind(), ErrorKind::PermissionDenied);
                assert_eq!(source.to_string(), "save failed");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn missing_save_file_cleanup_returns_original_error() {
        let error = Error::io_error(
            Some(PathBuf::from("config.toml")),
            IoError::new(ErrorKind::WriteZero, "save failed"),
        );
        let error = Error::with_save_file_cleanup(
            error,
            PathBuf::from(".conferencier.123.0.tmp"),
            Err(IoError::new(ErrorKind::NotFound, "already removed")),
        );

        match error {
            Error::Io { path, source } => {
                assert_eq!(path, Some(PathBuf::from("config.toml")));
                assert_eq!(source.kind(), ErrorKind::WriteZero);
                assert_eq!(source.to_string(), "save failed");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn failed_save_file_cleanup_preserves_both_errors() {
        let error = Error::io_error(
            Some(PathBuf::from("config.toml")),
            IoError::new(ErrorKind::PermissionDenied, "save failed"),
        );
        let error = Error::with_save_file_cleanup(
            error,
            PathBuf::from(".conferencier.123.0.tmp"),
            Err(IoError::new(ErrorKind::PermissionDenied, "cleanup failed")),
        );

        assert!(error.to_string().contains("config.toml"));
        assert!(error.to_string().contains("save failed"));
        assert!(error.to_string().contains(".conferencier.123.0.tmp"));
        assert!(error.to_string().contains("cleanup failed"));

        match &error {
            Error::SaveFileCleanup {
                save_error,
                temporary_path,
                cleanup_error,
            } => {
                assert_eq!(temporary_path, &PathBuf::from(".conferencier.123.0.tmp"));
                assert_eq!(cleanup_error.kind(), ErrorKind::PermissionDenied);
                match save_error.as_ref() {
                    Error::Io { path, source } => {
                        assert_eq!(path.as_deref(), Some(std::path::Path::new("config.toml")));
                        assert_eq!(source.kind(), ErrorKind::PermissionDenied);
                        assert_eq!(source.to_string(), "save failed");
                    }
                    other => panic!("unexpected save error: {other}"),
                }
            }
            other => panic!("unexpected error: {other}"),
        }

        let expected_source = match &error {
            Error::SaveFileCleanup { save_error, .. } => save_error.to_string(),
            other => panic!("unexpected error: {other}"),
        };
        let source = error.source().expect("save error is the error source");
        assert!(source.downcast_ref::<Error>().is_some());
        assert_eq!(source.to_string(), expected_source);
    }

    #[test]
    fn failed_save_file_cleanup_preserves_raw_os_code() {
        let error = Error::with_save_file_cleanup(
            Error::io_error(
                Some(PathBuf::from("config.toml")),
                IoError::from_raw_os_error(123),
            ),
            PathBuf::from(".conferencier.123.0.tmp"),
            Err(IoError::new(ErrorKind::PermissionDenied, "cleanup failed")),
        );

        match error {
            Error::SaveFileCleanup { save_error, .. } => match save_error.as_ref() {
                Error::Io { source, .. } => assert_eq!(source.raw_os_error(), Some(123)),
                other => panic!("unexpected save error: {other}"),
            },
            other => panic!("unexpected error: {other}"),
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
