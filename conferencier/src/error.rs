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
/// [guide](https://docs.rs/conferencier/%5E0.1.0-beta.1/conferencier/guide/concepts/index.html#reading-a-file-before-the-runtime-starts).
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

#[cfg(doctest)]
/// Each non-exhaustive variant rejects external construction and full patterns.
///
/// ```compile_fail
/// # fn construct(source: std::io::Error) {
/// let _ = conferencier::Error::Io {
///     path: None,
///     operation: None,
///     target_path: None,
///     source,
/// };
/// # }
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::Io { path, operation, target_path, source } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// # fn construct(save_error: conferencier::Error, cleanup_error: std::io::Error) {
/// let _ = conferencier::Error::SaveFileCleanup {
///     target_path: "config.toml".into(),
///     save_error: Box::new(save_error),
///     temporary_path: "temp.toml".into(),
///     cleanup_error,
/// };
/// # }
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::SaveFileCleanup {
///     target_path, save_error, temporary_path, cleanup_error
/// } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// # fn construct(source: conferencier::toml::de::Error) {
/// let _ = conferencier::Error::Parse { path: None, source };
/// # }
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::Parse { path, source } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// # fn construct(source: conferencier::toml::ser::Error) {
/// let _ = conferencier::Error::Serialize { target_path: None, source };
/// # }
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::Serialize { target_path, source } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// # fn construct(source: tokio::task::JoinError) {
/// let _ = conferencier::Error::SaveFileTaskJoin {
///     target_path: "config.toml".into(),
///     source,
/// };
/// # }
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::SaveFileTaskJoin { target_path, source } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// let _ = conferencier::Error::MissingKey {
///     table_path: "App".into(),
///     key: "port".into(),
/// };
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::MissingKey { table_path, key } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// let _ = conferencier::Error::TypeMismatch {
///     table_path: "App".into(),
///     key: "port".into(),
///     expected: "integer",
///     found: "string",
/// };
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::TypeMismatch { table_path, key, expected, found } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// let _ = conferencier::Error::ValueParse {
///     table_path: "App".into(),
///     key: "port".into(),
///     message: "out of range".into(),
/// };
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::ValueParse { table_path, key, message } = error {}
/// # }
/// ```
///
/// ```compile_fail
/// let _ = conferencier::Error::UnknownKey {
///     table_path: "App".into(),
///     key: "port".into(),
/// };
/// ```
///
/// ```compile_fail
/// # fn inspect(error: conferencier::Error) {
/// if let conferencier::Error::UnknownKey { table_path, key } = error {}
/// # }
/// ```
struct ErrorVariantExtensibilityDoctests;

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

    fn assert_display_without_source(error: &Error, expected: &str) {
        let display = error.to_string();
        assert_eq!(display, expected);
        let source = error.source().expect("wrapping errors have a source");
        assert!(!display.contains(&source.to_string()));
    }

    #[test]
    fn error_is_send_sync_and_static() {
        fn assert_traits<T: Send + Sync + 'static>() {}
        assert_traits::<Error>();
    }

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
            (
                Error::unknown_key_error("App", "port"),
                "unknown key App.port",
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
            let Error::Io { source: stored, .. } = &error else {
                unreachable!("the constructor returns an I/O error");
            };
            assert!(std::ptr::eq(source, stored));
        }

        let path = PathBuf::from("temp.toml");
        let error = Error::io_error_with_operation(
            Some(path.clone()),
            IoOperation::WriteTemporary,
            IoError::new(ErrorKind::WriteZero, "write failed"),
        );
        assert_eq!(
            error.to_string(),
            "failed to write temporary file \"temp.toml\""
        );
        assert!(matches!(
            &error,
            Error::Io {
                path: Some(error_path),
                operation: Some(IoOperation::WriteTemporary),
                target_path: None,
                source,
                ..
            } if error_path == &path
                && source.kind() == ErrorKind::WriteZero
                && source.to_string() == "write failed"
        ));
        let source = error.source().unwrap().downcast_ref::<IoError>().unwrap();
        assert_eq!(source.kind(), ErrorKind::WriteZero);
        assert_eq!(source.to_string(), "write failed");
        let Error::Io { source: stored, .. } = &error else {
            unreachable!("the constructor returns an I/O error");
        };
        assert!(std::ptr::eq(source, stored));
    }

    #[test]
    fn io_error_conversion_leaves_context_unset() {
        let error = Error::from(IoError::new(ErrorKind::NotFound, "missing config"));
        assert!(matches!(
            &error,
            Error::Io {
                path: None,
                operation: None,
                target_path: None,
                source,
                ..
            } if source.kind() == ErrorKind::NotFound && source.to_string() == "missing config"
        ));
        let source = error.source().unwrap().downcast_ref::<IoError>().unwrap();
        assert_eq!(source.kind(), ErrorKind::NotFound);
        assert_eq!(source.to_string(), "missing config");
    }

    #[test]
    fn successful_save_file_cleanup_returns_original_error() {
        let error = Error::io_error_with_operation(
            Some(PathBuf::from("config.toml")),
            IoOperation::ReplaceTarget,
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
            Error::Io {
                path,
                operation: Some(IoOperation::ReplaceTarget),
                target_path: Some(target_path),
                source,
                ..
            } => {
                assert_eq!(path, Some(PathBuf::from("config.toml")));
                assert_eq!(target_path, PathBuf::from("config.toml"));
                assert_eq!(source.kind(), ErrorKind::PermissionDenied);
                assert_eq!(source.to_string(), "save failed");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn missing_save_file_cleanup_returns_original_error() {
        let error = Error::io_error_with_operation(
            Some(PathBuf::from("config.toml")),
            IoOperation::ReplaceTarget,
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
            Error::Io {
                path,
                operation: Some(IoOperation::ReplaceTarget),
                target_path: Some(target_path),
                source,
                ..
            } => {
                assert_eq!(path, Some(PathBuf::from("config.toml")));
                assert_eq!(target_path, PathBuf::from("config.toml"));
                assert_eq!(source.kind(), ErrorKind::WriteZero);
                assert_eq!(source.to_string(), "save failed");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn failed_save_file_cleanup_preserves_both_errors() {
        let error = Error::io_error_with_operation(
            Some(PathBuf::from(".conferencier.123.0.tmp")),
            IoOperation::WriteTemporary,
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
                let reported_source = error.source().unwrap().downcast_ref::<Error>().unwrap();
                assert!(std::ptr::eq(save_error.as_ref(), reported_source));
                match save_error.as_ref() {
                    Error::Io {
                        path: Some(path),
                        operation: Some(IoOperation::WriteTemporary),
                        target_path: Some(nested_target),
                        source,
                        ..
                    } => {
                        assert_eq!(path, temporary_path);
                        assert_eq!(nested_target, target_path);
                        assert_eq!(source.kind(), ErrorKind::PermissionDenied);
                        assert_eq!(source.to_string(), "save failed");
                    }
                    other => panic!("unexpected save error: {other}"),
                }
            }
            other => panic!("unexpected error: {other}"),
        }

        let source = error.source().expect("save error is the error source");
        assert!(source.downcast_ref::<Error>().is_some());
        assert_eq!(
            source.to_string(),
            "failed to write temporary file \".conferencier.123.0.tmp\" for \"config.toml\""
        );
        assert!(!error.to_string().contains(&source.to_string()));
        assert!(!source.to_string().contains("cleanup failed"));
        let io_source = source.source().unwrap().downcast_ref::<IoError>().unwrap();
        assert_eq!(io_source.to_string(), "save failed");
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
    fn parse_and_serialize_constructors_keep_source_out_of_display() {
        let path = PathBuf::from("config.toml");
        let parse_source = toml::from_str::<toml::Table>("[broken").unwrap_err();
        let parse_error = Error::parse_error(Some(path.clone()), parse_source);
        assert_eq!(
            parse_error.to_string(),
            "failed to parse TOML file \"config.toml\""
        );
        match &parse_error {
            Error::Parse {
                path: Some(error_path),
                source,
                ..
            } => {
                assert_eq!(error_path, &path);
                let reported_source = parse_error
                    .source()
                    .unwrap()
                    .downcast_ref::<toml::de::Error>()
                    .unwrap();
                assert!(std::ptr::eq(source, reported_source));
            }
            other => panic!("unexpected error: {other}"),
        }

        let serialize_source = toml::to_string(&"not a table").unwrap_err();
        let serialize_error = Error::serialize_error(Some(path.clone()), serialize_source);
        assert_eq!(
            serialize_error.to_string(),
            "failed to serialize TOML for \"config.toml\""
        );
        match &serialize_error {
            Error::Serialize {
                target_path: Some(target_path),
                source,
                ..
            } => {
                assert_eq!(target_path, &path);
                let reported_source = serialize_error
                    .source()
                    .unwrap()
                    .downcast_ref::<toml::ser::Error>()
                    .unwrap();
                assert!(std::ptr::eq(source, reported_source));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn io_display_uses_operation_and_path_precedence() {
        let cases = [
            (
                IoOperation::Read,
                "failed to read file",
                "failed to read file \"affected.toml\"",
                "failed to read file",
                "failed to read file \"affected.toml\"",
            ),
            (
                IoOperation::InspectTarget,
                "failed to inspect save target",
                "failed to inspect save target \"affected.toml\"",
                "failed to inspect save target \"target.toml\"",
                "failed to inspect save target \"affected.toml\"",
            ),
            (
                IoOperation::CreateTemporary,
                "failed to create temporary file",
                "failed to create temporary file \"affected.toml\"",
                "failed to create temporary file for \"target.toml\"",
                "failed to create temporary file \"affected.toml\" for \"target.toml\"",
            ),
            (
                IoOperation::PrepareTemporary,
                "failed to prepare temporary file",
                "failed to prepare temporary file \"affected.toml\"",
                "failed to prepare temporary file for \"target.toml\"",
                "failed to prepare temporary file \"affected.toml\" for \"target.toml\"",
            ),
            (
                IoOperation::WriteTemporary,
                "failed to write temporary file",
                "failed to write temporary file \"affected.toml\"",
                "failed to write temporary file for \"target.toml\"",
                "failed to write temporary file \"affected.toml\" for \"target.toml\"",
            ),
            (
                IoOperation::ReplaceTarget,
                "failed to replace target file",
                "failed to replace target file \"affected.toml\"",
                "failed to replace target file \"target.toml\"",
                "failed to replace target file \"affected.toml\"",
            ),
        ];

        for (operation, pathless, with_path, target_only, with_both) in cases {
            for (path, target_path, expected) in [
                (None, None, pathless),
                (Some("affected.toml"), None, with_path),
                (None, Some("target.toml"), target_only),
                (Some("affected.toml"), Some("target.toml"), with_both),
            ] {
                let error = Error::Io {
                    path: path.map(PathBuf::from),
                    operation: Some(operation),
                    target_path: target_path.map(PathBuf::from),
                    source: IoError::other("distinct I/O source"),
                };
                assert_eq!(
                    error.to_string(),
                    expected,
                    "{operation:?}, {path:?}, {target_path:?}"
                );
            }
        }

        for (path, target_path, expected) in [
            (None, None, "I/O error"),
            (
                Some("affected.toml"),
                None,
                "I/O error on \"affected.toml\"",
            ),
            (None, Some("target.toml"), "failed to save \"target.toml\""),
            (
                Some("affected.toml"),
                Some("target.toml"),
                "failed to save \"target.toml\"",
            ),
        ] {
            let error = Error::Io {
                path: path.map(PathBuf::from),
                operation: None,
                target_path: target_path.map(PathBuf::from),
                source: IoError::other("distinct I/O source"),
            };
            assert_eq!(error.to_string(), expected, "{path:?}, {target_path:?}");
        }
    }

    #[tokio::test]
    async fn file_error_displays_escape_paths_without_repeating_sources() {
        let fragment = "name\"\\\n\r\u{1b}\u{2028}\u{2029}";
        let escaped = r#"name\"\\\n\r\u{1b}\u{2028}\u{2029}"#;
        let temporary_path = PathBuf::from(format!("temp-{fragment}"));
        let target_path = PathBuf::from(format!("target-{fragment}"));
        let temporary_display = format!("temp-{escaped}");
        let target_display = format!("target-{escaped}");

        let io_error = Error::io_error_with_operation(
            Some(temporary_path.clone()),
            IoOperation::WriteTemporary,
            IoError::other("primary I/O failure"),
        )
        .with_save_target(&target_path);
        assert_display_without_source(
            &io_error,
            &format!(
                "failed to write temporary file \"{temporary_display}\" for \"{target_display}\""
            ),
        );
        assert!(matches!(
            &io_error,
            Error::Io { path: Some(path), target_path: Some(target), .. }
                if path == &temporary_path && target == &target_path
        ));
        assert!(
            io_error
                .source()
                .unwrap()
                .downcast_ref::<IoError>()
                .is_some()
        );

        let parse_source = toml::from_str::<toml::Table>("[broken").unwrap_err();
        let parse_error = Error::parse_error(Some(temporary_path.clone()), parse_source);
        assert_display_without_source(
            &parse_error,
            &format!("failed to parse TOML file \"{temporary_display}\""),
        );
        assert!(matches!(
            &parse_error,
            Error::Parse { path: Some(path), .. } if path == &temporary_path
        ));
        assert!(
            parse_error
                .source()
                .unwrap()
                .downcast_ref::<toml::de::Error>()
                .is_some()
        );

        let serialize_source = toml::to_string(&"not a table").unwrap_err();
        let serialize_error = Error::serialize_error(Some(target_path.clone()), serialize_source);
        assert_display_without_source(
            &serialize_error,
            &format!("failed to serialize TOML for \"{target_display}\""),
        );
        assert!(matches!(
            &serialize_error,
            Error::Serialize { target_path: Some(target), .. } if target == &target_path
        ));
        assert!(
            serialize_error
                .source()
                .unwrap()
                .downcast_ref::<toml::ser::Error>()
                .is_some()
        );

        let task = tokio::spawn(std::future::pending::<()>());
        task.abort();
        let source = task.await.unwrap_err();
        let join_error = Error::SaveFileTaskJoin {
            target_path: target_path.clone(),
            source,
        };
        assert_display_without_source(&join_error, &format!("failed to save \"{target_display}\""));
        assert!(matches!(
            &join_error,
            Error::SaveFileTaskJoin { target_path: target, .. } if target == &target_path
        ));
        assert!(
            join_error
                .source()
                .unwrap()
                .downcast_ref::<tokio::task::JoinError>()
                .is_some()
        );

        let cleanup_error = Error::SaveFileCleanup {
            target_path: target_path.clone(),
            save_error: Box::new(io_error),
            temporary_path: temporary_path.clone(),
            cleanup_error: IoError::other("secondary cleanup failure"),
        };
        assert_display_without_source(
            &cleanup_error,
            &format!(
                "failed to save \"{target_display}\"; removing temporary file \"{temporary_display}\" also failed (secondary cleanup failure)"
            ),
        );
        assert!(matches!(
            &cleanup_error,
            Error::SaveFileCleanup {
                target_path: target,
                temporary_path: temporary,
                ..
            } if target == &target_path && temporary == &temporary_path
        ));
        assert!(
            cleanup_error
                .source()
                .unwrap()
                .downcast_ref::<Error>()
                .is_some()
        );
    }
}
