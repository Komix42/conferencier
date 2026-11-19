//! The [`Error`] type and the [`Result`] alias used throughout the crate.

use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Result type returned by conferencier operations.
///
/// Equivalent to `std::result::Result<T, conferencier::Error>`. Application code
/// and manual trait implementations can use this alias to propagate errors with `?`.
pub type Result<T> = std::result::Result<T, Error>;

/// The high-level file-operation phase that failed.
///
/// These variants identify phases rather than individual system calls. A phase
/// such as preparing the temporary file may perform no system call on a platform
/// that does not need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IoOperation {
    /// Read a configuration file.
    Read,
    /// Inspect and validate the save destination before writing.
    InspectTarget,
    /// Create a new temporary file for the save.
    CreateTemporary,
    /// Prepare the temporary file before writing, if the platform requires it.
    PrepareTemporary,
    /// Write the serialized configuration to the temporary file.
    WriteTemporary,
    /// Replace the save destination with the completed temporary file.
    ReplaceTarget,
}

impl IoOperation {
    fn description(self) -> &'static str {
        match self {
            Self::Read => "read file",
            Self::InspectTarget => "inspect save target",
            Self::CreateTemporary => "create temporary file",
            Self::PrepareTemporary => "prepare temporary file",
            Self::WriteTemporary => "write temporary file",
            Self::ReplaceTarget => "replace target file",
        }
    }
}

