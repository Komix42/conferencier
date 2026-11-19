use std::fmt;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use tokio::fs;
use toml::value::Datetime;
use toml::{Table, Value};

use crate::error::{Error, IoOperation, Result};
use crate::value_conversion;

/// Shared handle to an in-memory TOML configuration document.
///
/// # Handle semantics
///
/// Clones share the same document; they do not copy it. A value set through one clone is
/// visible through all of them, and [`load_str`](Self::load_str) or
/// [`load_file`](Self::load_file) replaces the document for all of them. Share the store
/// between tasks with `store.clone()`; [`new`](Self::new) and [`Default`] create a new,
/// empty store.
///
/// # Synchronous and asynchronous methods
///
/// Only file I/O is async: [`from_file`](Self::from_file), [`load_file`](Self::load_file)
/// and [`save_file`](Self::save_file) run their file operations on Tokio's blocking thread
/// pool. Every other method works only on
/// the in-memory document and is synchronous, so it can also be called before the runtime
/// starts or from synchronous code on a runtime thread. Conferencier does not start a
/// Tokio runtime or choose its flavor; applications enable the Tokio features they need.
/// The file methods work on any Tokio runtime, including a current-thread runtime.
///
/// # Absence of sections and keys
///
/// - Peeks report absence in their type and treat a non-table top-level entry as absent:
///   [`get_value`](Self::get_value) and [`get_section`](Self::get_section) return
///   [`Option`], [`has_section`](Self::has_section) returns `bool`, and
///   [`list_sections`](Self::list_sections) lists only table entries.
/// - Typed getters such as [`get_integer`](Self::get_integer) require the key and fail with
///   [`Error::MissingKey`] or [`Error::TypeMismatch`].
/// - Operations that write into, replace or enumerate a section require the entry named
///   `section`, if present, to be a table and fail with [`Error::TypeMismatch`] otherwise:
///   [`set_value`](Self::set_value), the typed setters, [`set_section`](Self::set_section),
///   [`ensure_section`](Self::ensure_section), [`remove_key`](Self::remove_key) and
///   [`list_keys`](Self::list_keys).
///
/// `section` is always a literal top-level key, not a dotted path.
/// `key` is literal too: use [`get_section`](Self::get_section) to inspect a nested
/// table and [`set_section`](Self::set_section) to replace its owning section.
///
/// # Example
///
/// ```
/// use conferencier::ConfigStore;
///
/// let store = ConfigStore::from_str("[App]\nport = 8080")?;
/// let shared = store.clone();
/// shared.set_integer("App", "port", 9090)?;
/// assert_eq!(store.get_integer("App", "port")?, 9090);
///
/// let mut snapshot = store.get_section("App").expect("App is a table");
/// snapshot.insert("enabled".into(), conferencier::toml::Value::Boolean(true));
/// assert!(store.get_value("App", "enabled").is_none());
/// store.set_section("App", snapshot)?;
/// assert!(store.get_boolean("App", "enabled")?);
/// # Ok::<(), conferencier::Error>(())
/// ```
///
/// # Locking
///
/// The document is guarded by a [`std::sync::RwLock`]. The lock is held only for the
/// operation on the in-memory document: never across an `.await`, never during file I/O
/// and never while user code runs, and the store hands out no guards. Calling the store
/// again, also from another task, therefore cannot deadlock on it. A contended acquisition
/// blocks the worker thread, and the lock gives no reader/writer priority guarantee. If a
/// panic ever poisons the lock, later calls continue with the data, which every write
/// leaves consistent.
///
/// Typed setters finish all input conversions and iterator consumption and build the
/// complete TOML value before acquiring the write lock. They then validate the section
/// and replace the value in one update. Input preparation therefore runs even when the
/// section is invalid, but caller-provided code never runs under the lock and readers
/// never observe a partially built array.
///
/// Individual calls are synchronized, not sequences of calls: a read followed by
/// a write is not a transaction, and another task may change the document between them.
/// The [guide](https://docs.rs/conferencier/latest/conferencier/guide/concepts/index.html#locking)
/// explains the design assumptions behind this lock.
///
/// # Debug output
///
/// The `Debug` implementation lists top-level key names only and never displays
/// configuration values. If the lock is unavailable, it reports that the keys could not
/// be read rather than waiting. Key names may themselves be sensitive, and the output
/// format is not stable or intended for parsing.
#[derive(Clone, Default)]
pub struct ConfigStore {
    table: Arc<RwLock<Table>>,
}

impl fmt::Debug for ConfigStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys = match self.table.try_read() {
            Ok(table) => Some(table.keys().cloned().collect::<Vec<_>>()),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(error)) => {
                Some(error.into_inner().keys().cloned().collect::<Vec<_>>())
            }
        };

        let mut debug_struct = f.debug_struct("ConfigStore");
        match keys {
            Some(keys) => {
                debug_struct.field("keys", &keys);
            }
            None => {
                debug_struct.field("keys", &"<unavailable: locked>");
            }
        }
        debug_struct.finish_non_exhaustive()
    }
}

