//! Console summary and the optional JSON catalog.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::config::{BoxError, Settings};
use crate::scan::IndexedFile;

/// Prints the report for the latest scan. The settings and runtime results are
/// copied under short read locks; formatting does not hold a scope lock.
pub async fn print(settings: &Settings) {
    let (files, source_stats, has_scanned) = {
        let config = settings.indexer.read().await;
        (
            config.files.clone(),
            config.source_stats.clone(),
            config.has_scanned,
        )
    };
    let (largest, by_extension) = {
        let config = settings.report.read().await;
        (config.summary.largest as usize, config.summary.by_extension)
    };

    if !has_scanned {
        println!("No scan results yet. Type `scan` to index the configured folders.");
        return;
    }

    println!("Indexed {} files:", files.len());
    if source_stats.is_empty() {
        println!("  No folders are configured. Use `add-source` to add one.");
    }
    for stats in source_stats {
        println!(
            "  {:<16} {:>5} files, {}",
            stats.name,
            stats.files,
            format_bytes(stats.bytes)
        );
    }

    if by_extension {
        let mut counts = BTreeMap::<String, usize>::new();
        for file in &files {
            *counts
                .entry(
                    file.extension
                        .as_deref()
                        .map(str::to_ascii_lowercase)
                        .unwrap_or_else(|| "(none)".into()),
                )
                .or_default() += 1;
        }
        if !counts.is_empty() {
            println!("By extension:");
            for (extension, count) in counts {
                println!("  {extension:<12} {count}");
            }
        }
    }

    if largest > 0 && !files.is_empty() {
        let mut largest_files = files;
        largest_files.sort_by(|left, right| {
            right
                .size_bytes
                .cmp(&left.size_bytes)
                .then_with(|| (&left.source, &left.path).cmp(&(&right.source, &right.path)))
        });
        println!("Largest files:");
        for file in largest_files.iter().take(largest) {
            println!(
                "  {}:{} ({})",
                file.source,
                file.path,
                format_bytes(file.size_bytes)
            );
        }
    }
}

/// Writes the catalog when `[Report.catalog]` is configured.
///
/// The catalog is generated from runtime results; it is not a conferencier
/// scope, so `serde_json::json!` is used only for this output format.
/// A catalog never overwrites an existing file, including a previous catalog.
pub async fn write_catalog(settings: &Settings) -> Result<(), BoxError> {
    let (catalog, files) = {
        let config = settings.report.read().await;
        let Some(catalog) = &config.catalog else {
            return Ok(());
        };
        let indexer = settings.indexer.read().await;
        (catalog.clone(), indexer.files.clone())
    };

    let value = json!({
        "files": files.iter().map(catalog_entry).collect::<Vec<_>>(),
        "total_files": files.len(),
        "total_bytes": files.iter().map(|file| file.size_bytes).sum::<u64>(),
    });
    let contents = if catalog.pretty {
        serde_json::to_vec_pretty(&value)?
    } else {
        serde_json::to_vec(&value)?
    };
    let path = resolve_path(settings, &catalog.path);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_err(|error| format!("cannot create new catalog {}: {error}", path.display()))?;
    file.write_all(&contents).await.map_err(|error| {
        format!(
            "cannot write catalog {}; it may be incomplete: {error}",
            path.display()
        )
    })?;
    file.flush().await.map_err(|error| {
        format!(
            "cannot finish catalog {}; it may be incomplete: {error}",
            path.display()
        )
    })?;
    println!("Wrote {} files to {}", files.len(), path.display());
    Ok(())
}

fn catalog_entry(file: &IndexedFile) -> serde_json::Value {
    json!({
        "source": file.source,
        "path": file.path,
        "extension": file.extension,
        "size_bytes": file.size_bytes,
        "modified_unix_secs": file.modified_unix_secs,
    })
}

fn resolve_path(settings: &Settings, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        settings
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}
