//! Folder indexer: an example application configured with conferencier.
//!
//! The application creates and changes its own configuration file. See
//! `README.md` for usage and architecture.

mod config;
mod report;
mod scan;

use std::future::Future;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;

use tokio::sync::mpsc;

use crate::config::{BoxError, Settings, Source};

type AppResult<T> = Result<T, BoxError>;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> AppResult<()> {
    let path = config_path(std::env::args().skip(1))?;
    let (settings, created) = Settings::open(path).await?;
    if created {
        println!(
            "Created {} with the default configuration",
            settings.path.display()
        );
    } else {
        println!("Loaded configuration from {}", settings.path.display());
    }
    for section in settings.unexpected_sections() {
        println!("warning: unexpected section [{section}] is ignored; is its name misspelled?");
    }
    println!("Type `help` for commands.");
    report::print(&settings).await;

    command_loop(&settings, spawn_console(), tokio::signal::ctrl_c()).await?;
    println!("Goodbye");
    Ok(())
}

async fn command_loop(
    settings: &Settings,
    mut console: mpsc::Receiver<String>,
    interrupt: impl Future<Output = io::Result<()>>,
) -> AppResult<()> {
    // Keep the interrupt listener alive while a command runs.
    tokio::pin!(interrupt);
    loop {
        tokio::select! {
            // Handle an interrupt before any queued command.
            biased;
            result = &mut interrupt => {
                result?;
                break;
            }
            line = console.recv() => {
                let Some(line) = line else { break };
                match handle(settings, line.trim()).await {
                    Ok(Continue::Yes) => {}
                    Ok(Continue::No) => break,
                    Err(error) => eprintln!("error: {error}"),
                }
            }
        }
    }
    Ok(())
}

enum Continue {
    Yes,
    No,
}

async fn handle(settings: &Settings, line: &str) -> AppResult<Continue> {
    let mut words = line.split_whitespace();
    let Some(command) = words.next() else {
        return Ok(Continue::Yes);
    };
    match command {
        "help" => print_help(),
        "quit" => return Ok(Continue::No),
        "config" => print!("{}", settings.effective_toml().await?),
        "report" => report::print(settings).await,
        "scan" => scan::run(settings).await?,
        "add-source" => {
            let name = required_arg(&mut words, "usage: add-source <name> <path>")?;
            let path = required_arg(&mut words, "usage: add-source <name> <path>")?;
            no_more_args(words, "usage: add-source <name> <path>")?;

            let mut indexer = settings.indexer.write().await;
            indexer.sources.push(Source {
                name: name.to_owned(),
                path: path.to_owned(),
                exclude: None,
            });
            // Validation and saving need to acquire the scope locks themselves.
            drop(indexer);
            if let Err(error) = validate_settings(settings).await {
                settings.indexer.write().await.sources.pop();
                return Err(error.into());
            }
            settings.save().await?;
            println!("Added source {name:?}");
        }
        "remove-source" => {
            let name = required_arg(&mut words, "usage: remove-source <name>")?;
            no_more_args(words, "usage: remove-source <name>")?;
            let mut indexer = settings.indexer.write().await;
            let Some(position) = indexer
                .sources
                .iter()
                .position(|source| source.name == name)
            else {
                return Err(format!("no source named {name:?}").into());
            };
            let removed = indexer.sources.remove(position);
            drop(indexer);
            if let Err(error) = validate_settings(settings).await {
                settings
                    .indexer
                    .write()
                    .await
                    .sources
                    .insert(position, removed);
                return Err(error.into());
            }
            settings.save().await?;
            println!("Removed source {name:?}");
        }
        "extensions" => {
            let value = required_arg(&mut words, "usage: extensions <all|md,txt>")?;
            no_more_args(words, "usage: extensions <all|md,txt>")?;
            let extensions: Option<Vec<String>> = if value == "all" {
                None
            } else {
                Some(value.split(',').map(str::to_owned).collect())
            };
            let mut indexer = settings.indexer.write().await;
            let previous = std::mem::replace(&mut indexer.extensions, extensions);
            drop(indexer);
            if let Err(error) = validate_settings(settings).await {
                settings.indexer.write().await.extensions = previous;
                return Err(error.into());
            }
            settings.save().await?;
            println!("Updated the extension filter");
        }
        "largest" => {
            let value = required_arg(&mut words, "usage: largest <count>")?;
            no_more_args(words, "usage: largest <count>")?;
            let count = value
                .parse::<u32>()
                .map_err(|error| format!("invalid largest-file count {value:?}: {error}"))?;
            let previous = {
                let mut report = settings.report.write().await;
                std::mem::replace(&mut report.summary.largest, count)
            };
            if let Err(error) = validate_settings(settings).await {
                settings.report.write().await.summary.largest = previous;
                return Err(error.into());
            }
            settings.save().await?;
            println!("The report will list up to {count} largest files");
        }
        "catalog" => {
            let value = required_arg(&mut words, "usage: catalog <off|path>")?;
            no_more_args(words, "usage: catalog <off|path>")?;
            let mut report = settings.report.write().await;
            let replacement = if value == "off" {
                None
            } else {
                // Keep a `pretty` setting edited in the file.
                let pretty = report.catalog.as_ref().is_none_or(|catalog| catalog.pretty);
                Some(crate::config::CatalogFile {
                    path: value.to_owned(),
                    pretty,
                })
            };
            let previous = std::mem::replace(&mut report.catalog, replacement);
            drop(report);
            if let Err(error) = validate_settings(settings).await {
                settings.report.write().await.catalog = previous;
                return Err(error.into());
            }
            settings.save().await?;
            println!("Updated the catalog setting");
        }
        other => println!("Unknown command {other:?}; type `help` for the list."),
    }
    Ok(Continue::Yes)
}

