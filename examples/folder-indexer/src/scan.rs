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
/// source succeeds. A failed scan leaves the previous runtime results intact.
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
                path: relative.to_string_lossy().replace('\\', "/"),
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
