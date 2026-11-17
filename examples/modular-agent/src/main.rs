//! Modular agent: an example application configured with conferencier.

mod component;
mod events;
mod persist;

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;

use component::{Component, RunningComponent};
use conferencier::ConfigStore;
use persist::PersistenceHandle;
use tokio::sync::{broadcast, mpsc, watch};

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect(), components()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn components() -> Vec<Box<dyn Component>> {
    Vec::new()
}

async fn run(args: Vec<String>, components: Vec<Box<dyn Component>>) -> AppResult<()> {
    let options = Options::parse(args)?;
    validate_registry(&components)?;
    for name in &options.without {
        if !components.iter().any(|component| component.name() == name) {
            return Err(format!("unknown component {name:?}").into());
        }
    }

    let (store, created) = open_store(&options.config).await?;
    let (persist_tx, persist_rx) = mpsc::unbounded_channel();
    let persistence = PersistenceHandle::new(persist_tx);
    let persister = tokio::spawn(persist::run(
        store.clone(),
        options.config.clone(),
        persist_rx,
    ));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (events_tx, _) = broadcast::channel(32);

    if created {
        println!(
            "Starting with an empty configuration at {}",
            options.config.display()
        );
    } else {
        println!("Loaded configuration from {}", options.config.display());
    }

    let context = component::Context {
        store: store.clone(),
        persistence: persistence.clone(),
        events: events_tx,
        shutdown: shutdown_rx,
    };
    let mut running = Vec::<Box<dyn RunningComponent>>::new();
    for component in &components {
        if options.without.iter().any(|name| name == component.name()) {
            println!(
                "Component {:?} disabled; its configuration is left untouched",
                component.name()
            );
            continue;
        }
        match component.start(context.clone()).await {
            Ok(instance) => running.push(instance),
            Err(error) => eprintln!("component {}: {error}", component.name()),
        }
    }

    let owned_sections = components
        .iter()
        .map(|component| component.section())
        .collect::<Vec<_>>();
    for section in store.list_sections() {
        if !owned_sections.contains(&section.as_str()) {
            println!("warning: unowned section [{section}] is preserved");
        }
    }

    println!("Type `help` for commands.");
    let mut console = spawn_console();
    let outcome = loop {
        tokio::select! {
            line = console.recv() => {
                let Some(line) = line else { break Ok(()) };
                match handle(&store, &mut running, line.trim()).await {
                    Ok(Continue::Yes) => {}
                    Ok(Continue::No) => break Ok(()),
                    Err(error) => eprintln!("error: {error}"),
                }
            }
            result = tokio::signal::ctrl_c() => {
                result?;
                break Ok(());
            }
        }
    };

    let _ = shutdown_tx.send(true);
    for component in &mut running {
        if let Err(error) = component.shutdown().await {
            eprintln!("component {} shutdown: {error}", component.name());
        }
    }
    let flush_result = persistence.flush().await;
    drop(persistence);
    let _ = persister.await;
    flush_result?;
    outcome
}

fn validate_registry(components: &[Box<dyn Component>]) -> AppResult<()> {
    for (index, component) in components.iter().enumerate() {
        if components[..index]
            .iter()
            .any(|prior| prior.name() == component.name())
        {
            return Err(format!("duplicate component name {:?}", component.name()).into());
        }
        if components[..index]
            .iter()
            .any(|prior| prior.section() == component.section())
        {
            return Err(
                format!("duplicate configuration section {:?}", component.section()).into(),
            );
        }
    }
    Ok(())
}

async fn open_store(path: &std::path::Path) -> AppResult<(ConfigStore, bool)> {
    let exists = tokio::fs::try_exists(path)
        .await
        .map_err(|error| format!("cannot access {}: {error}", path.display()))?;
    if exists {
        Ok((ConfigStore::from_file(path).await?, false))
    } else {
        Ok((ConfigStore::new(), true))
    }
}

enum Continue {
    Yes,
    No,
}

async fn handle(
    store: &ConfigStore,
    components: &mut [Box<dyn RunningComponent>],
    line: &str,
) -> AppResult<Continue> {
    let mut words = line.split_whitespace();
    let Some(command) = words.next() else {
        return Ok(Continue::Yes);
    };
    match command {
        "help" => print_help(components),
        "components" => {
            for component in components.iter() {
                println!("{}", component.name());
            }
        }
        "config" => print!("{}", store.save_str()?),
        "quit" => return Ok(Continue::No),
        name => {
            let component = components
                .iter_mut()
                .find(|component| component.name() == name)
                .ok_or_else(|| format!("unknown command or component {name:?}"))?;
            let args = words.collect::<Vec<_>>().join(" ");
            component.handle_command(&args).await?;
        }
    }
    Ok(Continue::Yes)
}

fn print_help(components: &[Box<dyn RunningComponent>]) {
    println!("Commands:");
    println!("  components                     list running components");
    println!("  config                         show the complete TOML configuration");
    println!("  <component> <command>          run a component command");
    println!("  help                           show this list");
    println!("  quit                           exit (or press Ctrl+C)");
    if !components.is_empty() {
        println!("Components:");
        for component in components {
            println!("  {}", component.name());
        }
    }
}

/// Reads console commands on a dedicated thread, which Tokio does not wait on at shutdown.
fn spawn_console() -> mpsc::UnboundedReceiver<String> {
    let (sender, receiver) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

struct Options {
    config: PathBuf,
    without: Vec<String>,
}

impl Options {
    fn parse(args: Vec<String>) -> AppResult<Self> {
        let mut args = args.into_iter();
        let mut config = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agent.toml");
        let mut without = Vec::new();
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--config" => {
                    let path = args
                        .next()
                        .ok_or("usage: modular-agent [--config <path>] [--without <name>]...")?;
                    config = PathBuf::from(path);
                }
                "--without" => {
                    let name = args
                        .next()
                        .ok_or("usage: modular-agent [--config <path>] [--without <name>]...")?;
                    if without.contains(&name) {
                        return Err(format!("component {name:?} is disabled more than once").into());
                    }
                    without.push(name);
                }
                _ => return Err(format!("unknown argument {argument:?}").into()),
            }
        }
        Ok(Self { config, without })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_and_repeated_disabled_components() {
        let options = Options::parse(vec![
            "--config".into(),
            "agent.toml".into(),
            "--without".into(),
            "heartbeat".into(),
            "--without".into(),
            "journal".into(),
        ])
        .unwrap();
        assert_eq!(options.config, PathBuf::from("agent.toml"));
        assert_eq!(options.without, ["heartbeat", "journal"]);
    }

    #[test]
    fn rejects_missing_or_duplicate_arguments() {
        assert!(Options::parse(vec!["--config".into()]).is_err());
        assert!(
            Options::parse(vec![
                "--without".into(),
                "journal".into(),
                "--without".into(),
                "journal".into()
            ])
            .is_err()
        );
        assert!(Options::parse(vec!["--other".into()]).is_err());
    }
}