async fn validate_settings(settings: &Settings) -> Result<(), String> {
    let indexer = settings.indexer.read().await;
    let report = settings.report.read().await;
    crate::config::validate(&indexer, &report)
}

fn required_arg<'a>(words: &mut impl Iterator<Item = &'a str>, usage: &str) -> AppResult<&'a str> {
    words.next().ok_or_else(|| usage.into())
}

fn no_more_args<'a>(mut words: impl Iterator<Item = &'a str>, usage: &str) -> AppResult<()> {
    if words.next().is_some() {
        Err(usage.into())
    } else {
        Ok(())
    }
}

fn print_help() {
    println!("Commands:");
    println!("  config                          show the effective configuration");
    println!("  add-source <name> <path>        add a folder (relative to the config file)");
    println!("  remove-source <name>            remove a folder");
    println!("  extensions <all|md,txt>         select extensions; `all` removes the key");
    println!("  largest <count>                 set the number of large files in the report");
    println!("  catalog <off|path>              write to a new JSON file, or disable it");
    println!("  scan                            scan all configured folders");
    println!("  report                          print the last scan report again");
    println!("  help                            show this list");
    println!("  quit                            exit (or press Ctrl+C)");
}

/// Reads stdin on a dedicated thread: unlike Tokio's blocking stdin worker,
/// it does not hold up runtime shutdown.
fn spawn_console() -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel(8);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if sender.blocking_send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

/// The configuration file: `--config <path>`, or `indexer.toml` in the package
/// folder, independent of the working directory.
fn config_path(mut args: impl Iterator<Item = String>) -> AppResult<PathBuf> {
    match (args.next().as_deref(), args.next(), args.next()) {
        (None, _, _) => Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("indexer.toml")),
        (Some("--config"), Some(path), None) => Ok(PathBuf::from(path)),
        _ => Err("usage: folder-indexer [--config <path>]".into()),
    }
}

#[cfg(test)]
mod tests {
    use conferencier::{ConfigScope, ConfigStore};
    use tempfile::tempdir;

    use super::*;
    use crate::config::{CatalogFile, IndexerConfig, ReportConfig};

