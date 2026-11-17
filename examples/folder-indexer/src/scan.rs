//! The directory scanner. It reads configuration from its scope, then releases
//! the lock before walking the filesystem.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tokio::task::JoinSet;

use crate::config::{BoxError, Settings, Source};

/// One discovered file. These values are runtime state, not configuration.
#[derive(Clone)]
pub struct IndexedFile {
    pub source: String,
    /// Path relative to the configured source directory, with `/` separators.
    pub path: String,
    pub extension: Option<String>,
    pub size_bytes: u64,
    /// Seconds since the Unix epoch; unavailable if the filesystem has no
    /// modification time or the clock value predates the epoch.
    pub modified_unix_secs: Option<u64>,
}

/// Counts collected for one configured source.
#[derive(Clone)]
pub struct SourceStats {
    pub name: String,
    pub files: usize,
    pub bytes: u64,
}

struct SourceScan {
    files: Vec<IndexedFile>,
    stats: SourceStats,
}

/// Scans each source concurrently and publishes results only after every
/// source succeeds. A failed source leaves the previous results intact.
/// A later catalog-write error does not undo a successful scan.
pub async fn run(settings: &Settings) -> Result<(), BoxError> {
    let (recursive, include_hidden, extensions, sources) = {
        let config = settings.indexer.read().await;
        (
            config.recursive,
            config.include_hidden,
            config.extensions.clone(),
            config.sources.clone(),
        )
    };
    let config_dir = settings
        .path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let allowed_extensions: Option<HashSet<String>> = extensions.map(|items| {
        items
            .into_iter()
            .map(|extension| extension.to_ascii_lowercase())
            .collect()
    });

    let mut tasks = JoinSet::new();
    for source in sources {
        let root = resolve_path(&config_dir, &source.path);
        let allowed_extensions = allowed_extensions.clone();
        tasks.spawn(async move {
            scan_source(source, root, recursive, include_hidden, allowed_extensions).await
        });
    }

    let mut files = Vec::new();
    let mut source_stats = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let result = match joined {
            Ok(result) => result?,
            Err(error) => return Err(format!("scanner task failed: {error}").into()),
        };
        files.extend(result.files);
        source_stats.push(result.stats);
    }
    files.sort_by(|left, right| (&left.source, &left.path).cmp(&(&right.source, &right.path)));
    source_stats.sort_by(|left, right| left.name.cmp(&right.name));

    {
        let mut config = settings.indexer.write().await;
        config.files = files;
        config.source_stats = source_stats;
        config.has_scanned = true;
    }
    crate::report::print(settings).await;
    crate::report::write_catalog(settings).await?;
    Ok(())
}

fn resolve_path(config_dir: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        config_dir.join(path)
    }
}

