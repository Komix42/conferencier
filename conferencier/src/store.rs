use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::fs;
use tokio::sync::RwLock;
use toml::value::Datetime;
use toml::{Table, Value};

use crate::error::{ConferError, Result};
use crate::value_conversion;

/// In-memory TOML-backed configuration store guarded by an asynchronous `RwLock`.
#[derive(Debug, Default)]
pub struct Confer {
    table: RwLock<Table>,
}

/// Shared reference-counted handle to a [`Confer`] instance.
pub type SharedConfer = Arc<Confer>;

impl Confer {
    /// Creates an empty configuration store wrapped in [`SharedConfer`].
    pub fn new() -> SharedConfer {
        Arc::new(Self::default())
    }

    /// Builds a store from a TOML string, returning a shared handle on success.
    pub fn from_string(source: &str) -> Result<SharedConfer> {
        let table = Self::parse_table(source)?;
        Ok(Arc::new(Self {
            table: RwLock::new(table),
        }))
    }

    /// Synchronously reads a TOML file from disk and constructs the shared store.
    pub fn from_file(path: impl AsRef<Path>) -> Result<SharedConfer> {
        let path_buf = path.as_ref().to_path_buf();
        let contents = std::fs::read_to_string(&path_buf)
            .map_err(|err| ConferError::io_error(Some(path_buf.clone()), err))?;
        let table = Self::parse_table(&contents)?;
        Ok(Arc::new(Self {
            table: RwLock::new(table),
        }))
    }