impl ConfigStore {
    /// Creates an empty configuration store.
    ///
    /// Equivalent to [`Default::default`]. Each call creates an independent document;
    /// use [`Clone::clone`] to share an existing one.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a store from a TOML string.
    ///
    /// `source.parse::<ConfigStore>()` does the same through [`FromStr`].
    /// This creates a new document without reading a file or requiring a Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] when `source` is not valid TOML.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(source: &str) -> Result<Self> {
        Ok(Self::from_table(Self::parse_table(source)?))
    }

    /// Reads a TOML file asynchronously and builds a store from it.
    ///
    /// A failed read returns [`Error::Io`] with [`IoOperation::Read`] and `path`.
    /// This uses [`tokio::fs`] and must run within a Tokio runtime. There is no
    /// blocking variant; the [guide](https://docs.rs/conferencier/latest/conferencier/guide/concepts/index.html#reading-a-file-before-the-runtime-starts)
    /// shows how to read a file before the runtime starts.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read as UTF-8, or [`Error::Parse`]
    /// if its contents are not valid TOML. Parse errors carry parser diagnostics
    /// and the file path.
    ///
    /// # Panics
    ///
    /// Panics if this future is polled without an active Tokio runtime when it reaches
    /// the Tokio file read.
    pub async fn from_file(path: impl AsRef<Path> + Send + Sync) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let contents = read_file(&path).await?;
        Self::from_str(&contents).map_err(|error| error.with_parse_path(&path))
    }

    /// Replaces the in-memory document with the contents of the provided TOML string.
    ///
    /// This replaces all sections and top-level values, not just the keys in `source`.
    /// Every clone of this handle observes the replacement; existing scopes do not
    /// reload automatically.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] for invalid TOML, leaving the whole document unchanged.
    pub fn load_str(&self, source: &str) -> Result<()> {
        let table = Self::parse_table(source)?;
        *self.write() = table;
        Ok(())
    }

    /// Replaces the in-memory document with the contents of the TOML file at `path`.
    ///
    /// The file is read and parsed before the document is replaced; on error the document
    /// stays unchanged. All clones see the replacement, but existing scopes must be
    /// reloaded separately with [`ConfigScope::load`](crate::ConfigScope::load).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] with [`IoOperation::Read`] and `path` if reading
    /// fails, or [`Error::Parse`] with `path` for invalid TOML.
    ///
    /// # Panics
    ///
    /// Panics if this future is polled without an active Tokio runtime when it reaches
    /// the Tokio file read.
    pub async fn load_file(&self, path: impl AsRef<Path> + Send + Sync) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let contents = read_file(&path).await?;
        self.load_str(&contents)
            .map_err(|error| error.with_parse_path(&path))
    }

    /// Serializes the current document to a TOML string.
    ///
    /// This captures one consistent document while holding a read lock. It does not
    /// modify the store or include unsaved changes in a scope. Comments, formatting,
    /// key order and original value notation are not preserved.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialize`] if the in-memory values cannot be encoded as TOML.
    /// The error's `target_path` is `None`.
    pub fn save_str(&self) -> Result<String> {
        toml::to_string(&*self.read()).map_err(Error::from)
    }

    /// Writes a serialized snapshot of the document to the file at `path`.
    ///
    /// The document is serialized first; the store lock is not held during file I/O.
    /// Changes made after serialization are not part of this snapshot. The snapshot is
    /// written to a temporary file next to the target, which then replaces the target
    /// with a single rename, so comments and formatting are lost as with
    /// [`save_str`](Self::save_str). Parent directories are not created.
    ///
    /// Concurrent calls to the same target, including calls through cloned handles,
    /// each install one complete snapshot; which call wins is unspecified. Replacement
    /// is atomic only where the filesystem provides atomic rename, a failed rename is
    /// not retried, and crash durability is not promised.
    ///
    /// Dropping the returned future does not cancel a save whose file operations have
    /// started: it can still replace the target. A relative `path` depends on the
    /// current directory until those operations finish.
    ///
    /// Only a missing target or a regular file is accepted: a final-component symlink,
    /// a directory, another non-regular file or a Windows reparse point is rejected,
    /// while links in parent directories are allowed. Names beginning with
    /// `.conferencier.` are reserved for temporary files and must not be used as
    /// targets. On Unix, an existing target's `rwx` mode bits are preserved;
    /// ownership, ACLs, Windows access settings and other metadata are not.
    ///
    /// The [guide](https://docs.rs/conferencier/latest/conferencier/guide/files/index.html)
    /// describes the replacement, cancellation, temporary files with all reserved
    /// names, accepted targets and preserved permissions in detail, including how to
    /// create a private configuration file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialize`] with `target_path` before any file is written
    /// if serialization fails. Unsupported targets return [`Error::Io`] with
    /// [`IoOperation::InspectTarget`], [`ErrorKind::InvalidInput`], and the target
    /// path before creating a temporary file. Other file-operation errors carry
    /// their [`IoOperation`], the relevant file in `path`, and the save destination
    /// in `target_path`. If the blocking task cannot be joined, returns
    /// [`Error::SaveFileTaskJoin`]. If removing a temporary file also fails,
    /// returns [`Error::SaveFileCleanup`] with the target path, original error,
    /// temporary path and cleanup error. An I/O failure does not change the
    /// in-memory store.
    ///
    /// # Panics
    ///
    /// Panics if serialization succeeds and this future is polled without an active Tokio
    /// runtime when it starts the blocking file operation. A serialization error is
    /// returned before a runtime is needed.
    pub async fn save_file(&self, path: impl AsRef<Path> + Send + Sync) -> Result<()> {
        let path = path.as_ref();
        let serialized = self
            .save_str()
            .map_err(|error| error.with_save_target(path))?;
        write_atomic(path, serialized).await
    }

    /// Returns an owned snapshot of the raw TOML value at `section` and `key`.
    ///
    /// Both arguments name literal keys, not dotted paths. A missing section, a
    /// non-table section or a missing key returns `None`. Changing the returned
    /// value does not change the store; use [`set_value`](Self::set_value) to write it.
    pub fn get_value(&self, section: &str, key: &str) -> Option<Value> {
        section_table(&self.read(), section)
            .and_then(|table| table.get(key))
            .cloned()
    }

    /// Returns a cloned snapshot of the table stored at `section`, if it exists.
    ///
    /// Missing or non-table top-level entries return `None`. The snapshot includes
    /// nested tables and unknown keys; edits to it do not affect the store until
    /// passed to [`set_section`](Self::set_section).
    pub fn get_section(&self, section: &str) -> Option<Table> {
        section_table(&self.read(), section).cloned()
    }

    /// Returns a snapshot of `section`: empty when absent, an error when not a table.
    pub(crate) fn read_section(&self, section: &str) -> Result<Table> {
        match self.read().get(section) {
            Some(Value::Table(table)) => Ok(table.clone()),
            Some(other) => Err(section_mismatch(section, other)),
            None => Ok(Table::new()),
        }
    }

    /// Atomically replaces the complete table at `section`, creating it if absent.
    ///
    /// Unknown keys in the old section are removed. If the section holds a
    /// non-table value, returns a type mismatch and leaves the store unchanged.
    /// Other sections are not changed. `section` is a literal top-level key.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] with an empty `path` and the section name
    /// as `key` when the entry is not a table.
    pub fn set_section(&self, section: &str, table: Table) -> Result<()> {
        let mut guard = self.write();
        match guard.entry(section.to_owned()) {
            toml::map::Entry::Occupied(mut entry) => match entry.get_mut() {
                Value::Table(inner) => {
                    *inner = table;
                    Ok(())
                }
                other => Err(section_mismatch(section, other)),
            },
            toml::map::Entry::Vacant(entry) => {
                entry.insert(Value::Table(table));
                Ok(())
            }
        }
    }

    /// Inserts `value` at `section.key`, creating the section if necessary.
    ///
    /// Both arguments name literal keys. An existing value at the key is replaced,
    /// regardless of its type; all other keys are left unchanged. Tables and arrays
    /// are accepted as raw values too.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not
    /// a table, leaving the document unchanged.
    pub fn set_value(&self, section: &str, key: &str, value: Value) -> Result<()> {
        let mut guard = self.write();
        match guard.entry(section.to_owned()) {
            toml::map::Entry::Occupied(mut entry) => match entry.get_mut() {
                Value::Table(inner) => {
                    inner.insert(key.to_owned(), value);
                    Ok(())
                }
                other => Err(section_mismatch(section, other)),
            },
            toml::map::Entry::Vacant(entry) => {
                let mut table = Table::new();
                table.insert(key.to_owned(), value);
                entry.insert(Value::Table(table));
                Ok(())
            }
        }
    }

    /// Returns `true` when the store contains a table for `section`.
    ///
    /// An empty table counts as present. Missing entries and entries of any other
    /// TOML type return `false`.
    pub fn has_section(&self, section: &str) -> bool {
        matches!(self.read().get(section), Some(Value::Table(_)))
    }

    /// Creates `section` as an empty table if it is missing.
    ///
    /// An existing table is left unchanged, so the call is idempotent; unlike
    /// [`set_section`](Self::set_section), it never replaces a section. A non-table entry
    /// named `section` returns a type mismatch.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] for a non-table entry without modifying it.
    pub fn ensure_section(&self, section: &str) -> Result<()> {
        let mut guard = self.write();
        match guard.entry(section.to_owned()) {
            toml::map::Entry::Occupied(entry) => match entry.get() {
                Value::Table(_) => Ok(()),
                other => Err(section_mismatch(section, other)),
            },
            toml::map::Entry::Vacant(entry) => {
                entry.insert(Value::Table(Table::new()));
                Ok(())
            }
        }
    }

    /// Removes `key` from `section`, ignoring missing keys or sections.
    ///
    /// Removing the last key leaves an empty section in place. To remove the
    /// top-level entry as well, use [`remove_section`](Self::remove_section).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if `section` exists but is not a table.
    pub fn remove_key(&self, section: &str, key: &str) -> Result<()> {
        match self.write().get_mut(section) {
            Some(Value::Table(inner)) => {
                inner.remove(key);
                Ok(())
            }
            Some(other) => Err(section_mismatch(section, other)),
            None => Ok(()),
        }
    }

    /// Removes the top-level entry `section` from the store, ignoring missing sections.
    ///
    /// Removes the whole entry, including nested tables, even if it is not a table.
    /// Unlike [`remove_key`](Self::remove_key), this operation has no error result.
    pub fn remove_section(&self, section: &str) {
        self.write().remove(section);
    }

    /// Lists all sections currently backed by a TOML table.
    ///
    /// Empty tables are included; top-level scalar and array entries are not.
    /// The returned names are owned snapshots, with no promised ordering.
    pub fn list_sections(&self) -> Vec<String> {
        self.read()
            .iter()
            .filter_map(|(name, value)| value.as_table().map(|_| name.clone()))
            .collect()
    }

    /// Lists the keys contained in `section`, or an empty vector when the section is absent.
    ///
    /// Lists immediate keys only, including those holding tables or arrays; it does
    /// not walk nested tables. The returned names have no promised ordering.
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if `section` exists but is not a table.
    pub fn list_keys(&self, section: &str) -> Result<Vec<String>> {
        match self.read().get(section) {
            Some(Value::Table(table)) => Ok(table.keys().cloned().collect()),
            Some(other) => Err(section_mismatch(section, other)),
            None => Ok(Vec::new()),
        }
    }

    /// Retrieves a string value stored at `section.key`.
    ///
    /// Returns an owned string. Numbers, booleans and datetimes are not coerced to strings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for a missing section or key, or
    /// [`Error::TypeMismatch`] for a non-table section or a non-string value.
    pub fn get_string(&self, section: &str, key: &str) -> Result<String> {
        let value = self.fetch_value(section, key)?;
        value_conversion::string(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves an integer value stored at `section.key`.
    ///
    /// TOML integers are signed 64-bit values. Floats and numeric strings are not converted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for a missing section or key, or
    /// [`Error::TypeMismatch`] for a non-table section or a non-integer value.
    pub fn get_integer(&self, section: &str, key: &str) -> Result<i64> {
        let value = self.fetch_value(section, key)?;
        value_conversion::integer(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a floating-point value stored at `section.key`.
    ///
    /// Accepts TOML floats and integers. Integer-to-`f64` conversion may lose precision
    /// for large integers; numeric strings are not parsed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for a missing section or key, or
    /// [`Error::TypeMismatch`] for a non-table section or an incompatible value.
    pub fn get_float(&self, section: &str, key: &str) -> Result<f64> {
        let value = self.fetch_value(section, key)?;
        value_conversion::float(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a boolean value stored at `section.key`.
    ///
    /// Accepts only a TOML boolean, not integers or strings such as `"true"`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for a missing section or key, or
    /// [`Error::TypeMismatch`] for a non-table section or a non-boolean value.
    pub fn get_boolean(&self, section: &str, key: &str) -> Result<bool> {
        let value = self.fetch_value(section, key)?;
        value_conversion::boolean(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a [`Datetime`] value stored at `section.key`, parsing strings when necessary.
    ///
    /// Accepts TOML date, time and datetime values, or strings in a form understood
    /// by [`Datetime`]. It does not supply a timezone or normalize omitted components.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for absence, [`Error::TypeMismatch`] for a
    /// non-table section or an incompatible type, or [`Error::ValueParse`] for
    /// a string that cannot be parsed as a TOML datetime.
    pub fn get_datetime(&self, section: &str, key: &str) -> Result<Datetime> {
        let value = self.fetch_value(section, key)?;
        value_conversion::datetime(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a string array stored at `section.key`.
    ///
    /// Returns an owned vector; an empty TOML array produces an empty vector.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for absence or [`Error::TypeMismatch`] if the
    /// section is not a table or the value is not an array. A non-string element
    /// produces [`Error::ValueParse`] naming its zero-based index.
    pub fn get_string_vec(&self, section: &str, key: &str) -> Result<Vec<String>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::string_vec(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves an integer array stored at `section.key`.
    ///
    /// Every element must be a TOML integer; floats and numeric strings are not converted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for absence or [`Error::TypeMismatch`] for a
    /// non-table section or non-array value. An incompatible element produces
    /// [`Error::ValueParse`] naming its zero-based index.
    pub fn get_integer_vec(&self, section: &str, key: &str) -> Result<Vec<i64>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::integer_vec(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a floating-point array stored at `section.key`.
    ///
    /// Accepts both float and integer elements, including mixed numeric arrays.
    /// Integer-to-`f64` conversion can lose precision.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for absence or [`Error::TypeMismatch`] for a
    /// non-table section or non-array value. An incompatible element produces
    /// [`Error::ValueParse`] naming its zero-based index.
    pub fn get_float_vec(&self, section: &str, key: &str) -> Result<Vec<f64>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::float_vec(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a boolean array stored at `section.key`.
    ///
    /// Every element must be a TOML boolean; strings and numbers are not converted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for absence or [`Error::TypeMismatch`] for a
    /// non-table section or non-array value. An incompatible element produces
    /// [`Error::ValueParse`] naming its zero-based index.
    pub fn get_boolean_vec(&self, section: &str, key: &str) -> Result<Vec<bool>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::boolean_vec(&crate::table_path::section_path(section), key, value)
    }

    /// Retrieves a [`Datetime`] array stored at `section.key`, parsing string values when necessary.
    ///
    /// Elements may be TOML datetime values or strings parsed as [`Datetime`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`] for absence or [`Error::TypeMismatch`] for a
    /// non-table section or non-array value. An incompatible or invalid element
    /// produces [`Error::ValueParse`] naming its zero-based index.
    pub fn get_datetime_vec(&self, section: &str, key: &str) -> Result<Vec<Datetime>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::datetime_vec(&crate::table_path::section_path(section), key, value)
    }

    /// Stores a string at `section.key`, creating the section if needed.
    ///
    /// Converts `value` with [`Into<String>`] before validating the section or acquiring
    /// the write lock. Replaces the key's previous value; other keys are unchanged.
    /// Section validation and errors are the same as [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged. The error has an empty `path` and uses
    /// `section` as its `key`.
    pub fn set_string(&self, section: &str, key: &str, value: impl Into<String>) -> Result<()> {
        self.set_value(section, key, Value::String(value.into()))
    }

    /// Stores an integer at `section.key`, creating the section if needed.
    ///
    /// Writes a TOML signed 64-bit integer. Replacement and section errors follow
    /// [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_integer(&self, section: &str, key: &str, value: i64) -> Result<()> {
        self.set_value(section, key, Value::Integer(value))
    }

    /// Stores a floating-point number at `section.key`, creating the section if needed.
    ///
    /// Writes a TOML float without integer coercion. Replacement and section errors
    /// follow [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_float(&self, section: &str, key: &str, value: f64) -> Result<()> {
        self.set_value(section, key, Value::Float(value))
    }

    /// Stores a boolean at `section.key`, creating the section if needed.
    ///
    /// Replaces the key's previous value. Section validation and errors follow
    /// [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_boolean(&self, section: &str, key: &str, value: bool) -> Result<()> {
        self.set_value(section, key, Value::Boolean(value))
    }

    /// Stores a TOML [`Datetime`] at `section.key`, creating the section if needed.
    ///
    /// Writes a datetime value, not a quoted string. Replacement and section errors
    /// follow [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_datetime(&self, section: &str, key: &str, value: Datetime) -> Result<()> {
        self.set_value(section, key, Value::Datetime(value))
    }

    /// Stores a string array at `section.key`, creating the section if needed.
    ///
    /// Accepts any iterator whose items convert into [`String`], including arrays of
    /// `&str`, vectors of `String`, and a borrowed `Vec<String>`. For a borrowed slice
    /// of `&str`, pass `values.iter().copied()`. A typed empty vector is stored as `[]`,
    /// not removed.
    ///
    /// The iterator is consumed and each item converted before validating the section
    /// or acquiring the write lock. Section validation and errors follow
    /// [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_string_vec(
        &self,
        section: &str,
        key: &str,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<()> {
        let array = values
            .into_iter()
            .map(|value| Value::String(value.into()))
            .collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores an integer array at `section.key`, creating the section if needed.
    ///
    /// Accepts any iterator of TOML signed 64-bit integers, including borrowed slices
    /// passed with `.iter().copied()`. An empty iterator is written as `[]`.
    /// Section validation and errors follow [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_integer_vec(
        &self,
        section: &str,
        key: &str,
        values: impl IntoIterator<Item = i64>,
    ) -> Result<()> {
        let array = values.into_iter().map(Value::Integer).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores a floating-point array at `section.key`, creating the section if needed.
    ///
    /// Accepts any iterator of TOML floating-point numbers, including borrowed slices
    /// passed with `.iter().copied()`. An empty iterator is written as `[]`.
    /// Section validation and errors follow [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_float_vec(
        &self,
        section: &str,
        key: &str,
        values: impl IntoIterator<Item = f64>,
    ) -> Result<()> {
        let array = values.into_iter().map(Value::Float).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores a boolean array at `section.key`, creating the section if needed.
    ///
    /// Accepts any iterator of booleans, including borrowed slices passed with
    /// `.iter().copied()`. An empty iterator is written as `[]`, not removed.
    /// Section validation and errors follow [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_boolean_vec(
        &self,
        section: &str,
        key: &str,
        values: impl IntoIterator<Item = bool>,
    ) -> Result<()> {
        let array = values.into_iter().map(Value::Boolean).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores a [`Datetime`] array at `section.key`, creating the section if needed.
    ///
    /// Accepts any iterator of [`Datetime`] values, including borrowed slices passed
    /// with `.iter().copied()`. Writes datetime values, not quoted strings. An empty
    /// iterator is written as `[]`.
    /// Section validation and errors follow [`set_value`](Self::set_value).
    ///
    /// # Errors
    ///
    /// Returns [`Error::TypeMismatch`] if the top-level entry named `section` is not a
    /// table, leaving the document unchanged.
    pub fn set_datetime_vec(
        &self,
        section: &str,
        key: &str,
        values: impl IntoIterator<Item = Datetime>,
    ) -> Result<()> {
        let array = values.into_iter().map(Value::Datetime).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Fetches the raw TOML [`Value`] stored at `section.key`, producing detailed errors.
    fn fetch_value(&self, section: &str, key: &str) -> Result<Value> {
        let path = crate::table_path::section_path(section);
        let guard = self.read();
        let section_value = guard
            .get(section)
            .ok_or_else(|| Error::missing_key_error(&path, key))?;
        let table = section_value
            .as_table()
            .ok_or_else(|| section_mismatch(section, section_value))?;
        table
            .get(key)
            .cloned()
            .ok_or_else(|| Error::missing_key_error(&path, key))
    }

    fn from_table(table: Table) -> Self {
        Self {
            table: Arc::new(RwLock::new(table)),
        }
    }

    /// Parses a TOML table from `source`, mapping parsing failures into [`Error`].
    fn parse_table(source: &str) -> Result<Table> {
        toml::from_str(source).map_err(Error::from)
    }

    // Every write is a single map operation or replaces a whole table, so the data behind a
    // poisoned lock is consistent and can be used as is.
    fn read(&self) -> RwLockReadGuard<'_, Table> {
        self.table.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Table> {
        self.table.write().unwrap_or_else(PoisonError::into_inner)
    }
}

impl FromStr for ConfigStore {
    type Err = Error;

    /// Parses a new store with the same behavior and errors as [`ConfigStore::from_str`].
    fn from_str(source: &str) -> Result<Self> {
        ConfigStore::from_str(source)
    }
}

/// Retrieves the table stored within `root` at `section`, if it exists and is a table.
fn section_table<'a>(root: &'a Table, section: &str) -> Option<&'a Table> {
    root.get(section)?.as_table()
}

/// The error for a top-level entry `section` that is not a table.
fn section_mismatch(section: &str, found: &Value) -> Error {
    Error::type_mismatch_error("", section, "table", value_conversion::describe(found))
}

/// Reads the file at `path`, attaching the path to I/O errors.
async fn read_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).await.map_err(|err| {
        Error::io_error_with_operation(Some(path.to_path_buf()), IoOperation::Read, err)
    })
}

/// Persists `contents` to `path` through a temporary sibling and a rename
/// (see [`ConfigStore::save_file`]).
///
/// Every call writes its own temporary sibling, so concurrent saves to the same path never
/// share a temporary file; on error cleanup is attempted. The blocking file
/// operations run on Tokio's blocking pool, like those of [`tokio::fs`].
async fn write_atomic(path: &Path, contents: String) -> Result<()> {
    let target_path = path.to_path_buf();
    let task_path = target_path.clone();
    tokio::task::spawn_blocking(move || write_atomic_blocking(&task_path, contents.as_bytes()))
        .await
        .map_err(|source| Error::SaveFileTaskJoin {
            target_path,
            source,
        })?
}

fn write_atomic_blocking(path: &Path, contents: &[u8]) -> Result<()> {
    write_atomic_blocking_with(
        path,
        contents,
        std::fs::symlink_metadata(path),
        prepare_temporary,
        Write::write_all,
        |from, to| std::fs::rename(from, to),
    )
}

fn write_atomic_blocking_with(
    path: &Path,
    contents: &[u8],
    metadata: std::io::Result<std::fs::Metadata>,
    prepare: impl FnOnce(&std::fs::File, Option<&std::fs::Metadata>) -> std::io::Result<()>,
    write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    mut rename: impl FnMut(&Path, &Path) -> std::io::Result<()>,
) -> Result<()> {
    let metadata =
        validate_target_metadata(path, metadata).map_err(|error| error.with_save_target(path))?;
    let (tmp_path, mut file) = create_temporary(metadata.as_ref(), || temporary_path(path))
        .map_err(|error| error.with_save_target(path))?;
    let result = (|| {
        prepare(&file, metadata.as_ref()).map_err(|err| {
            Error::io_error_with_operation(
                Some(tmp_path.clone()),
                IoOperation::PrepareTemporary,
                err,
            )
        })?;
        write(&mut file, contents).map_err(|err| {
            Error::io_error_with_operation(Some(tmp_path.clone()), IoOperation::WriteTemporary, err)
        })?;
        // Close the handle before the rename, which Windows requires.
        drop(file);
        rename(&tmp_path, path).map_err(|err| {
            Error::io_error_with_operation(
                Some(path.to_path_buf()),
                IoOperation::ReplaceTarget,
                err,
            )
        })
    })();
    match result {
        Ok(()) => Ok(()),
        Err(save_error) => Err(Error::with_save_file_cleanup(
            path.to_path_buf(),
            save_error.with_save_target(path),
            tmp_path.clone(),
            std::fs::remove_file(&tmp_path),
        )),
    }
}

fn validate_target_metadata(
    path: &Path,
    metadata: std::io::Result<std::fs::Metadata>,
) -> Result<Option<std::fs::Metadata>> {
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(Error::io_error_with_operation(
                Some(path.to_path_buf()),
                IoOperation::InspectTarget,
                err,
            ));
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        validate_windows_attributes(path, metadata.file_attributes())?;
    }

    let file_type = metadata.file_type();
    let unsupported = if file_type.is_file() {
        None
    } else if file_type.is_symlink() {
        Some("symlink")
    } else if file_type.is_dir() {
        Some("directory")
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            Some(if file_type.is_socket() {
                "socket"
            } else if file_type.is_fifo() {
                "FIFO"
            } else if file_type.is_char_device() {
                "character device"
            } else if file_type.is_block_device() {
                "block device"
            } else {
                "non-regular file"
            })
        }
        #[cfg(not(unix))]
        {
            Some("non-regular file")
        }
    };
    if let Some(kind) = unsupported {
        return Err(unsupported_target(path, kind));
    }
    Ok(Some(metadata))
}

fn unsupported_target(path: &Path, kind: &str) -> Error {
    Error::io_error_with_operation(
        Some(path.to_path_buf()),
        IoOperation::InspectTarget,
        std::io::Error::new(
            ErrorKind::InvalidInput,
            format!("unsupported save target: {kind}"),
        ),
    )
}

#[cfg(windows)]
fn validate_windows_attributes(path: &Path, attributes: u32) -> Result<()> {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x00000400;
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(unsupported_target(path, "Windows reparse point"));
    }
    Ok(())
}

/// Creates a new temporary file, never opening an existing entry.
fn create_temporary(
    metadata: Option<&std::fs::Metadata>,
    mut next_path: impl FnMut() -> PathBuf,
) -> Result<(PathBuf, std::fs::File)> {
    const ATTEMPTS: usize = 16;
    let mut attempt = 1;
    loop {
        let tmp_path = next_path();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if let Some(metadata) = metadata {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            options.mode(metadata.permissions().mode() & 0o777);
        }
        #[cfg(not(unix))]
        let _ = metadata;
        let opened = options.open(&tmp_path);
        match opened {
            Ok(file) => return Ok((tmp_path, file)),
            // A stale file from an earlier process can hold the name; the next name differs.
            Err(err) if err.kind() == ErrorKind::AlreadyExists && attempt < ATTEMPTS => {
                attempt += 1;
            }
            Err(err) => {
                return Err(Error::io_error_with_operation(
                    Some(tmp_path),
                    IoOperation::CreateTemporary,
                    err,
                ));
            }
        }
    }
}

#[cfg(unix)]
fn prepare_temporary(
    file: &std::fs::File,
    metadata: Option<&std::fs::Metadata>,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(metadata) = metadata {
        file.set_permissions(std::fs::Permissions::from_mode(
            metadata.permissions().mode() & 0o777,
        ))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn prepare_temporary(
    _file: &std::fs::File,
    _metadata: Option<&std::fs::Metadata>,
) -> std::io::Result<()> {
    Ok(())
}

/// Computes a unique temporary sibling path independent of the target filename.
fn temporary_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".conferencier.{}.{counter}.tmp",
        std::process::id()
    ))
}

