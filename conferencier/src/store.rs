use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use tokio::fs;
use toml::value::Datetime;
use toml::{Table, Value};

use crate::error::{Error, Result};
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
/// and [`save_file`](Self::save_file) use [`tokio::fs`]. Every other method works only on
/// the in-memory document and is synchronous, so it can also be called before the runtime
/// starts or from synchronous code on a runtime thread.
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
///
/// # Locking
///
/// The document is guarded by a [`std::sync::RwLock`]. The lock is held only for the
/// operation on the in-memory document: never across an `.await`, never during file I/O
/// and never while user code runs, and the store hands out no guards. Calling the store
/// again, also from another task, therefore cannot deadlock on it. If a panic ever poisons
/// the lock, later calls continue with the data, which every write leaves consistent.
///
/// The design assumes relatively small documents and low contention, in line with
/// [Tokio's guidance on synchronous locks](https://tokio.rs/tokio/tutorial/shared-state#on-using-stdsyncmutex-and-tokiosyncmutex).
/// Work under the lock is not constant-time: cloning a section and serializing the document
/// scale with their contents, and a contended acquisition blocks the worker thread. Unlike
/// Tokio's `RwLock`, the standard lock gives no reader/writer priority guarantee; its policy
/// depends on the platform. Should real workloads exceed these assumptions, the
/// implementation and possibly the API have to be reevaluated based on measurements:
/// switching to an async lock would not be a transparent backend change, because acquiring
/// it has to be awaited and it would not offload the CPU work of serialization.
#[derive(Clone, Debug, Default)]
pub struct ConfigStore {
    table: Arc<RwLock<Table>>,
}

impl ConfigStore {
    /// Creates an empty configuration store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a store from a TOML string.
    ///
    /// `source.parse::<ConfigStore>()` does the same through [`FromStr`].
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(source: &str) -> Result<Self> {
        Ok(Self::from_table(Self::parse_table(source)?))
    }