    /// Asynchronously reads a TOML file from disk and constructs the shared store.
    pub async fn from_file_async(path: impl AsRef<Path> + Send + Sync) -> Result<SharedConfer> {
        let path_buf = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path_buf)
            .await
            .map_err(|err| ConferError::io_error(Some(path_buf.clone()), err))?;
        let table = Self::parse_table(&contents)?;
        Ok(Arc::new(Self {
            table: RwLock::new(table),
        }))
    }

    /// Replaces the in-memory table with the contents of the provided TOML string.
    pub async fn load_str(&self, source: &str) -> Result<()> {
        let table = Self::parse_table(source)?;
        let mut guard = self.table.write().await;
        *guard = table;
        Ok(())
    }

    /// Replaces the in-memory table with the contents of the TOML file at `path`.
    pub async fn load_file(&self, path: impl AsRef<Path> + Send + Sync) -> Result<()> {
        let path_buf = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path_buf)
            .await
            .map_err(|err| ConferError::io_error(Some(path_buf.clone()), err))?;
        self.load_str(&contents).await
    }

    /// Serializes the current table to a TOML string.
    pub async fn save_str(&self) -> Result<String> {
        let guard = self.table.read().await;
        toml::to_string(&*guard).map_err(ConferError::from)
    }

    /// Serializes the current table and writes it atomically to the specified file.
    pub async fn save_file(&self, path: impl AsRef<Path> + Send + Sync) -> Result<()> {
        let path_buf = path.as_ref().to_path_buf();
        let serialized = self.save_str().await?;
        write_atomic(&path_buf, serialized.as_bytes()).await
    }

    /// Returns the raw TOML value stored under `section.key`, if present.
    pub async fn get_value(&self, section: &str, key: &str) -> Option<Value> {
        let guard = self.table.read().await;
        section_table(&guard, section)
            .and_then(|table| table.get(key).cloned())
    }

    /// Returns a cloned snapshot of the table stored at `section`, if it exists.
    pub async fn get_section_table(&self, section: &str) -> Option<Table> {
        let guard = self.table.read().await;
        section_table(&guard, section).map(|table| table.clone())
    }

    /// Inserts `value` at `section.key`, creating the section if necessary.
    pub async fn set_value(&self, section: &str, key: &str, value: Value) -> Result<()> {
        let mut guard = self.table.write().await;
        match guard.entry(section.to_owned()) {
            toml::map::Entry::Occupied(mut entry) => {
                if let Value::Table(inner) = entry.get_mut() {
                    inner.insert(key.to_owned(), value);
                    Ok(())
                } else {
                    Err(ConferError::type_mismatch(
                        section,
                        "<section>",
                        "table",
                        value_conversion::describe(entry.get()),
                    ))
                }
            }
            toml::map::Entry::Vacant(entry) => {
                let mut table = Table::new();
                table.insert(key.to_owned(), value);
                entry.insert(Value::Table(table));
                Ok(())
            }
        }
    }

    /// Returns `true` when the store contains a table for `section`.
    pub async fn section_exists(&self, section: &str) -> bool {
        let guard = self.table.read().await;
        matches!(guard.get(section), Some(Value::Table(_)))
    }

    /// Ensures that `section` exists as an empty table, returning an error on type mismatch.
    pub async fn add_section(&self, section: &str) -> Result<()> {
        let mut guard = self.table.write().await;
        match guard.entry(section.to_owned()) {
            toml::map::Entry::Occupied(entry) => {
                if entry.get().is_table() {
                    Ok(())
                } else {
                    Err(ConferError::type_mismatch(
                        section,
                        "<section>",
                        "table",
                        value_conversion::describe(entry.get()),
                    ))
                }
            }
            toml::map::Entry::Vacant(entry) => {
                entry.insert(Value::Table(Table::new()));
                Ok(())
            }
        }
    }

    /// Removes `key` from `section`, ignoring missing keys or sections.
    pub async fn remove_key(&self, section: &str, key: &str) -> Result<()> {
        let mut guard = self.table.write().await;
        if let Some(value) = guard.get_mut(section) {
            if let Value::Table(inner) = value {
                inner.remove(key);
                Ok(())
            } else {
                Err(ConferError::type_mismatch(
                    section,
                    "<section>",
                    "table",
                    value_conversion::describe(value),
                ))
            }
        } else {
            Ok(())
        }
    }

    /// Removes `section` from the store, ignoring missing sections.
    pub async fn remove_section(&self, section: &str) -> Result<()> {
        let mut guard = self.table.write().await;
        guard.remove(section);
        Ok(())
    }

    /// Lists all sections currently backed by a TOML table.
    pub async fn list_sections(&self) -> Vec<String> {
        let guard = self.table.read().await;
        guard
            .iter()
            .filter_map(|(name, value)| value.as_table().map(|_| name.clone()))
            .collect()
    }

    /// Lists the keys contained in `section`, or an empty vector when the section is absent.
    pub async fn list_keys(&self, section: &str) -> Result<Vec<String>> {
        let guard = self.table.read().await;
        match section_table(&guard, section) {
            Some(table) => Ok(table.keys().cloned().collect()),
            None => {
                if guard.contains_key(section) {
                    Err(ConferError::type_mismatch(
                        section,
                        "<section>",
                        "table",
                        value_conversion::describe(guard.get(section).unwrap()),
                    ))
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    /// Retrieves a string value stored at `section.key`.
    pub async fn get_string(&self, section: &str, key: &str) -> Result<String> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::string(section, key, value)
    }

    /// Retrieves an integer value stored at `section.key`.
    pub async fn get_integer(&self, section: &str, key: &str) -> Result<i64> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::integer(section, key, value)
    }

    /// Retrieves a floating-point value stored at `section.key`.
    pub async fn get_float(&self, section: &str, key: &str) -> Result<f64> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::float(section, key, value)
    }

    /// Retrieves a boolean value stored at `section.key`.
    pub async fn get_boolean(&self, section: &str, key: &str) -> Result<bool> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::boolean(section, key, value)
    }

    /// Retrieves a [`Datetime`] value stored at `section.key`, parsing strings when necessary.
    pub async fn get_datetime(
        &self,
        section: &str,
        key: &str,
    ) -> Result<Datetime> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::datetime(section, key, value)
    }

    /// Retrieves a string array stored at `section.key`.
    pub async fn get_string_vec(&self, section: &str, key: &str) -> Result<Vec<String>> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::string_vec(section, key, value)
    }

    /// Retrieves an integer array stored at `section.key`.
    pub async fn get_integer_vec(&self, section: &str, key: &str) -> Result<Vec<i64>> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::integer_vec(section, key, value)
    }

    /// Retrieves a floating-point array stored at `section.key`.
    pub async fn get_float_vec(&self, section: &str, key: &str) -> Result<Vec<f64>> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::float_vec(section, key, value)
    }

    /// Retrieves a boolean array stored at `section.key`.
    pub async fn get_boolean_vec(&self, section: &str, key: &str) -> Result<Vec<bool>> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::boolean_vec(section, key, value)
    }

    /// Retrieves a [`Datetime`] array stored at `section.key`, parsing string values when necessary.
    pub async fn get_datetime_vec(
        &self,
        section: &str,
        key: &str,
    ) -> Result<Vec<Datetime>> {
        let value = self.fetch_value(section, key).await?;
        value_conversion::datetime_vec(section, key, value)
    }

    /// Stores a string at `section.key`, creating the section if needed.
    pub async fn set_string(&self, section: &str, key: &str, value: String) -> Result<()> {
        self.set_value(section, key, Value::String(value)).await
    }

    /// Stores an integer at `section.key`, creating the section if needed.
    pub async fn set_integer(&self, section: &str, key: &str, value: i64) -> Result<()> {
        self.set_value(section, key, Value::Integer(value)).await
    }

    /// Stores a floating-point number at `section.key`, creating the section if needed.
    pub async fn set_float(&self, section: &str, key: &str, value: f64) -> Result<()> {
        self.set_value(section, key, Value::Float(value)).await
    }

    /// Stores a boolean at `section.key`, creating the section if needed.
    pub async fn set_boolean(&self, section: &str, key: &str, value: bool) -> Result<()> {
        self.set_value(section, key, Value::Boolean(value)).await
    }

    /// Stores a TOML [`Datetime`] at `section.key`, creating the section if needed.
    pub async fn set_datetime(
        &self,
        section: &str,
        key: &str,
        value: Datetime,
    ) -> Result<()> {
        self.set_value(section, key, Value::Datetime(value)).await
    }

    /// Stores a string array at `section.key`, creating the section if needed.
    pub async fn set_string_vec(
        &self,
        section: &str,
        key: &str,
        value: Vec<String>,
    ) -> Result<()> {
        let array = value.into_iter().map(Value::String).collect();
        self.set_value(section, key, Value::Array(array)).await
    }

    /// Stores an integer array at `section.key`, creating the section if needed.
    pub async fn set_integer_vec(
        &self,
        section: &str,
        key: &str,
        value: Vec<i64>,
    ) -> Result<()> {
        let array = value.into_iter().map(Value::Integer).collect();
        self.set_value(section, key, Value::Array(array)).await
    }

    /// Stores a floating-point array at `section.key`, creating the section if needed.
    pub async fn set_float_vec(
        &self,
        section: &str,
        key: &str,
        value: Vec<f64>,
    ) -> Result<()> {
        let array = value.into_iter().map(Value::Float).collect();
        self.set_value(section, key, Value::Array(array)).await
    }

    /// Stores a boolean array at `section.key`, creating the section if needed.
    pub async fn set_boolean_vec(
        &self,
        section: &str,
        key: &str,
        value: Vec<bool>,
    ) -> Result<()> {
        let array = value.into_iter().map(Value::Boolean).collect();
        self.set_value(section, key, Value::Array(array)).await
    }

    /// Stores a [`Datetime`] array at `section.key`, creating the section if needed.
    pub async fn set_datetime_vec(
        &self,
        section: &str,
        key: &str,
        value: Vec<Datetime>,
    ) -> Result<()> {
        let array = value.into_iter().map(Value::Datetime).collect();
        self.set_value(section, key, Value::Array(array)).await
    }

    /// Fetches the raw TOML [`Value`] stored at `section.key`, producing detailed errors.
    async fn fetch_value(&self, section: &str, key: &str) -> Result<Value> {
        let guard = self.table.read().await;
        let section_value = guard
            .get(section)
            .ok_or_else(|| ConferError::missing_key(section, key))?;
        let table = section_value.as_table().ok_or_else(|| {
            ConferError::type_mismatch(
                section,
                "<section>",
                "table",
                value_conversion::describe(section_value),
            )
        })?;
        table
            .get(key)
            .cloned()
            .ok_or_else(|| ConferError::missing_key(section, key))
    }

    /// Parses a TOML table from `source`, mapping parsing failures into [`ConferError`].
    fn parse_table(source: &str) -> Result<Table> {
        toml::from_str(source).map_err(ConferError::from)
    }
}