/// Errors returned by [`ConfigStore`](crate::ConfigStore) and by derived
/// [`ConfigScope`](crate::ConfigScope) and [`ConfigTable`](crate::ConfigTable)
/// implementations.
///
/// In key errors, `table_path` holds the formatted path of the table containing
/// the key, such as `App.upstream[1]`. For errors about a top-level entry that is
/// not a table, `table_path` is empty and `key` is the entry name.
///
/// # Inspecting errors
///
/// This enum is non-exhaustive: match the variants relevant to your application
/// and keep a fallback arm for future variants. Key-error variants are also
/// individually non-exhaustive: patterns outside this crate must include `..`,
/// and the constructors such as [`Error::missing_key_error`] create them.
/// [`std::error::Error::source`] exposes wrapped errors without duplicating them
/// in this error's `Display`; field errors and [`UnknownKey`](Self::UnknownKey)
/// carry their diagnostic context directly.
/// For [`SaveFileCleanup`](Self::SaveFileCleanup), the source is the original
/// save error; the target path, cleanup error and temporary path are also
/// available as fields.
///
/// ```
/// use conferencier::{ConfigStore, Error};
///
/// let error = ConfigStore::new().get_integer("App", "port").unwrap_err();
/// match error {
///     Error::MissingKey { table_path, key, .. } => {
///         assert_eq!((table_path.as_str(), key.as_str()), ("App", "port"));
///     }
///     other => panic!("unexpected error: {other}"),
/// }
/// ```
///
/// When saving and cleanup both fail, inspect the original error through
/// `save_error`; its I/O kind and optional OS code remain available. Such errors
/// are produced by `save_file`; callers cannot construct them directly.
///
/// ```
/// use conferencier::Error;
///
/// fn report_save_failure(error: &Error) {
///     if let Error::SaveFileCleanup { save_error, .. } = error {
///         if let Error::Io { source, .. } = save_error.as_ref() {
///             eprintln!("save failed with {:?}; source: {}", source.kind(), error);
///         }
///     }
/// }
/// ```
///
/// File methods attach operation and path context to [`Error::Io`]. Converting a
/// plain [`std::io::Error`] with `?` or `Error::from` leaves the paths unset; use
/// [`Error::io_error`] or [`Error::io_error_with_operation`] to supply context,
/// as shown in the
/// [guide](https://docs.rs/conferencier/latest/conferencier/guide/concepts/index.html#reading-a-file-before-the-runtime-starts).
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Reading or writing a file failed.
    #[non_exhaustive]
    Io {
        /// The file directly involved in the failed operation, if known.
        path: Option<PathBuf>,
        /// The phase of the file operation, if known.
        operation: Option<IoOperation>,
        /// The original save destination, if this error came from `save_file`.
        target_path: Option<PathBuf>,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// Saving failed and removing its temporary file also failed.
    ///
    /// The original save failure remains the error source. The cleanup failure
    /// is shown in this error's `Display`, not in the source chain; the temporary
    /// path and cleanup error are also available separately.
    #[non_exhaustive]
    SaveFileCleanup {
        /// The destination path of the save.
        target_path: PathBuf,
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
    #[non_exhaustive]
    Parse {
        /// The file that was parsed, if known.
        path: Option<PathBuf>,
        /// The parser's diagnostic, with its location in the document.
        source: toml::de::Error,
    },
    /// The in-memory document could not be serialized to TOML.
    ///
    /// Serialization happens before file writes; this error leaves the file untouched.
    #[non_exhaustive]
    Serialize {
        /// The destination of the save, if any.
        target_path: Option<PathBuf>,
        /// The serializer's error.
        source: toml::ser::Error,
    },
    /// The Tokio blocking task for a save could not be joined.
    #[non_exhaustive]
    SaveFileTaskJoin {
        /// The destination path of the save.
        target_path: PathBuf,
        /// The failure to join the Tokio blocking task.
        source: tokio::task::JoinError,
    },
    /// A required key is absent.
    #[non_exhaustive]
    MissingKey {
        /// Formatted path of the table containing the key.
        table_path: String,
        /// The missing key, without path formatting.
        key: String,
    },
    /// A value has a different TOML type than the field expects.
    #[non_exhaustive]
    TypeMismatch {
        /// Formatted path of the table containing the key.
        table_path: String,
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
    #[non_exhaustive]
    ValueParse {
        /// Formatted path of the table containing the key.
        table_path: String,
        /// The key holding the value, without path formatting.
        key: String,
        /// Description of the problem.
        message: String,
    },
    /// A strict struct found a key that no loaded field maps.
    #[non_exhaustive]
    UnknownKey {
        /// Formatted path of the table containing the key.
        table_path: String,
        /// The unknown TOML key, without formatting.
        key: String,
    },
}

impl Error {
    /// Creates an I/O error with optional path context and no operation.
    ///
    /// `target_path` is unset; adjust the public fields if additional context is needed.
    pub fn io_error(path: Option<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path,
            operation: None,
            target_path: None,
            source,
        }
    }

    /// Creates an I/O error with an operation and optional path context.
    ///
    /// `target_path` is unset; adjust the public fields if additional context is needed.
    pub fn io_error_with_operation(
        path: Option<PathBuf>,
        operation: IoOperation,
        source: std::io::Error,
    ) -> Self {
        Self::Io {
            path,
            operation: Some(operation),
            target_path: None,
            source,
        }
    }

    /// Creates a TOML parse error with optional file context.
    pub fn parse_error(path: Option<PathBuf>, source: toml::de::Error) -> Self {
        Self::Parse { path, source }
    }

    /// Creates a TOML serialization error with optional save-target context.
    pub fn serialize_error(target_path: Option<PathBuf>, source: toml::ser::Error) -> Self {
        Self::Serialize {
            target_path,
            source,
        }
    }

    pub(crate) fn with_save_target(self, path: &Path) -> Self {
        match self {
            Self::Io {
                path: file_path,
                operation,
                source,
                ..
            } => Self::Io {
                path: file_path,
                operation,
                target_path: Some(path.to_path_buf()),
                source,
            },
            Self::Serialize { source, .. } => Self::Serialize {
                target_path: Some(path.to_path_buf()),
                source,
            },
            Self::SaveFileCleanup {
                save_error,
                temporary_path,
                cleanup_error,
                ..
            } => Self::SaveFileCleanup {
                target_path: path.to_path_buf(),
                save_error: Box::new(save_error.with_save_target(path)),
                temporary_path,
                cleanup_error,
            },
            other => other,
        }
    }

    pub(crate) fn with_parse_path(mut self, path: &Path) -> Self {
        if let Self::Parse {
            path: error_path, ..
        } = &mut self
        {
            *error_path = Some(path.to_path_buf());
        }
        self
    }

    pub(crate) fn with_save_file_cleanup(
        target_path: PathBuf,
        save_error: Self,
        temporary_path: PathBuf,
        cleanup_result: std::io::Result<()>,
    ) -> Self {
        match cleanup_result {
            Ok(()) => save_error,
            Err(error) if error.kind() == ErrorKind::NotFound => save_error,
            Err(cleanup_error) => Self::SaveFileCleanup {
                target_path,
                save_error: Box::new(save_error),
                temporary_path,
                cleanup_error,
            },
        }
    }

    /// Reports that `key` is required but absent from the table at `table_path`.
    ///
    /// Pass an already formatted diagnostic path, such as `App.server`. For a
    /// top-level entry itself, use an empty table path and its name as the key. This
    /// constructs an error; it does not look in a store.
    pub fn missing_key_error(table_path: impl Into<String>, key: impl Into<String>) -> Self {
        Self::MissingKey {
            table_path: table_path.into(),
            key: key.into(),
        }
    }

    /// Reports a value whose TOML type differs from the expected type.
    ///
    /// `expected` and `found` are type labels such as `"integer"` or `"string"`;
    /// [`toml::Value::type_str`] supplies the latter. Pass a nested table's
    /// diagnostic path as `table_path` when appropriate.
    pub fn type_mismatch_error(
        table_path: impl Into<String>,
        key: impl Into<String>,
        expected: &'static str,
        found: &'static str,
    ) -> Self {
        Self::TypeMismatch {
            table_path: table_path.into(),
            key: key.into(),
            expected,
            found,
        }
    }

    /// Reports a value that cannot be converted despite having an acceptable TOML type.
    ///
    /// Use `message` to explain the reason, for example `"value out of range for u16"`.
    /// Both borrowed and owned strings are accepted for `table_path`, `key` and
    /// `message`; the table path must already be formatted.
    pub fn value_parse_error(
        table_path: impl Into<String>,
        key: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::ValueParse {
            table_path: table_path.into(),
            key: key.into(),
            message: message.into(),
        }
    }

    /// Reports an unknown key in a strict table.
    ///
    /// Pass an already formatted diagnostic path and the raw TOML key. This
    /// constructs an error; it does not inspect a table.
    pub fn unknown_key_error(table_path: impl Into<String>, key: impl Into<String>) -> Self {
        Self::UnknownKey {
            table_path: table_path.into(),
            key: key.into(),
        }
    }
}

fn escaped_path(path: &Path) -> String {
    let mut escaped = String::new();
    for character in path.to_string_lossy().chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{2028}' => escaped.push_str("\\u{2028}"),
            '\u{2029}' => escaped.push_str("\\u{2029}"),
            character if character.is_control() => escaped.extend(character.escape_debug()),
            character => escaped.push(character),
        }
    }
    escaped
}