    #[tokio::test]
    async fn interrupt_during_scan_finishes_scan_and_skips_queued_commands() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("notes");
        tokio::fs::create_dir(&root).await.unwrap();
        tokio::fs::write(root.join("note.txt"), "a note")
            .await
            .unwrap();
        let (settings, _) = Settings::open(dir.path().join("indexer.toml"))
            .await
            .unwrap();
        settings.report.write().await.catalog = Some(CatalogFile {
            path: "catalog.json".into(),
            pretty: true,
        });
        let mut indexer = settings.indexer.write().await;
        indexer.sources.push(Source {
            name: "notes".into(),
            path: "notes".into(),
            exclude: None,
        });
        let (sender, console) = mpsc::channel(8);
        sender.send("scan".into()).await.unwrap();
        let (interrupt_tx, interrupt_rx) = tokio::sync::oneshot::channel();
        let running = command_loop(&settings, console, async {
            interrupt_rx.await.unwrap();
            Ok(())
        });
        tokio::pin!(running);
        // Poll until the scan waits for the held indexer lock, then send the interrupt.
        std::future::poll_fn(|cx| {
            assert!(running.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;

        interrupt_tx.send(()).unwrap();
        sender.send("largest 99".into()).await.unwrap();
        assert!(!indexer.has_scanned);
        drop(indexer);
        running.await.unwrap();

        let indexer = settings.indexer.read().await;
        assert!(indexer.has_scanned);
        assert_eq!(indexer.files.len(), 1);
        assert_eq!(indexer.files[0].path, "note.txt");
        assert_eq!(settings.report.read().await.summary.largest, 5);
        let catalog: serde_json::Value = serde_json::from_slice(
            &tokio::fs::read(dir.path().join("catalog.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(catalog["total_files"], 1);
        assert_eq!(catalog["files"][0]["path"], "note.txt");
    }

    #[tokio::test]
    async fn interrupt_during_configuration_edit_does_not_cancel_save() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("indexer.toml");
        let (settings, _) = Settings::open(path.clone()).await.unwrap();
        let indexer = settings.indexer.write().await;
        let (sender, console) = mpsc::channel(8);
        sender.send("largest 2".into()).await.unwrap();
        let (interrupt_tx, interrupt_rx) = tokio::sync::oneshot::channel();
        let running = command_loop(&settings, console, async {
            interrupt_rx.await.unwrap();
            Ok(())
        });
        tokio::pin!(running);
        // Pause the command at validation on the held indexer lock.
        std::future::poll_fn(|cx| {
            assert!(running.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;

        assert_eq!(settings.report.read().await.summary.largest, 2);
        interrupt_tx.send(()).unwrap();
        drop(indexer);
        running.await.unwrap();

        let store = ConfigStore::from_file(&path).await.unwrap();
        let report = ReportConfig::from_store(&store).unwrap();
        assert_eq!(report.read().await.summary.largest, 2);
    }

    #[tokio::test]
    async fn catalog_command_keeps_the_pretty_setting() {
        let dir = tempdir().unwrap();
        let (settings, _) = Settings::open(dir.path().join("indexer.toml"))
            .await
            .unwrap();
        settings.report.write().await.catalog = Some(CatalogFile {
            path: "old.json".into(),
            pretty: false,
        });

        assert!(handle(&settings, "catalog new.json").await.is_ok());
        let report = settings.report.read().await;
        let catalog = report.catalog.as_ref().unwrap();
        assert_eq!((catalog.path.as_str(), catalog.pretty), ("new.json", false));
    }

    #[tokio::test]
    async fn console_edits_persist_and_remove_optional_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("indexer.toml");
        let (settings, _) = Settings::open(path.clone()).await.unwrap();

        for command in [
            "add-source notes sample/notes",
            "extensions md,txt",
            "catalog catalog.json",
            "largest 2",
        ] {
            assert!(handle(&settings, command).await.is_ok(), "{command}");
        }
        let store = ConfigStore::from_file(&path).await.unwrap();
        let indexer = IndexerConfig::from_store(&store).unwrap();
        let indexer = indexer.read().await;
        assert_eq!(indexer.sources[0].name, "notes");
        assert_eq!(indexer.sources[0].path, "sample/notes");
        assert_eq!(indexer.extensions.as_deref().unwrap(), ["md", "txt"]);
        let report = ReportConfig::from_store(&store).unwrap();
        let report = report.read().await;
        assert_eq!(report.summary.largest, 2);
        assert_eq!(report.catalog.as_ref().unwrap().path, "catalog.json");
        drop(report);
        drop(indexer);

        let before = tokio::fs::read_to_string(&path).await.unwrap();
        for command in [
            "add-source notes another",
            "extensions .md",
            "extensions md,,txt",
            "remove-source missing",
            "largest nope",
        ] {
            assert!(handle(&settings, command).await.is_err(), "{command}");
            assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), before);
            assert_eq!(settings.indexer.read().await.sources.len(), 1);
            assert_eq!(
                settings.indexer.read().await.extensions.as_deref().unwrap(),
                ["md", "txt"]
            );
        }

        for command in ["extensions all", "catalog off", "remove-source notes"] {
            assert!(handle(&settings, command).await.is_ok(), "{command}");
        }
        let store = ConfigStore::from_file(&path).await.unwrap();
        assert!(store.get_value("Indexer", "extensions").is_none());
        assert!(store.get_value("Report", "catalog").is_none());
        assert!(
            IndexerConfig::from_store(&store)
                .unwrap()
                .read()
                .await
                .sources
                .is_empty()
        );
    }
}
