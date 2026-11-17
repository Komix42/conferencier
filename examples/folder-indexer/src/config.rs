//! Configuration of the folder indexer.
//!
//! The application owns its configuration file: the first start creates it from
//! the defaults declared below, and console commands change it. Two components
//! each own one section of the shared `ConfigStore`: the scanner owns
//! `[Indexer]` and the report owns `[Report]`.

use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

use conferencier::{ConfigScope, ConfigStore, ConfigTable, SharedScope};

use crate::scan::{IndexedFile, SourceStats};

pub type BoxError = Box<dyn Error + Send + Sync>;

/// Settings of the scanner, the `[Indexer]` section.
///
/// Unknown keys are rejected, so a misspelled `include_hidden` (with an
/// underscore) fails loading instead of silently falling back to the default.
#[derive(ConfigScope)]
#[confer(section = "Indexer", deny_unknown_keys)]
pub struct IndexerConfig {
    /// Whether subfolders are indexed too.
    #[confer(default = true)]
    pub recursive: bool,
    /// `rename` maps the field to a kebab-case TOML key. `default` without a
    /// value uses `Default::default()`, here `false`.
    #[confer(rename = "include-hidden", default)]
    pub include_hidden: bool,
    /// The file extensions to index, without the dot. An `Option` needs no
    /// default: a missing key loads as `None`, which means all files, and saving
    /// `None` removes the key from the file.
    pub extensions: Option<Vec<String>>,
    /// The `[[Indexer.sources]]` array of tables. `default` makes the key
    /// optional: a missing key loads as an empty list, which is saved as
    /// `sources = []`.
    #[confer(table, default)]
    pub sources: Vec<Source>,
    /// Runtime results from the last successful scan; never written to TOML.
    #[confer(ignore = Vec::new())]
    pub files: Vec<IndexedFile>,
    /// Runtime per-source counts from the last successful scan.
    #[confer(ignore = Vec::new())]
    pub source_stats: Vec<SourceStats>,
    /// Distinguishes a completed scan with no sources from no scan yet.
    #[confer(ignore)]
    pub has_scanned: bool,
}

/// One folder to index, an element of `[[Indexer.sources]]`.
///
/// Strictness is a property of each type, not inherited from the scope, so the
/// table repeats `deny_unknown_keys`.
#[derive(Clone, ConfigTable)]
#[confer(deny_unknown_keys)]
pub struct Source {
    /// A unique name shown in the report.
    pub name: String,
    /// The folder, relative to the configuration file or absolute.
    pub path: String,
    /// Names of files and folders to skip, such as `drafts`; optional.
    pub exclude: Option<Vec<String>>,
}

/// Settings of the report, the `[Report]` section.
///
/// Unlike `[Indexer]`, the section is lenient: unknown keys are ignored when
/// loading. They stay in the file until the report settings are saved, because
/// saving a scope replaces its whole section.
#[derive(ConfigScope)]
#[confer(section = "Report")]
pub struct ReportConfig {
    /// The `[Report.summary]` table. With `field_defaults`, a missing table
    /// loads as an empty one, so each of its fields takes its own default, and
    /// saving writes the table with those values.
    #[confer(table, field_defaults)]
    pub summary: Summary,
    /// The optional `[Report.catalog]` table: without it, no catalog file is
    /// written. Saving `None` removes the table from the file.
    #[confer(table)]
    pub catalog: Option<CatalogFile>,
}

/// What the report prints, the `[Report.summary]` table.
#[derive(ConfigTable)]
pub struct Summary {
    /// How many of the largest files are listed; `0` lists none.
    #[confer(default = 5)]
    pub largest: u32,
    /// Whether the files are counted by extension.
    #[confer(rename = "by-extension", default = true)]
    pub by_extension: bool,
}

/// Where the catalog is written, the `[Report.catalog]` table.
///
/// It is strict although `[Report]` is not: strictness is not inherited in
/// either direction.
#[derive(Clone, ConfigTable)]
#[confer(deny_unknown_keys)]
pub struct CatalogFile {
    /// The JSON file, relative to the configuration file or absolute.
    pub path: String,
    /// Whether the JSON is indented for reading.
    #[confer(default = true)]
    pub pretty: bool,
}

/// The sections the scopes above own. Keep in sync with their `section` attributes.
const SECTIONS: [&str; 2] = ["Indexer", "Report"];

/// The configuration file, its store and the scopes of both components.
pub struct Settings {
    pub path: PathBuf,
    pub store: ConfigStore,
    pub indexer: SharedScope<IndexerConfig>,
    pub report: SharedScope<ReportConfig>,
}