#[cfg(test)]
mod save_tests;

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::error::Error as _;
    use std::future::Future;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::pin::Pin;
    use std::sync::mpsc::sync_channel;
    use std::task::{Context, Poll, Waker};
    use std::time::Duration;

    use super::*;
    use crate::error::{Error, Result};

    use tempfile::NamedTempFile;

    fn poll_without_runtime<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        let mut context = Context::from_waker(Waker::noop());
        future.poll(&mut context)
    }

    struct StoreReadingString(ConfigStore);

    impl From<StoreReadingString> for String {
        fn from(value: StoreReadingString) -> Self {
            assert_eq!(value.0.get_integer("App", "answer").unwrap(), 42);
            String::from("converted")
        }
    }

    #[test]
    fn new_store_is_empty() {
        let store = ConfigStore::new();
        assert!(store.list_sections().is_empty());
    }

    #[test]
    fn clones_share_store_data() -> Result<()> {
        let store = ConfigStore::new();
        let clone = store.clone();

        clone.set_string("App", "name", String::from("shared"))?;
        assert_eq!(store.get_string("App", "name")?, "shared");

        clone.load_str("[App]\nname = \"reloaded\"")?;
        assert_eq!(store.get_string("App", "name")?, "reloaded");
        Ok(())
    }

    #[test]
    fn from_str_and_parse_agree() -> Result<()> {
        let source = "[App]\nport = 8080\n";
        let inherent = ConfigStore::from_str(source)?;
        let parsed: ConfigStore = source.parse()?;
        assert_eq!(inherent.save_str()?, parsed.save_str()?);

        let invalid = "[App\nport = 8080";
        let inherent = ConfigStore::from_str(invalid).unwrap_err();
        let parsed = invalid.parse::<ConfigStore>().unwrap_err();
        assert!(matches!(inherent, Error::Parse { path: None, .. }));
        assert_eq!(inherent.to_string(), parsed.to_string());
        Ok(())
    }

    #[tokio::test]
    async fn from_file_reports_missing_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("missing.toml");
        let error = ConfigStore::from_file(&path).await.unwrap_err();
        assert!(matches!(
            &error,
            Error::Io {
                path: Some(reported),
                operation: Some(IoOperation::Read),
                target_path: None,
                ..
            } if reported == &path
        ));
        assert_eq!(error.to_string(), format!("failed to read file {path:?}"));
    }

    #[test]
    fn file_reads_panic_without_a_tokio_runtime() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[App]\nport = 80")?;

        let mut from_file = Box::pin(ConfigStore::from_file(&path));
        assert!(
            catch_unwind(AssertUnwindSafe(|| poll_without_runtime(
                from_file.as_mut()
            )))
            .is_err()
        );

        let store = ConfigStore::new();
        let mut load_file = Box::pin(store.load_file(&path));
        assert!(
            catch_unwind(AssertUnwindSafe(|| poll_without_runtime(
                load_file.as_mut()
            )))
            .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn file_parse_errors_keep_the_file_path_and_load_is_atomic() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("invalid.toml");
        std::fs::write(&path, "[broken")?;

        let error = ConfigStore::from_file(&path).await.unwrap_err();
        assert!(matches!(
            &error,
            Error::Parse {
                path: Some(error_path),
                ..
            } if error_path == &path
        ));
        assert_eq!(
            error.to_string(),
            format!("failed to parse TOML file {path:?}")
        );
        assert!(error.source().is_some());

        let store = ConfigStore::from_str("[App]\nport = 8080")?;
        let error = store.load_file(&path).await.unwrap_err();
        assert!(matches!(
            error,
            Error::Parse {
                path: Some(error_path),
                ..
            } if error_path == path
        ));
        assert_eq!(store.get_integer("App", "port")?, 8080);
        Ok(())
    }

    #[test]
    fn nesting_beyond_tomls_default_limit_returns_parse_errors() -> Result<()> {
        let depth = 128;
        let dotted_path = vec!["a"; depth].join(".");
        let inputs = [
            format!("value = {}0{}", "[".repeat(depth), "]".repeat(depth)),
            format!(
                "value = {}0{}",
                "{ value = ".repeat(depth),
                " }".repeat(depth)
            ),
            format!("{dotted_path}.value = 0"),
            format!("[{dotted_path}]\nvalue = 0"),
        ];
        let store = ConfigStore::from_str("[Existing]\nkeep = 1")?;
        let before = store.save_str()?;

        for source in inputs {
            let error = ConfigStore::from_str(&source).unwrap_err();
            assert!(matches!(error, Error::Parse { .. }));

            let error = store.load_str(&source).unwrap_err();
            assert!(matches!(error, Error::Parse { .. }));
            assert_eq!(store.save_str()?, before);
        }
        Ok(())
    }

    #[test]
    fn poisoned_lock_keeps_working() -> Result<()> {
        let store = ConfigStore::from_str("[App]\nport = 718293")?;
        let shared = store.clone();
        let _ = std::thread::spawn(move || {
            let _guard = shared.write();
            panic!("poison the store lock");
        })
        .join();
        assert!(store.table.is_poisoned());

        let debug = format!("{store:?}");
        assert!(debug.contains("App"));
        assert!(!debug.contains("718293"));
        store.set_integer("App", "port", 2)?;
        assert_eq!(store.get_integer("App", "port")?, 2);
        Ok(())
    }

    #[test]
    fn debug_lists_top_level_keys_without_values() -> Result<()> {
        let store = ConfigStore::from_str(
            "App = { password = \"sensitive_value_marker\" }\nMode = \"secret_mode_marker\"\nCounter = 92837465",
        )?;

        for debug in [format!("{store:?}"), format!("{store:#?}")] {
            assert!(debug.contains("App"));
            assert!(debug.contains("Mode"));
            assert!(debug.contains("Counter"));
            assert!(!debug.contains("sensitive_value_marker"));
            assert!(!debug.contains("secret_mode_marker"));
            assert!(!debug.contains("92837465"));
        }
        Ok(())
    }

    #[test]
    fn debug_does_not_wait_for_a_locked_store() -> Result<()> {
        let store = ConfigStore::from_str("[App]\nport = 80")?;
        let guard = store.write();
        let debug_store = store.clone();
        let (sender, receiver) = sync_channel(1);

        let debug_thread = std::thread::spawn(move || {
            let _ = sender.send(format!("{debug_store:?}"));
        });

        let result = receiver.recv_timeout(Duration::from_secs(5));
        drop(guard);
        let debug = result.expect("Debug should not wait for the store lock");
        debug_thread
            .join()
            .expect("formatting the store should not panic");
        assert!(debug.contains("locked"));
        Ok(())
    }

    #[test]
    fn section_tables_create_replace_and_snapshot() -> Result<()> {
        let store = ConfigStore::from_str("[Other]\nkeep = 1")?;
        assert!(store.read_section("App")?.is_empty());
        let first: Table = "old = 1\nnested = { stale = true }".parse().unwrap();
        store.set_section("App", first.clone())?;
        let mut snapshot = store.read_section("App")?;
        assert_eq!(snapshot, first);
        snapshot.clear();
        assert_eq!(store.get_section("App"), Some(first));

        let second: Table = "new = 2".parse().unwrap();
        store.set_section("App", second.clone())?;
        assert_eq!(store.get_section("App"), Some(second));
        assert_eq!(store.get_integer("Other", "keep")?, 1);
        store.set_section("App", Table::new())?;
        assert_eq!(store.get_section("App"), Some(Table::new()));
        store.set_section("App.server", Table::new())?;
        assert!(store.has_section("App.server"));
        assert!(store.get_value("App", "server").is_none());
        Ok(())
    }

    #[test]
    fn strict_section_operations_reject_non_tables_without_changes() -> Result<()> {
        let store = ConfigStore::from_str("App = 7\n[Other]\nkeep = true")?;
        let before = store.save_str()?;
        for error in [
            store.read_section("App").unwrap_err(),
            store.set_section("App", Table::new()).unwrap_err(),
        ] {
            assert!(matches!(
                &error,
                Error::TypeMismatch { table_path, key, expected: "table", found: "integer" }
                    if table_path.is_empty() && key == "App"
            ));
            assert_eq!(error.to_string(), "expected table at App but found integer");
        }
        assert_eq!(store.save_str()?, before);
        Ok(())
    }

    #[test]
    fn diagnostics_quote_literal_top_level_sections_and_keys() {
        let error = ConfigStore::new()
            .get_integer("App.server", "port")
            .unwrap_err();
        assert!(matches!(
            error,
            Error::MissingKey { ref table_path, ref key }
                if table_path == "\"App.server\"" && key == "port"
        ));
        assert_eq!(error.to_string(), "missing key \"App.server\".port");

        let error = Error::missing_key_error("App", "a.b");
        assert_eq!(error.to_string(), "missing key App.\"a.b\"");

        let error = ConfigStore::new().get_integer("", "key").unwrap_err();
        assert_eq!(error.to_string(), "missing key \"\".key");

        let error = Error::missing_key_error("App", "a\"b\n");
        assert_eq!(error.to_string(), "missing key App.\"a\\\"b\\n\"");
    }

    #[test]
    fn set_and_get_string_roundtrip() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("App", "name", "demo")?;
        assert_eq!(store.get_string("App", "name")?, "demo");
        assert_eq!(
            store.get_value("App", "name"),
            Some(Value::String("demo".into()))
        );
        Ok(())
    }

    #[test]
    fn typed_setters_accept_into_and_iterable_inputs() -> Result<()> {
        let store = ConfigStore::new();

        store.set_string("App", "borrowed", "text")?;
        store.set_string("App", "owned", String::from("text"))?;

        store.set_string_vec("App", "array", ["array", "values"])?;
        store.set_string_vec(
            "App",
            "owned_vec",
            vec![String::from("owned"), String::from("values")],
        )?;
        let borrowed_vec = vec![String::from("borrowed"), String::from("values")];
        store.set_string_vec("App", "borrowed_vec", &borrowed_vec)?;
        let borrowed_slice = ["slice", "values"];
        store.set_string_vec("App", "borrowed_slice", borrowed_slice.iter().copied())?;
        store.set_string_vec(
            "App",
            "mapped",
            (0..2).map(|index| format!("mapped-{index}")),
        )?;

        let integers = [1, 2];
        store.set_integer_vec("App", "integers", integers.iter().copied())?;
        let floats = [1.5, 2.5];
        store.set_float_vec("App", "floats", floats.iter().copied())?;
        let booleans = [true, false];
        store.set_boolean_vec("App", "booleans", booleans.iter().copied())?;
        let date: Datetime = "2024-01-01".parse().expect("valid TOML date");
        let dates = [date, date];
        store.set_datetime_vec("App", "dates", dates.iter().copied())?;

        store.set_integer_vec("App", "empty_integers", vec![])?;
        store.set_string_vec("App", "empty_strings", Vec::<String>::new())?;

        assert_eq!(store.get_string("App", "borrowed")?, "text");
        assert_eq!(store.get_string("App", "owned")?, "text");
        assert_eq!(
            store.get_string_vec("App", "array")?,
            vec![String::from("array"), String::from("values")]
        );
        assert_eq!(
            store.get_string_vec("App", "owned_vec")?,
            vec![String::from("owned"), String::from("values")]
        );
        assert_eq!(
            store.get_string_vec("App", "borrowed_vec")?,
            vec![String::from("borrowed"), String::from("values")]
        );
        assert_eq!(
            store.get_string_vec("App", "borrowed_slice")?,
            vec![String::from("slice"), String::from("values")]
        );
        assert_eq!(
            store.get_string_vec("App", "mapped")?,
            vec![String::from("mapped-0"), String::from("mapped-1")]
        );
        assert_eq!(store.get_integer_vec("App", "integers")?, integers);
        assert_eq!(store.get_float_vec("App", "floats")?, floats);
        assert_eq!(store.get_boolean_vec("App", "booleans")?, booleans);
        assert_eq!(store.get_datetime_vec("App", "dates")?, dates);
        assert!(store.get_integer_vec("App", "empty_integers")?.is_empty());
        assert!(store.get_string_vec("App", "empty_strings")?.is_empty());
        Ok(())
    }

    #[test]
    fn setter_inputs_can_read_the_same_store_without_deadlocking() -> Result<()> {
        let store = ConfigStore::from_str("[App]\nanswer = 42")?;
        let (sender, receiver) = sync_channel(1);
        let conversion_store = store.clone();
        std::thread::spawn(move || {
            let result = conversion_store.set_string(
                "App",
                "converted",
                StoreReadingString(conversion_store.clone()),
            );
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("custom string conversion should finish outside the write lock")?;
        assert_eq!(store.get_string("App", "converted")?, "converted");

        let (sender, receiver) = sync_channel(1);
        let iterator_store = store.clone();
        std::thread::spawn(move || {
            let values = (0..2).map(|index| {
                assert_eq!(iterator_store.get_integer("App", "answer").unwrap(), 42);
                format!("item-{index}")
            });
            let result = iterator_store.set_string_vec("App", "items", values);
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("iterator should finish outside the write lock")?;
        assert_eq!(
            store.get_string_vec("App", "items")?,
            vec![String::from("item-0"), String::from("item-1")]
        );
        Ok(())
    }

    #[test]
    fn typed_setters_consume_inputs_before_rejecting_invalid_sections() -> Result<()> {
        let store = ConfigStore::from_str("App = 7\n[Other]\nkeep = true")?;
        let before = store.save_str()?;
        let consumed = Cell::new(0);
        let values = (0_i64..3).inspect(|_| consumed.set(consumed.get() + 1));

        let error = store.set_integer_vec("App", "values", values).unwrap_err();
        assert!(matches!(
            error,
            Error::TypeMismatch {
                ref table_path,
                ref key,
                expected: "table",
                found: "integer",
                ..
            } if table_path.is_empty() && key == "App"
        ));
        assert_eq!(consumed.get(), 3);
        assert_eq!(store.save_str()?, before);
        Ok(())
    }

    #[test]
    fn datetime_fallback_from_string() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("Build", "time", "2024-01-01T00:00:00Z")?;
        let dt = store.get_datetime("Build", "time")?;
        assert_eq!(dt.to_string(), "2024-01-01T00:00:00Z");
        Ok(())
    }

    #[test]
    fn float_vec_accepts_integers() -> Result<()> {
        let store = ConfigStore::new();
        store.set_value(
            "App",
            "thresholds",
            Value::Array(vec![Value::Integer(1), Value::Float(2.5)]),
        )?;
        let values = store.get_float_vec("App", "thresholds")?;
        assert_eq!(values, vec![1.0, 2.5]);
        Ok(())
    }

    #[test]
    fn scalar_array_type_and_datetime_errors_include_the_index() -> Result<()> {
        let store = ConfigStore::from_str(
            "[App]\nports = [1, true]\ntimes = [\"2024-01-01\", \"not-a-date\"]",
        )?;

        let error = store.get_integer_vec("App", "ports").unwrap_err();
        assert!(matches!(
            error,
            Error::ValueParse { ref table_path, ref key, ref message }
                if table_path == "App"
                    && key == "ports"
                    && message == "expected array elements of type integer, found boolean (at index 1)"
        ));

        let error = store.get_datetime_vec("App", "times").unwrap_err();
        assert!(matches!(
            error,
            Error::ValueParse { ref table_path, ref key, ref message }
                if table_path == "App"
                    && key == "times"
                    && message.ends_with("(at index 1)")
        ));
        Ok(())
    }

    #[test]
    fn missing_key_yields_error() {
        let store = ConfigStore::new();
        let err = store.get_string("Missing", "key").unwrap_err();
        assert!(matches!(err, Error::MissingKey { .. }));
    }

    #[test]
    fn load_str_replaces_content() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("App", "name", "demo")?;
        store.load_str("[Srv]\nport = 8080\n")?;

        assert!(store.get_value("App", "name").is_none());
        assert_eq!(store.get_integer("Srv", "port")?, 8080);
        Ok(())
    }

    #[test]
    fn list_keys_missing_section_empty() -> Result<()> {
        let store = ConfigStore::new();
        assert!(store.list_keys("Unknown")?.is_empty());
        Ok(())
    }

    #[test]
    fn ensure_section_is_idempotent() -> Result<()> {
        let store = ConfigStore::new();
        store.ensure_section("Srv")?;
        store.set_integer("Srv", "port", 1)?;
        store.ensure_section("Srv")?;
        assert_eq!(store.get_integer("Srv", "port")?, 1);
        Ok(())
    }

    #[test]
    fn remove_key_missing_is_ok() -> Result<()> {
        let store = ConfigStore::new();
        store.remove_key("Srv", "unknown")?;
        store.ensure_section("Srv")?;
        store.remove_key("Srv", "missing")?;
        Ok(())
    }

    #[tokio::test]
    async fn save_and_load_file_roundtrip() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("App", "name", "demo")?;
        store.set_integer("App", "port", 3000)?;

        let temp = NamedTempFile::new().expect("temp file");
        let path_buf = temp.path().to_path_buf();
        store.save_file(&path_buf).await?;

        let restored = ConfigStore::from_file(&path_buf).await?;
        assert_eq!(restored.get_string("App", "name")?, "demo");
        assert_eq!(restored.get_integer("App", "port")?, 3000);
        Ok(())
    }

    #[tokio::test]
    async fn save_serialization_error_carries_the_target_path() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        let store = ConfigStore::new();
        store.set_datetime(
            "App",
            "invalid",
            Datetime {
                date: None,
                time: None,
                offset: None,
            },
        )?;

        let error = store.save_file(&path).await.unwrap_err();
        assert!(matches!(
            &error,
            Error::Serialize {
                target_path: Some(target_path),
                ..
            } if target_path == &path
        ));
        assert_eq!(
            error.to_string(),
            format!("failed to serialize TOML for {path:?}")
        );
        assert!(error.source().is_some());
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn save_file_requires_a_runtime_only_after_serialization() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "original")?;

        let invalid = ConfigStore::new();
        invalid.set_datetime(
            "App",
            "invalid",
            Datetime {
                date: None,
                time: None,
                offset: None,
            },
        )?;
        let mut serialization = Box::pin(invalid.save_file(&path));
        assert!(matches!(
            poll_without_runtime(serialization.as_mut()),
            Poll::Ready(Err(Error::Serialize {
                target_path: Some(target_path),
                ..
            })) if target_path == path
        ));
        assert_eq!(std::fs::read_to_string(&path)?, "original");

        let valid = ConfigStore::from_str("[App]\nport = 80")?;
        let mut file_operation = Box::pin(valid.save_file(&path));
        assert!(
            catch_unwind(AssertUnwindSafe(|| poll_without_runtime(
                file_operation.as_mut()
            )))
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(&path)?, "original");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn file_methods_work_on_a_current_thread_runtime() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[App]\nport = 80")?;

        let store = ConfigStore::from_file(&path).await?;
        store.set_integer("App", "port", 8080)?;
        store.save_file(&path).await?;

        let reloaded = ConfigStore::new();
        reloaded.load_file(&path).await?;
        assert_eq!(reloaded.get_integer("App", "port")?, 8080);
        Ok(())
    }

    #[tokio::test]
    async fn save_file_overwrites_existing() -> Result<()> {
        let store = ConfigStore::new();
        store.set_boolean("Flags", "enabled", true)?;
        let temp = NamedTempFile::new().expect("temp file");
        let path_buf = temp.path().to_path_buf();
        store.save_file(&path_buf).await?;

        // Write again with different content to ensure overwrite
        store.set_boolean("Flags", "enabled", false)?;
        store.save_file(&path_buf).await?;

        let file_contents = tokio::fs::read_to_string(&path_buf).await?;
        assert!(file_contents.contains("enabled = false"));
        Ok(())
    }

    #[test]
    fn publication_error_does_not_remove_or_retry_target() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "original")?;
        let mut attempts = 0;
        let mut temporary_path = None;

        let error = write_atomic_blocking_with(
            &path,
            b"replacement",
            std::fs::symlink_metadata(&path),
            prepare_temporary,
            Write::write_all,
            |from, to| {
                attempts += 1;
                assert_eq!(to, path);
                assert_eq!(std::fs::read(from)?, b"replacement");
                temporary_path = Some(from.to_path_buf());
                Err(std::io::Error::new(
                    ErrorKind::AlreadyExists,
                    "rename failed",
                ))
            },
        )
        .unwrap_err();

        assert_eq!(attempts, 1);
        assert!(matches!(
            &error,
            Error::Io {
                path: Some(error_path),
                operation: Some(IoOperation::ReplaceTarget),
                target_path: Some(target_path),
                ..
            } if error_path == &path && target_path == &path
        ));
        match error {
            Error::Io {
                path: error_path,
                source,
                operation: Some(IoOperation::ReplaceTarget),
                target_path: Some(target_path),
                ..
            } => {
                assert_eq!(error_path.as_deref(), Some(path.as_path()));
                assert_eq!(target_path, path);
                assert_eq!(source.kind(), ErrorKind::AlreadyExists);
                assert_eq!(source.to_string(), "rename failed");
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(std::fs::read(&path)?, b"original");
        assert!(!temporary_path.expect("rename was called").exists());
        Ok(())
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn save_file_reports_windows_sharing_conflict() -> Result<()> {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "original")?;
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x00000003) // Share reads and writes, but not deletion.
            .open(&path)?;
        let store = ConfigStore::new();
        store.set_integer("App", "value", 42)?;

        match store.save_file(&path).await.unwrap_err() {
            Error::Io {
                path: error_path,
                source,
                ..
            } => {
                assert_eq!(error_path.as_deref(), Some(path.as_path()));
                assert!(source.raw_os_error().is_some());
            }
            other => panic!("unexpected error: {other}"),
        }
        assert_eq!(std::fs::read(&path)?, b"original");
        drop(locked);
        let entries: Vec<_> = std::fs::read_dir(dir.path())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<_>>()?;
        assert_eq!(entries, ["config.toml"]);
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_reader_sees_complete_replacements() -> Result<()> {
        use std::io::Read;
        use std::sync::atomic::AtomicBool;
        use std::sync::mpsc::{channel, sync_channel};
        use std::time::Duration;

        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        let mut stores = Vec::new();
        let mut snapshots = Vec::new();
        for value in ["a", "b"] {
            let store = ConfigStore::new();
            store.set_string("App", "payload", value.repeat(64 * 1024))?;
            snapshots.push(store.save_str()?.into_bytes());
            stores.push(store);
        }
        std::fs::write(&path, &snapshots[0])?;

        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = Arc::clone(&stop);
        let reader_path = path.clone();
        let (ready_tx, ready_rx) = sync_channel(0);
        let (resume_tx, resume_rx) = channel();
        let reader = std::thread::spawn(move || {
            let mut reads = 0;
            while !reader_stop.load(Ordering::Acquire) {
                let mut options = std::fs::OpenOptions::new();
                options.read(true);
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    options.share_mode(0x00000007); // Allow replacement while reading.
                }
                let mut file = options.open(&reader_path)?;
                let mut contents = Vec::new();
                file.read_to_end(&mut contents)?;
                assert!(
                    snapshots.contains(&contents),
                    "reader saw a partial snapshot"
                );
                reads += 1;
                if reads == 2 {
                    assert_eq!(
                        contents, snapshots[1],
                        "reader missed the first replacement"
                    );
                }
                if reads <= 2 && ready_tx.send(()).is_err() {
                    break;
                }
                if reads == 1 && resume_rx.recv().is_err() {
                    break;
                }
            }
            Ok::<_, std::io::Error>(reads)
        });

        let result: Result<()> = async {
            ready_rx
                .recv_timeout(Duration::from_secs(10))
                .expect("reader did not start");
            for index in 0..100 {
                stores[(index + 1) % stores.len()].save_file(&path).await?;
                if index == 0 {
                    resume_tx.send(()).expect("reader stopped");
                    ready_rx
                        .recv_timeout(Duration::from_secs(10))
                        .expect("reader did not run during saves");
                }
            }
            Ok(())
        }
        .await;
        stop.store(true, Ordering::Release);
        drop(ready_rx);
        drop(resume_tx);
        let reads = reader.join().expect("reader failed")?;
        result?;
        assert!(reads >= 2);
        Ok(())
    }

    #[tokio::test]
    async fn save_file_supports_long_target_filenames() -> Result<()> {
        let dir = tempfile::tempdir().expect("temp dir");
        let filename = format!("{}.toml", "a".repeat(245));
        let path = dir.path().join(filename);
        std::fs::write(&path, "original = true")?;

        let store = ConfigStore::new();
        store.set_integer("App", "value", 42)?;
        store.save_file(&path).await?;

        let saved = std::fs::read_to_string(&path)?;
        assert!(saved.contains("value = 42"));
        let entries: Vec<_> = std::fs::read_dir(dir.path())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<_>>()?;
        assert_eq!(entries, [path.file_name().unwrap()]);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_saves_install_complete_snapshots() -> Result<()> {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");

        let mut snapshots = Vec::new();
        let mut tasks = Vec::new();
        for writer in 0..4_i64 {
            let store = ConfigStore::new();
            store.set_integer("App", "writer", writer)?;
            store.set_string("App", "payload", "x".repeat(64 * 1024))?;
            snapshots.push(store.save_str()?);

            let path = path.clone();
            tasks.push(tokio::spawn(async move {
                for _ in 0..50 {
                    store.save_file(&path).await?;
                }
                Ok::<(), Error>(())
            }));
        }
        for task in tasks {
            task.await.expect("save task panicked")?;
        }

        let saved = std::fs::read_to_string(&path)?;
        assert!(snapshots.contains(&saved), "the file mixes several saves");
        let entries: Vec<_> = std::fs::read_dir(dir.path())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<_>>()?;
        assert_eq!(entries, ["config.toml"], "temporary files were left behind");
        Ok(())
    }
}