fn write_quoted_path(f: &mut fmt::Formatter<'_>, path: &Path) -> fmt::Result {
    write!(f, "\"{}\"", escaped_path(path))
}

fn display_io(
    f: &mut fmt::Formatter<'_>,
    path: Option<&Path>,
    operation: Option<IoOperation>,
    target_path: Option<&Path>,
) -> fmt::Result {
    if let Some(operation) = operation {
        let description = operation.description();
        if let Some(path) = path {
            write!(f, "failed to {description} ")?;
            write_quoted_path(f, path)?;
        } else if matches!(
            operation,
            IoOperation::InspectTarget | IoOperation::ReplaceTarget
        ) {
            if let Some(target_path) = target_path {
                write!(f, "failed to {description} ")?;
                write_quoted_path(f, target_path)?;
            } else {
                write!(f, "failed to {description}")?;
            }
        } else {
            write!(f, "failed to {description}")?;
        }
        if matches!(
            operation,
            IoOperation::CreateTemporary
                | IoOperation::PrepareTemporary
                | IoOperation::WriteTemporary
        ) {
            if let Some(target_path) = target_path {
                write!(f, " for ")?;
                write_quoted_path(f, target_path)?;
            }
        }
        return Ok(());
    }

    if let Some(target_path) = target_path {
        f.write_str("failed to save ")?;
        write_quoted_path(f, target_path)
    } else if let Some(path) = path {
        f.write_str("I/O error on ")?;
        write_quoted_path(f, path)
    } else {
        f.write_str("I/O error")
    }
}

