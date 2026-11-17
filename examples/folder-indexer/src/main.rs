//! Folder indexer: an example application configured with conferencier.
//!
//! The application creates and changes its own configuration file. See
//! `README.md` for usage and architecture.

mod config;
mod report;
mod scan;

use std::io::BufRead;
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

    let mut console = spawn_console();
    loop {
        tokio::select! {
            line = console.recv() => {
                let Some(line) = line else { break };
                match handle(&settings, line.trim()).await {
                    Ok(Continue::Yes) => {}
                    Ok(Continue::No) => break,
                    Err(error) => eprintln!("error: {error}"),
                }
            }
            result = tokio::signal::ctrl_c() => {
                result?;
                break;
            }
        }
    }
    println!("Goodbye");
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
            if name.trim().is_empty() {
                return Err("source name must not be empty".into());
            }
            if path.trim().is_empty() {
                return Err("source path must not be empty".into());
            }
            if indexer.sources.iter().any(|source| source.name == name) {
                return Err(format!("duplicate source name {name:?}").into());
            }
            indexer.sources.push(Source {
                name: name.to_owned(),
                path: path.to_owned(),
                exclude: None,
            });
            drop(indexer);
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
            indexer.sources.remove(position);
            drop(indexer);
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
            if let Some(extensions) = &extensions {
                if let Some((index, _)) = extensions
                    .iter()
                    .enumerate()
                    .find(|(_, extension)| extension.is_empty() || extension.starts_with('.'))
                {
                    return Err(format!(
                        "Indexer.extensions[{index}] must be an extension without the dot, such as \"md\""
                    )
                    .into());
                }
            }
            indexer.extensions = extensions;
            drop(indexer);
            settings.save().await?;
            println!("Updated the extension filter");
        }
        "largest" => {
            let value = required_arg(&mut words, "usage: largest <count>")?;
            no_more_args(words, "usage: largest <count>")?;
            let count = value
                .parse::<u32>()
                .map_err(|error| format!("invalid largest-file count {value:?}: {error}"))?;
            settings.report.write().await.summary.largest = count;
            settings.save().await?;
            println!("The report will list up to {count} largest files");
        }
        "catalog" => {
            let value = required_arg(&mut words, "usage: catalog <off|path>")?;
            no_more_args(words, "usage: catalog <off|path>")?;
            let catalog = if value == "off" {
                None
            } else if value.trim().is_empty() {
                return Err("catalog path must not be empty".into());
            } else {
                Some(crate::config::CatalogFile {
                    path: value.to_owned(),
                    pretty: true,
                })
            };
            settings.report.write().await.catalog = catalog;
            settings.save().await?;
            println!("Updated the catalog setting");
        }
        other => println!("Unknown command {other:?}; type `help` for the list."),
    }
    Ok(Continue::Yes)
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
    println!("  config                         show the effective configuration");
    println!("  add-source <name> <path>       add a folder (relative to the config file)");
    println!("  remove-source <name>            remove a folder");
    println!("  extensions <all|md,txt>         select extensions; `all` removes the key");
    println!("  largest <count>                 set the number of large files in the report");
    println!("  catalog <off|path>              write or disable the JSON catalog");
    println!("  scan                            scan all configured folders");
    println!("  report                          print the last scan report again");
    println!("  help                            show this list");
    println!("  quit                            exit (or press Ctrl+C)");
}

/// Reads console commands on a dedicated thread. A plain thread is not waited
/// for by Tokio at shutdown, unlike Tokio's blocking stdin worker.
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