    /// Reads a TOML file asynchronously and builds a store from it.
    ///
    /// A failed read returns [`Error::Io`] carrying `path`.
    ///
    /// There is no blocking variant. Code that reads the configuration before the Tokio
    /// runtime starts can combine [`std::fs::read_to_string`] with
    /// [`from_str`](Self::from_str). The standard library's error does not carry the path;
    /// attach it through [`Error::Io`] if needed:
    ///
    /// ```no_run
    /// use conferencier::{ConfigStore, Error};
    ///
    /// # fn main() -> conferencier::Result<()> {
    /// let path = std::path::Path::new("config.toml");
    /// let source = std::fs::read_to_string(path).map_err(|source| Error::Io {
    ///     path: Some(path.to_path_buf()),
    ///     source,
    /// })?;
    /// let store = ConfigStore::from_str(&source)?;
    /// # let _ = store;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_file(path: impl AsRef<Path> + Send + Sync) -> Result<Self> {
        let contents = read_file(path.as_ref()).await?;
        Self::from_str(&contents)
    }

    /// Replaces the in-memory document with the contents of the provided TOML string.
    ///
    /// On a parse error the document stays unchanged.
    pub fn load_str(&self, source: &str) -> Result<()> {
        let table = Self::parse_table(source)?;
        *self.write() = table;
        Ok(())
    }

    /// Replaces the in-memory document with the contents of the TOML file at `path`.
    ///
    /// The file is read and parsed before the document is replaced; on error the document
    /// stays unchanged.
    pub async fn load_file(&self, path: impl AsRef<Path> + Send + Sync) -> Result<()> {
        let contents = read_file(path.as_ref()).await?;
        self.load_str(&contents)
    }

    /// Serializes the current document to a TOML string.
    pub fn save_str(&self) -> Result<String> {
        toml::to_string(&*self.read()).map_err(Error::from)
    }

    /// Serializes the current document and writes it atomically to the specified file.
    ///
    /// The document is serialized first; the lock is not held while the file is written.
    pub async fn save_file(&self, path: impl AsRef<Path> + Send + Sync) -> Result<()> {
        let serialized = self.save_str()?;
        write_atomic(path.as_ref(), serialized.as_bytes()).await
    }

    /// Returns the raw TOML value stored under `section.key`, if present.
    pub fn get_value(&self, section: &str, key: &str) -> Option<Value> {
        section_table(&self.read(), section)
            .and_then(|table| table.get(key))
            .cloned()
    }

    /// Returns a cloned snapshot of the table stored at `section`, if it exists.
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
    pub fn has_section(&self, section: &str) -> bool {
        matches!(self.read().get(section), Some(Value::Table(_)))
    }

    /// Creates `section` as an empty table if it is missing.
    ///
    /// An existing table is left unchanged, so the call is idempotent; unlike
    /// [`set_section`](Self::set_section), it never replaces a section. A non-table entry
    /// named `section` returns a type mismatch.
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
    pub fn remove_section(&self, section: &str) {
        self.write().remove(section);
    }

    /// Lists all sections currently backed by a TOML table.
    pub fn list_sections(&self) -> Vec<String> {
        self.read()
            .iter()
            .filter_map(|(name, value)| value.as_table().map(|_| name.clone()))
            .collect()
    }

    /// Lists the keys contained in `section`, or an empty vector when the section is absent.
    pub fn list_keys(&self, section: &str) -> Result<Vec<String>> {
        match self.read().get(section) {
            Some(Value::Table(table)) => Ok(table.keys().cloned().collect()),
            Some(other) => Err(section_mismatch(section, other)),
            None => Ok(Vec::new()),
        }
    }

    /// Retrieves a string value stored at `section.key`.
    pub fn get_string(&self, section: &str, key: &str) -> Result<String> {
        let value = self.fetch_value(section, key)?;
        value_conversion::string(section, key, value)
    }

    /// Retrieves an integer value stored at `section.key`.
    pub fn get_integer(&self, section: &str, key: &str) -> Result<i64> {
        let value = self.fetch_value(section, key)?;
        value_conversion::integer(section, key, value)
    }

    /// Retrieves a floating-point value stored at `section.key`.
    pub fn get_float(&self, section: &str, key: &str) -> Result<f64> {
        let value = self.fetch_value(section, key)?;
        value_conversion::float(section, key, value)
    }

    /// Retrieves a boolean value stored at `section.key`.
    pub fn get_boolean(&self, section: &str, key: &str) -> Result<bool> {
        let value = self.fetch_value(section, key)?;
        value_conversion::boolean(section, key, value)
    }

    /// Retrieves a [`Datetime`] value stored at `section.key`, parsing strings when necessary.
    pub fn get_datetime(&self, section: &str, key: &str) -> Result<Datetime> {
        let value = self.fetch_value(section, key)?;
        value_conversion::datetime(section, key, value)
    }

    /// Retrieves a string array stored at `section.key`.
    pub fn get_string_vec(&self, section: &str, key: &str) -> Result<Vec<String>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::string_vec(section, key, value)
    }

    /// Retrieves an integer array stored at `section.key`.
    pub fn get_integer_vec(&self, section: &str, key: &str) -> Result<Vec<i64>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::integer_vec(section, key, value)
    }

    /// Retrieves a floating-point array stored at `section.key`.
    pub fn get_float_vec(&self, section: &str, key: &str) -> Result<Vec<f64>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::float_vec(section, key, value)
    }

    /// Retrieves a boolean array stored at `section.key`.
    pub fn get_boolean_vec(&self, section: &str, key: &str) -> Result<Vec<bool>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::boolean_vec(section, key, value)
    }

    /// Retrieves a [`Datetime`] array stored at `section.key`, parsing string values when necessary.
    pub fn get_datetime_vec(&self, section: &str, key: &str) -> Result<Vec<Datetime>> {
        let value = self.fetch_value(section, key)?;
        value_conversion::datetime_vec(section, key, value)
    }

    /// Stores a string at `section.key`, creating the section if needed.
    pub fn set_string(&self, section: &str, key: &str, value: String) -> Result<()> {
        self.set_value(section, key, Value::String(value))
    }

    /// Stores an integer at `section.key`, creating the section if needed.
    pub fn set_integer(&self, section: &str, key: &str, value: i64) -> Result<()> {
        self.set_value(section, key, Value::Integer(value))
    }

    /// Stores a floating-point number at `section.key`, creating the section if needed.
    pub fn set_float(&self, section: &str, key: &str, value: f64) -> Result<()> {
        self.set_value(section, key, Value::Float(value))
    }

    /// Stores a boolean at `section.key`, creating the section if needed.
    pub fn set_boolean(&self, section: &str, key: &str, value: bool) -> Result<()> {
        self.set_value(section, key, Value::Boolean(value))
    }

    /// Stores a TOML [`Datetime`] at `section.key`, creating the section if needed.
    pub fn set_datetime(&self, section: &str, key: &str, value: Datetime) -> Result<()> {
        self.set_value(section, key, Value::Datetime(value))
    }

    /// Stores a string array at `section.key`, creating the section if needed.
    pub fn set_string_vec(&self, section: &str, key: &str, value: Vec<String>) -> Result<()> {
        let array = value.into_iter().map(Value::String).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores an integer array at `section.key`, creating the section if needed.
    pub fn set_integer_vec(&self, section: &str, key: &str, value: Vec<i64>) -> Result<()> {
        let array = value.into_iter().map(Value::Integer).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores a floating-point array at `section.key`, creating the section if needed.
    pub fn set_float_vec(&self, section: &str, key: &str, value: Vec<f64>) -> Result<()> {
        let array = value.into_iter().map(Value::Float).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores a boolean array at `section.key`, creating the section if needed.
    pub fn set_boolean_vec(&self, section: &str, key: &str, value: Vec<bool>) -> Result<()> {
        let array = value.into_iter().map(Value::Boolean).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Stores a [`Datetime`] array at `section.key`, creating the section if needed.
    pub fn set_datetime_vec(&self, section: &str, key: &str, value: Vec<Datetime>) -> Result<()> {
        let array = value.into_iter().map(Value::Datetime).collect();
        self.set_value(section, key, Value::Array(array))
    }

    /// Fetches the raw TOML [`Value`] stored at `section.key`, producing detailed errors.
    fn fetch_value(&self, section: &str, key: &str) -> Result<Value> {
        let guard = self.read();
        let section_value = guard
            .get(section)
            .ok_or_else(|| Error::missing_key(section, key))?;
        let table = section_value
            .as_table()
            .ok_or_else(|| section_mismatch(section, section_value))?;
        table
            .get(key)
            .cloned()
            .ok_or_else(|| Error::missing_key(section, key))
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
    Error::type_mismatch(
        section,
        "<section>",
        "table",
        value_conversion::describe(found),
    )
}

/// Reads the file at `path`, attaching the path to I/O errors.
async fn read_file(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .await
        .map_err(|err| Error::io_error(Some(path.to_path_buf()), err))
}

/// Atomically persists `contents` to `path`, ensuring the file is fully replaced on success.
async fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let tmp_path = temporary_path(path);
    fs::write(&tmp_path, contents)
        .await
        .map_err(|err| Error::io_error(Some(tmp_path.clone()), err))?;

    match fs::rename(&tmp_path, path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            fs::remove_file(path)
                .await
                .map_err(|remove_err| Error::io_error(Some(path.to_path_buf()), remove_err))?;
            fs::rename(&tmp_path, path)
                .await
                .map_err(|err| Error::io_error(Some(path.to_path_buf()), err))
        }
        Err(err) => {
            let _ = fs::remove_file(&tmp_path).await;
            Err(Error::io_error(Some(path.to_path_buf()), err))
        }
    }
}

/// Computes a temporary sibling path used during atomic write operations.
fn temporary_path(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "conferencier".into());
    file_name.push(".tmp");

    path.with_file_name(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Error, Result};

    use tempfile::NamedTempFile;

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
        assert!(matches!(inherent, Error::Parse(_)));
        assert_eq!(inherent.to_string(), parsed.to_string());
        Ok(())
    }

    #[tokio::test]
    async fn from_file_reports_missing_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("missing.toml");
        let error = ConfigStore::from_file(&path).await.unwrap_err();
        assert!(matches!(
            error,
            Error::Io { path: Some(ref reported), .. } if *reported == path
        ));
    }

    #[test]
    fn poisoned_lock_keeps_working() -> Result<()> {
        let store = ConfigStore::from_str("[App]\nport = 1")?;
        let shared = store.clone();
        let _ = std::thread::spawn(move || {
            let _guard = shared.write();
            panic!("poison the store lock");
        })
        .join();
        assert!(store.table.is_poisoned());

        store.set_integer("App", "port", 2)?;
        assert_eq!(store.get_integer("App", "port")?, 2);
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
                Error::TypeMismatch { section, key, expected: "table", found: "integer" }
                    if section == "App" && key == "<section>"
            ));
            assert_eq!(
                error.to_string(),
                "expected table at App.<section> but found integer"
            );
        }
        assert_eq!(store.save_str()?, before);
        Ok(())
    }

    #[test]
    fn set_and_get_string_roundtrip() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("App", "name", "demo".into())?;
        assert_eq!(store.get_string("App", "name")?, "demo");
        assert_eq!(
            store.get_value("App", "name"),
            Some(Value::String("demo".into()))
        );
        Ok(())
    }

    #[test]
    fn datetime_fallback_from_string() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("Build", "time", "2024-01-01T00:00:00Z".into())?;
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
    fn missing_key_yields_error() {
        let store = ConfigStore::new();
        let err = store.get_string("Missing", "key").unwrap_err();
        assert!(matches!(err, Error::MissingKey { .. }));
    }

    #[test]
    fn load_str_replaces_content() -> Result<()> {
        let store = ConfigStore::new();
        store.set_string("App", "name", "demo".into())?;
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
        store.set_string("App", "name", "demo".into())?;
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
}