async fn scan_source(
    source: Source,
    root: PathBuf,
    recursive: bool,
    include_hidden: bool,
    allowed_extensions: Option<HashSet<String>>,
) -> Result<SourceScan, BoxError> {
    let mut directories = vec![root.clone()];
    let mut files = Vec::new();
    let excluded: HashSet<&str> = source
        .exclude
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect();

    while let Some(directory) = directories.pop() {
        let mut entries = tokio::fs::read_dir(&directory)
            .await
            .map_err(|error| format!("cannot read {}: {error}", directory.display()))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
        {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if excluded.contains(name.as_ref()) || (!include_hidden && name.starts_with('.')) {
                continue;
            }

            let file_type = entry
                .file_type()
                .await
                .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
            // Do not follow symlinks: they can escape the source or create
            // cycles, and indexing the same file through aliases is confusing.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if recursive {
                    directories.push(entry.path());
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let extension = entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_owned);
            if let Some(allowed) = &allowed_extensions {
                let matches = extension
                    .as_ref()
                    .is_some_and(|extension| allowed.contains(&extension.to_ascii_lowercase()));
                if !matches {
                    continue;
                }
            }

            let metadata = entry
                .metadata()
                .await
                .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
            let entry_path = entry.path();
            let relative = entry_path
                .strip_prefix(&root)
                .map_err(|error| format!("cannot make file path relative: {error}"))?;
            let modified_unix_secs = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs());
            files.push(IndexedFile {
                source: source.name.clone(),
                path: relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
                extension,
                size_bytes: metadata.len(),
                modified_unix_secs,
            });
        }
    }

    let stats = SourceStats {
        name: source.name,
        files: files.len(),
        bytes: files.iter().map(|file| file.size_bytes).sum(),
    };
    Ok(SourceScan { files, stats })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::config::CatalogFile;

    fn sample_source(name: &str) -> Source {
        Source {
            name: name.into(),
            path: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("sample")
                .join(name)
                .to_str()
                .unwrap()
                .to_owned(),
            exclude: None,
        }
    }

    async fn paths(settings: &Settings) -> Vec<String> {
        settings
            .indexer
            .read()
            .await
            .files
            .iter()
            .map(|file| format!("{}:{}", file.source, file.path))
            .collect()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn backslashes_in_filenames_do_not_collide_with_directory_separators() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("notes");
        tokio::fs::create_dir_all(root.join("a")).await.unwrap();
        tokio::fs::write(root.join("a").join("b.txt"), "nested")
            .await
            .unwrap();
        tokio::fs::write(root.join(r"a\b.txt"), "backslash")
            .await
            .unwrap();
        let (settings, _) = Settings::open(dir.path().join("indexer.toml"))
            .await
            .unwrap();
        settings.indexer.write().await.sources.push(Source {
            name: "notes".into(),
            path: "notes".into(),
            exclude: None,
        });
        settings.report.write().await.catalog = Some(CatalogFile {
            path: "catalog.json".into(),
            pretty: false,
        });

        run(&settings).await.unwrap();

        assert_eq!(paths(&settings).await, ["notes:a/b.txt", r"notes:a\b.txt"]);
        let catalog: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(dir.path().join("catalog.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(catalog["total_files"], 2);
        assert_eq!(catalog["files"][0]["path"], "a/b.txt");
        assert_eq!(catalog["files"][1]["path"], r"a\b.txt");
    }

    #[tokio::test]
    async fn scan_filters_sources_and_writes_catalog() {
        let dir = tempdir().unwrap();
        let (settings, _) = Settings::open(dir.path().join("indexer.toml"))
            .await
            .unwrap();
        {
            let mut indexer = settings.indexer.write().await;
            indexer.sources = vec![sample_source("manuals"), sample_source("notes")];
            indexer.extensions = Some(vec!["MD".into()]);
        }
        settings.report.write().await.catalog = Some(CatalogFile {
            path: "catalog.json".into(),
            pretty: true,
        });
        run(&settings).await.unwrap();
        assert_eq!(
            paths(&settings).await,
            [
                "manuals:setup.md",
                "notes:drafts/idea.md",
                "notes:meeting.md",
            ]
        );
        {
            let indexer = settings.indexer.read().await;
            assert!(indexer.has_scanned);
            assert_eq!(indexer.source_stats[0].name, "manuals");
            assert_eq!(indexer.source_stats[0].files, 1);
            assert_eq!(indexer.source_stats[1].name, "notes");
            assert_eq!(indexer.source_stats[1].files, 2);
            let catalog: serde_json::Value = serde_json::from_slice(
                &tokio::fs::read(dir.path().join("catalog.json"))
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(catalog["total_files"], 3);
            assert_eq!(catalog["files"][0]["source"], "manuals");
            assert_eq!(catalog["files"][0]["path"], "setup.md");
            assert_eq!(
                catalog["total_bytes"].as_u64().unwrap(),
                indexer
                    .files
                    .iter()
                    .map(|file| file.size_bytes)
                    .sum::<u64>()
            );
        }

        settings.report.write().await.catalog = None;
        {
            let mut indexer = settings.indexer.write().await;
            indexer.sources = vec![sample_source("notes")];
            indexer.include_hidden = true;
            indexer.recursive = false;
            indexer.extensions = None;
        }
        run(&settings).await.unwrap();
        assert_eq!(
            paths(&settings).await,
            ["notes:.private.md", "notes:meeting.md", "notes:todo.txt"]
        );

        {
            let mut indexer = settings.indexer.write().await;
            indexer.recursive = true;
            indexer.sources[0].exclude = Some(vec!["drafts".into(), "todo.txt".into()]);
        }
        run(&settings).await.unwrap();
        assert_eq!(
            paths(&settings).await,
            ["notes:.private.md", "notes:meeting.md"]
        );
    }

    #[tokio::test]
    async fn failed_source_keeps_previous_results_and_catalog() {
        let dir = tempdir().unwrap();
        let (settings, _) = Settings::open(dir.path().join("indexer.toml"))
            .await
            .unwrap();
        settings
            .indexer
            .write()
            .await
            .sources
            .push(sample_source("notes"));
        settings.report.write().await.catalog = Some(CatalogFile {
            path: "catalog.json".into(),
            pretty: true,
        });
        run(&settings).await.unwrap();
        let before = paths(&settings).await;
        let catalog = tokio::fs::read(dir.path().join("catalog.json"))
            .await
            .unwrap();
        settings.indexer.write().await.sources.push(Source {
            name: "missing".into(),
            path: "missing".into(),
            exclude: None,
        });

        let error = run(&settings).await.unwrap_err();
        assert!(error.to_string().contains("missing"));
        assert_eq!(paths(&settings).await, before);
        assert_eq!(
            tokio::fs::read(dir.path().join("catalog.json"))
                .await
                .unwrap(),
            catalog
        );
    }

    #[tokio::test]
    async fn catalog_never_overwrites_existing_files() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("indexer.toml");
        let source_path = dir.path().join("document.md");
        tokio::fs::write(&source_path, "original document")
            .await
            .unwrap();
        let (settings, _) = Settings::open(config_path.clone()).await.unwrap();
        let config_before = tokio::fs::read(&config_path).await.unwrap();
        settings.indexer.write().await.sources.push(Source {
            name: "docs".into(),
            path: ".".into(),
            exclude: None,
        });

        for path in ["document.md", "indexer.toml"] {
            settings.report.write().await.catalog = Some(CatalogFile {
                path: path.into(),
                pretty: true,
            });
            let error = run(&settings).await.unwrap_err();
            assert!(error.to_string().contains("cannot create new catalog"));
            assert_eq!(
                tokio::fs::read(&source_path).await.unwrap(),
                b"original document"
            );
            assert_eq!(tokio::fs::read(&config_path).await.unwrap(), config_before);
        }

        settings.report.write().await.catalog = Some(CatalogFile {
            path: "catalog.json".into(),
            pretty: true,
        });
        run(&settings).await.unwrap();
        let catalog_path = dir.path().join("catalog.json");
        let catalog_before = tokio::fs::read(&catalog_path).await.unwrap();
        tokio::fs::write(dir.path().join("new.md"), "new document")
            .await
            .unwrap();
        let error = run(&settings).await.unwrap_err();
        assert!(error.to_string().contains("cannot create new catalog"));
        assert_eq!(tokio::fs::read(catalog_path).await.unwrap(), catalog_before);
        assert!(paths(&settings).await.contains(&"docs:new.md".to_string()));
    }
}