/// Retrieves the table stored within `root` at `section`, if it exists and is a table.
fn section_table<'a>(root: &'a Table, section: &str) -> Option<&'a Table> {
    root.get(section)?.as_table()
}

/// Atomically persists `contents` to `path`, ensuring the file is fully replaced on success.
async fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let tmp_path = temporary_path(path);
    fs::write(&tmp_path, contents)
        .await
        .map_err(|err| ConferError::io_error(Some(tmp_path.clone()), err))?;

    match fs::rename(&tmp_path, path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            fs::remove_file(path)
                .await
                .map_err(|remove_err| ConferError::io_error(Some(path.to_path_buf()), remove_err))?;
            fs::rename(&tmp_path, path)
                .await
                .map_err(|err| ConferError::io_error(Some(path.to_path_buf()), err))
        }
        Err(err) => {
            let _ = fs::remove_file(&tmp_path).await;
            Err(ConferError::io_error(Some(path.to_path_buf()), err))
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
    use crate::error::{ConferError, Result};

    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn new_store_is_empty() {
        let store = Confer::new();
        assert!(store.list_sections().await.is_empty());
    }

    #[tokio::test]
    async fn set_and_get_string_roundtrip() -> Result<()> {
        let store = Confer::new();
        store.set_string("App", "name", "demo".into()).await?;
        assert_eq!(store.get_string("App", "name").await?, "demo");
        assert_eq!(
            store.get_value("App", "name").await,
            Some(Value::String("demo".into()))
        );
        Ok(())
    }

    #[tokio::test]
    async fn datetime_fallback_from_string() -> Result<()> {
        let store = Confer::new();
        store
            .set_string("Build", "time", "2024-01-01T00:00:00Z".into())
            .await?;
        let dt = store.get_datetime("Build", "time").await?;
        assert_eq!(dt.to_string(), "2024-01-01T00:00:00Z");
        Ok(())
    }

    #[tokio::test]
    async fn float_vec_accepts_integers() -> Result<()> {
        let store = Confer::new();
        store
            .set_value(
                "App",
                "thresholds",
                Value::Array(vec![Value::Integer(1), Value::Float(2.5)]),
            )
            .await?;
        let values = store.get_float_vec("App", "thresholds").await?;
        assert_eq!(values, vec![1.0, 2.5]);
        Ok(())
    }

    #[tokio::test]
    async fn missing_key_yields_error() {
        let store = Confer::new();
        let err = store.get_string("Missing", "key").await.unwrap_err();
        assert!(matches!(err, ConferError::MissingKey { .. }));
    }

    #[tokio::test]
    async fn load_str_replaces_content() -> Result<()> {
        let store = Confer::new();
        store.set_string("App", "name", "demo".into()).await?;
        store.load_str("[Srv]\nport = 8080\n").await?;

        assert!(store.get_value("App", "name").await.is_none());
        assert_eq!(store.get_integer("Srv", "port").await?, 8080);
        Ok(())
    }

    #[tokio::test]
    async fn list_keys_missing_section_empty() -> Result<()> {
        let store = Confer::new();
        assert!(store.list_keys("Unknown").await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn add_section_is_idempotent() -> Result<()> {
        let store = Confer::new();
        store.add_section("Srv").await?;
        store.add_section("Srv").await?;
        assert!(store.section_exists("Srv").await);
        Ok(())
    }

    #[tokio::test]
    async fn remove_key_missing_is_ok() -> Result<()> {
        let store = Confer::new();
        store.remove_key("Srv", "unknown").await?;
        store.add_section("Srv").await?;
        store.remove_key("Srv", "missing").await?;
        Ok(())
    }

    #[tokio::test]
    async fn save_and_load_file_roundtrip() -> Result<()> {
        let store = Confer::new();
        store.set_string("App", "name", "demo".into()).await?;
        store.set_integer("App", "port", 3000).await?;

        let temp = NamedTempFile::new().expect("temp file");
        let path_buf = temp.path().to_path_buf();
        store.save_file(&path_buf).await?;

        let restored = Confer::from_file(&path_buf)?;
        assert_eq!(restored.get_string("App", "name").await?, "demo");
        assert_eq!(restored.get_integer("App", "port").await?, 3000);
        Ok(())
    }

    #[tokio::test]
    async fn save_file_overwrites_existing() -> Result<()> {
        let store = Confer::new();
        store.set_boolean("Flags", "enabled", true).await?;
        let temp = NamedTempFile::new().expect("temp file");
        let path_buf = temp.path().to_path_buf();
        store.save_file(&path_buf).await?;

        // Write again with different content to ensure overwrite
        store.set_boolean("Flags", "enabled", false).await?;
        store.save_file(&path_buf).await?;

        let file_contents = tokio::fs::read_to_string(&path_buf).await?;
        assert!(file_contents.contains("enabled = false"));
        Ok(())
    }
}