fn display_key(table_path: &str, key: &str) -> String {
    crate::table_path::child_path(table_path, key)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                path,
                operation,
                target_path,
                ..
            } => display_io(f, path.as_deref(), *operation, target_path.as_deref()),
            Self::SaveFileCleanup {
                target_path,
                temporary_path,
                cleanup_error,
                ..
            } => {
                f.write_str("failed to save ")?;
                write_quoted_path(f, target_path)?;
                f.write_str("; removing temporary file ")?;
                write_quoted_path(f, temporary_path)?;
                write!(f, " also failed ({cleanup_error})")
            }
            Self::SaveFileTaskJoin { target_path, .. } => {
                write!(f, "failed to save ")?;
                write_quoted_path(f, target_path)
            }
            Self::Parse { path, .. } => match path {
                Some(path) => {
                    write!(f, "failed to parse TOML file ")?;
                    write_quoted_path(f, path)
                }
                None => f.write_str("failed to parse TOML"),
            },
            Self::Serialize { target_path, .. } => match target_path {
                Some(path) => {
                    write!(f, "failed to serialize TOML for ")?;
                    write_quoted_path(f, path)
                }
                None => f.write_str("failed to serialize TOML"),
            },
            Self::MissingKey { table_path, key } => {
                write!(f, "missing key {}", display_key(table_path, key))
            }
            Self::TypeMismatch {
                table_path,
                key,
                expected,
                found,
            } => write!(
                f,
                "expected {expected} at {} but found {found}",
                display_key(table_path, key)
            ),
            Self::ValueParse {
                table_path,
                key,
                message,
            } => {
                write!(
                    f,
                    "invalid value at {}: {message}",
                    display_key(table_path, key)
                )
            }
            Self::UnknownKey { table_path, key } => {
                write!(f, "unknown key {}", display_key(table_path, key))
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::SaveFileCleanup { save_error, .. } => Some(save_error.as_ref()),
            Self::SaveFileTaskJoin { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Serialize { source, .. } => Some(source),
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
        Self::io_error(None, source)
    }
}