impl Settings {
    /// Loads the configuration from `path`, or creates the file from the
    /// defaults if it does not exist. Returns whether the file was created.
    pub async fn open(path: PathBuf) -> Result<(Self, bool), BoxError> {
        let created = !tokio::fs::try_exists(&path)
            .await
            .map_err(|error| format!("cannot access {}: {error}", path.display()))?;
        // An empty store behaves like an empty file: each scope finds no
        // section and loads its defaults.
        let store = if created {
            ConfigStore::new()
        } else {
            ConfigStore::from_file(&path).await?
        };
        // `from_store` converts each section into its typed scope. Errors name
        // the failing key, such as `unknown key Indexer.sources[0].pth`.
        let indexer = IndexerConfig::from_store(&store)?;
        let report = ReportConfig::from_store(&store)?;
        validate(&*indexer.read().await, &*report.read().await)?;

        let settings = Self {
            path,
            store,
            indexer,
            report,
        };
        if created {
            // `save` copies the defaults from the scopes into the store, and
            // `save_file` writes the store, so the new file lists every setting.
            IndexerConfig::save(&settings.indexer, &settings.store).await?;
            ReportConfig::save(&settings.report, &settings.store).await?;
            settings.write_file().await?;
        }
        Ok((settings, created))
    }

    /// Top-level sections that no scope owns, such as a misspelled `[Reprot]`.
    ///
    /// A missing section loads as empty, so with a misspelled name the scope
    /// would silently use its defaults. Conferencier leaves this check to the
    /// application. Unexpected sections are not lost: the store keeps them and
    /// writes them back with the rest of the file.
    pub fn unexpected_sections(&self) -> Vec<String> {
        self.store
            .list_sections()
            .into_iter()
            .filter(|section| !SECTIONS.contains(&section.as_str()))
            .collect()
    }

    /// The configuration in use as TOML, including defaults of keys that are
    /// missing from the file.
    ///
    /// The scopes are saved into a new store, so neither the application's
    /// store nor the file changes.
    pub async fn effective_toml(&self) -> Result<String, BoxError> {
        let snapshot = ConfigStore::new();
        IndexerConfig::save(&self.indexer, &snapshot).await?;
        ReportConfig::save(&self.report, &snapshot).await?;
        Ok(snapshot.save_str()?)
    }

    /// Copies both scopes into the store, then writes the whole file.
    ///
    /// Scope saves replace only their own sections; the store save persists all
    /// sections together. This is called after each successful console edit.
    pub async fn save(&self) -> Result<(), BoxError> {
        let previous = self.store.save_str()?;
        let result = async {
            IndexerConfig::save(&self.indexer, &self.store).await?;
            ReportConfig::save(&self.report, &self.store).await?;
            self.write_file().await?;
            Ok::<(), BoxError>(())
        }
        .await;
        if let Err(error) = result {
            // Restore the in-memory store and both scopes if persistence failed.
            // Scope loads leave their ignored runtime state untouched.
            self.store.load_str(&previous)?;
            IndexerConfig::load(&self.indexer, &self.store).await?;
            ReportConfig::load(&self.report, &self.store).await?;
            return Err(error);
        }
        Ok(())
    }

    /// Writes the whole store to the configuration file.
    ///
    /// `save_file` replaces the file atomically but does not keep comments or
    /// layout, which is why this file belongs to the application rather than
    /// being written by hand.
    async fn write_file(&self) -> conferencier::Result<()> {
        self.store.save_file(&self.path).await
    }
}

/// Checks application rules that the types alone do not express.
///
/// Conferencier converts values to the field types but deliberately does not
/// validate application-specific rules, so the application checks them itself:
/// after loading the file, which may have been edited by hand, and before
/// applying a change made by a command.
pub fn validate(indexer: &IndexerConfig, report: &ReportConfig) -> Result<(), String> {
    if let Some(extensions) = &indexer.extensions {
        for (index, extension) in extensions.iter().enumerate() {
            if extension.is_empty() || extension.starts_with('.') {
                return Err(format!(
                    "Indexer.extensions[{index}] must be an extension without the dot, such as \"md\""
                ));
            }
        }
    }
    let mut names = HashSet::new();
    for (index, source) in indexer.sources.iter().enumerate() {
        if source.name.trim().is_empty() {
            return Err(format!("Indexer.sources[{index}].name must not be empty"));
        }
        if source.path.trim().is_empty() {
            return Err(format!("Indexer.sources[{index}].path must not be empty"));
        }
        if !names.insert(source.name.as_str()) {
            return Err(format!("duplicate source name {:?}", source.name));
        }
    }
    if let Some(catalog) = &report.catalog {
        if catalog.path.trim().is_empty() {
            return Err("Report.catalog.path must not be empty".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use conferencier::Error;
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn first_start_persists_defaults_and_reopens() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("indexer.toml");
        let (settings, created) = Settings::open(path.clone()).await.unwrap();
        assert!(created);

        let store = ConfigStore::from_file(&path).await.unwrap();
        let indexer = IndexerConfig::from_store(&store).unwrap();
        let indexer = indexer.read().await;
        assert!(indexer.recursive);
        assert!(!indexer.include_hidden);
        assert!(indexer.extensions.is_none());
        assert!(indexer.sources.is_empty());
        let report = ReportConfig::from_store(&store).unwrap();
        assert_eq!(report.read().await.summary.largest, 5);
        assert!(report.read().await.summary.by_extension);
        assert!(report.read().await.catalog.is_none());
        let serialized = store.save_str().unwrap();
        assert!(serialized.contains("sources = []"));
        assert!(serialized.contains("[Report.summary]"));
        assert!(!serialized.contains("extensions"));
        assert!(!serialized.contains("[Report.catalog]"));
        assert!(!serialized.contains("has_scanned"));
        assert_eq!(settings.effective_toml().await.unwrap(), serialized);

        let (_, created) = Settings::open(path).await.unwrap();
        assert!(!created);
    }

    #[tokio::test]
    async fn strict_keys_and_unowned_sections_are_reported() {
        let invalid = [
            ("[Indexer]\ninclude_hiden = true", "include_hiden"),
            ("[[Indexer.sources]]\nname = 'a'\npth = '.'", "pth"),
            ("[Report.catalog]\npath = 'x'\nprety = true", "prety"),
        ];
        for (source, expected_key) in invalid {
            let store = ConfigStore::from_str(source).unwrap();
            let error = match IndexerConfig::from_store(&store) {
                Err(error) => error,
                Ok(_) => match ReportConfig::from_store(&store) {
                    Err(error) => error,
                    Ok(_) => panic!("accepted invalid configuration: {source}"),
                },
            };
            match error {
                Error::UnknownKey { key, .. } => assert_eq!(key, expected_key),
                other => panic!("unexpected error: {other}"),
            }
        }

        let dir = tempdir().unwrap();
        let path = dir.path().join("indexer.toml");
        let source = "[Report]\ncolour = true\n[Reprot]\nlargest = 10\n";
        tokio::fs::write(&path, source).await.unwrap();
        let (settings, _) = Settings::open(path.clone()).await.unwrap();
        assert_eq!(settings.unexpected_sections(), vec!["Reprot"]);
        assert_eq!(settings.report.read().await.summary.largest, 5);
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), source);

        settings.save().await.unwrap();
        let saved = ConfigStore::from_file(path).await.unwrap();
        assert!(saved.get_section("Reprot").is_some());
        assert!(saved.get_value("Report", "colour").is_none());
    }

    #[tokio::test]
    async fn validation_rejects_invalid_values() {
        let invalid = [
            ("[Indexer]\nextensions = ['.md']", "extensions[0]"),
            ("[Indexer]\nextensions = ['']", "extensions[0]"),
            ("[[Indexer.sources]]\nname = ' '\npath = '.'", "name"),
            ("[[Indexer.sources]]\nname = 'a'\npath = ' '", "path"),
            (
                "[[Indexer.sources]]\nname = 'a'\npath = '.'\n\
                 [[Indexer.sources]]\nname = 'a'\npath = '.'",
                "duplicate source name",
            ),
            ("[Report.catalog]\npath = ' '", "Report.catalog.path"),
        ];
        for (source, expected) in invalid {
            let store = ConfigStore::from_str(source).unwrap();
            let indexer = IndexerConfig::from_store(&store).unwrap();
            let report = ReportConfig::from_store(&store).unwrap();
            let error = validate(&*indexer.read().await, &*report.read().await).unwrap_err();
            assert!(error.contains(expected), "{source}: {error}");
        }
    }

    #[tokio::test]
    async fn failed_save_restores_scopes_and_preserves_runtime_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("indexer.toml");
        let (mut settings, _) = Settings::open(path.clone()).await.unwrap();
        let before = tokio::fs::read_to_string(&path).await.unwrap();
        settings.indexer.write().await.has_scanned = true;
        settings.indexer.write().await.recursive = false;
        settings.report.write().await.summary.largest = 1;
        // A directory cannot be replaced by save_file; the original config
        // remains available for checking the failed write.
        settings.path = dir.path().to_path_buf();

        assert!(settings.save().await.is_err());
        let indexer = settings.indexer.read().await;
        assert!(indexer.recursive);
        assert!(indexer.has_scanned);
        assert_eq!(settings.report.read().await.summary.largest, 5);
        assert_eq!(settings.store.save_str().unwrap(), before);
        assert_eq!(tokio::fs::read_to_string(path).await.unwrap(), before);
    }
}