impl From<toml::de::Error> for Error {
    fn from(source: toml::de::Error) -> Self {
        Self::parse_error(None, source)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(source: toml::ser::Error) -> Self {
        Self::serialize_error(None, source)
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
            (
                Error::missing_key_error("App", "port"),
                "missing key App.port",
            ),
            (
                Error::type_mismatch_error("App", "port", "integer", "string"),
                "expected integer at App.port but found string",
            ),
            (
                Error::value_parse_error("App", "port", "out of range"),
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
            Error::missing_key_error("App", "a.b").to_string(),
            "missing key App.\"a.b\""
        );
        assert_eq!(
            Error::type_mismatch_error("", "App.server", "table", "integer").to_string(),
            "expected table at \"App.server\" but found integer"
        );
        assert_eq!(
            Error::missing_key_error("", "").to_string(),
            "missing key \"\""
        );
        assert_eq!(
            Error::unknown_key_error("App", "a.b").to_string(),
            "unknown key App.\"a.b\""
        );
    }

    #[test]
    fn io_error_constructors_preserve_context_and_source() {
        for path in [None, Some(PathBuf::from("config.toml"))] {
            let source = IoError::new(ErrorKind::NotFound, "missing config");
            let error = Error::io_error(path.clone(), source);
            assert_eq!(
                error.to_string(),
                match path.as_deref() {
                    Some(path) => format!("I/O error on \"{}\"", escaped_path(path)),
                    None => "I/O error".to_owned(),
                }
            );
            assert!(matches!(
                &error,
                Error::Io {
                    path: error_path,
                    operation: None,
                    target_path: None,
                    ..
                } if error_path == &path
            ));
            let source = error.source().unwrap().downcast_ref::<IoError>().unwrap();
            assert_eq!(source.kind(), ErrorKind::NotFound);
            assert_eq!(source.to_string(), "missing config");
        }

        let error = Error::io_error_with_operation(
            Some("temp.toml".into()),
            IoOperation::WriteTemporary,
            IoError::new(ErrorKind::WriteZero, "write failed"),
        );
        assert_eq!(
            error.to_string(),
            "failed to write temporary file \"temp.toml\""
        );
        assert!(matches!(
            error,
            Error::Io {
                operation: Some(IoOperation::WriteTemporary),
                target_path: None,
                ..
            }
        ));
    }

    #[test]
    fn successful_save_file_cleanup_returns_original_error() {
        let error = Error::io_error(
            Some(PathBuf::from("config.toml")),
            IoError::new(ErrorKind::PermissionDenied, "save failed"),
        )
        .with_save_target(Path::new("config.toml"));
        let error = Error::with_save_file_cleanup(
            PathBuf::from("config.toml"),
            error,
            PathBuf::from(".conferencier.123.0.tmp"),
            Ok(()),
        );

        match error {
            Error::Io { path, source, .. } => {
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
        )
        .with_save_target(Path::new("config.toml"));
        let error = Error::with_save_file_cleanup(
            PathBuf::from("config.toml"),
            error,
            PathBuf::from(".conferencier.123.0.tmp"),
            Err(IoError::new(ErrorKind::NotFound, "already removed")),
        );

        match error {
            Error::Io { path, source, .. } => {
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
        )
        .with_save_target(Path::new("config.toml"));
        let error = Error::with_save_file_cleanup(
            PathBuf::from("config.toml"),
            error,
            PathBuf::from(".conferencier.123.0.tmp"),
            Err(IoError::new(ErrorKind::PermissionDenied, "cleanup failed")),
        );

        assert_eq!(
            error.to_string(),
            "failed to save \"config.toml\"; removing temporary file \".conferencier.123.0.tmp\" also failed (cleanup failed)"
        );

        match &error {
            Error::SaveFileCleanup {
                target_path,
                save_error,
                temporary_path,
                cleanup_error,
                ..
            } => {
                assert_eq!(target_path, &PathBuf::from("config.toml"));
                assert_eq!(temporary_path, &PathBuf::from(".conferencier.123.0.tmp"));
                assert_eq!(cleanup_error.kind(), ErrorKind::PermissionDenied);
                match save_error.as_ref() {
                    Error::Io { path, source, .. } => {
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
            Error::SaveFileCleanup { save_error, .. } => {
                assert_eq!(save_error.to_string(), "failed to save \"config.toml\"");
                save_error.to_string()
            }
            other => panic!("unexpected error: {other}"),
        };
        let source = error.source().expect("save error is the error source");
        assert!(source.downcast_ref::<Error>().is_some());
        assert_eq!(source.to_string(), expected_source);
    }

    #[test]
    fn failed_save_file_cleanup_preserves_raw_os_code() {
        let error = Error::with_save_file_cleanup(
            PathBuf::from("config.toml"),
            Error::io_error(
                Some(PathBuf::from("config.toml")),
                IoError::from_raw_os_error(123),
            )
            .with_save_target(Path::new("config.toml")),
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

        assert!(matches!(error, Error::Parse { path: None, .. }));
        assert_eq!(error.to_string(), "failed to parse TOML");
        assert_eq!(error.source().unwrap().to_string(), message);
    }

    #[test]
    fn serialize_error_conversion_preserves_source() {
        let source = toml::to_string(&"not a table").unwrap_err();
        let message = source.to_string();
        let error = Error::from(source);

        assert!(matches!(
            error,
            Error::Serialize {
                target_path: None,
                ..
            }
        ));
        assert_eq!(error.to_string(), "failed to serialize TOML");
        assert_eq!(error.source().unwrap().to_string(), message);
    }

    #[test]
    fn file_error_display_escapes_paths_and_keeps_context() {
        let source = IoError::new(ErrorKind::PermissionDenied, "denied");
        let error = Error::io_error_with_operation(
            Some("line\n\"quoted\"\\path".into()),
            IoOperation::WriteTemporary,
            source,
        )
        .with_save_target(Path::new("target\u{2028}.toml"));
        assert_eq!(
            error.to_string(),
            "failed to write temporary file \"line\\n\\\"quoted\\\"\\\\path\" for \"target\\u{2028}.toml\""
        );

        let error = Error::io_error_with_operation(
            None,
            IoOperation::InspectTarget,
            IoError::new(ErrorKind::InvalidInput, "directory"),
        )
        .with_save_target(Path::new("target.toml"));
        assert_eq!(
            error.to_string(),
            "failed to inspect save target \"target.toml\""
        );
    }

    #[test]
    fn parse_and_serialize_constructors_keep_source_out_of_display() {
        let parse_source = toml::from_str::<toml::Table>("[broken").unwrap_err();
        let parse_error = Error::parse_error(Some("config.toml".into()), parse_source);
        assert_eq!(
            parse_error.to_string(),
            "failed to parse TOML file \"config.toml\""
        );
        assert!(parse_error.source().is_some());

        let serialize_source = toml::to_string(&"not a table").unwrap_err();
        let serialize_error = Error::serialize_error(Some("config.toml".into()), serialize_source);
        assert_eq!(
            serialize_error.to_string(),
            "failed to serialize TOML for \"config.toml\""
        );
        assert!(serialize_error.source().is_some());
    }

    #[tokio::test]
    async fn save_task_join_error_displays_target_and_exposes_join_source() {
        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let source = task.await.unwrap_err();
        let source_message = source.to_string();
        let error = Error::SaveFileTaskJoin {
            target_path: "config.toml".into(),
            source,
        };

        assert_eq!(error.to_string(), "failed to save \"config.toml\"");
        assert_eq!(error.source().unwrap().to_string(), source_message);
    }
}
