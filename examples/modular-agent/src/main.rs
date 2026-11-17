//! Modular agent: an example application configured with conferencier.

mod component;
mod events;
mod persist;
mod components {
    pub mod heartbeat;
    pub mod journal;
    pub mod reminders;
}

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
    vec![
        Box::new(components::heartbeat::Heartbeat),
        Box::new(components::reminders::Reminders),
        Box::new(components::journal::Journal),
    ]
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
        config_path: options.config.clone(),
        persistence: persistence.clone(),
        events: events_tx,
        shutdown: shutdown_rx,
    };
    let mut running = start_components(&components, &options.without, context).await;

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
                match handle(&store, &mut running, line.trim_start()).await {
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

    stop(shutdown_tx, running, persistence, persister).await?;
    outcome
}

async fn stop(
    shutdown: watch::Sender<bool>,
    mut running: Vec<Box<dyn RunningComponent>>,
    persistence: PersistenceHandle,
    persister: tokio::task::JoinHandle<()>,
) -> AppResult<()> {
    let _ = shutdown.send(true);
    for component in &mut running {
        if let Err(error) = component.quiesce().await {
            eprintln!("component {} quiesce: {error}", component.name());
        }
    }
    for component in &mut running {
        if let Err(error) = component.shutdown().await {
            eprintln!("component {} shutdown: {error}", component.name());
        }
    }
    let flush_result = persistence.flush().await;
    // Components hold persistence handles; the persister stops once every handle is dropped.
    drop(running);
    drop(persistence);
    let _ = persister.await;
    Ok(flush_result?)
}

async fn start_components(
    components: &[Box<dyn Component>],
    disabled: &[String],
    context: component::Context,
) -> Vec<Box<dyn RunningComponent>> {
    let mut running = Vec::new();
    for component in components {
        if disabled.iter().any(|name| name == component.name()) {
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
    running
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
    let (command, args) = split_command(line);
    if command.is_empty() {
        return Ok(Continue::Yes);
    }
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
            component.handle_command(args.unwrap_or_default()).await?;
        }
    }
    Ok(Continue::Yes)
}

fn split_command(line: &str) -> (&str, Option<&str>) {
    match line.find(char::is_whitespace) {
        Some(index) => (
            &line[..index],
            Some(line[index..].trim_start_matches(char::is_whitespace)),
        ),
        None => (line, None),
    }
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

    struct FinalPublisher {
        release: Option<tokio::sync::oneshot::Sender<()>>,
        worker: Option<tokio::task::JoinHandle<()>>,
    }

    impl RunningComponent for FinalPublisher {
        fn name(&self) -> &'static str {
            "final-publisher"
        }

        fn handle_command<'a>(&'a mut self, _args: &'a str) -> component::ComponentFuture<'a, ()> {
            Box::pin(async { Err("no commands".into()) })
        }

        fn quiesce<'a>(&'a mut self) -> component::ComponentFuture<'a, ()> {
            Box::pin(async move {
                if let Some(release) = self.release.take() {
                    release
                        .send(())
                        .map_err(|_| "publisher stopped".to_string())?;
                }
                if let Some(worker) = self.worker.take() {
                    worker.await.map_err(|error| error.to_string())?;
                }
                Ok(())
            })
        }
    }

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

    #[test]
    fn component_arguments_preserve_internal_and_trailing_spaces() {
        assert_eq!(
            split_command("journal file events  archive.log"),
            ("journal", Some("file events  archive.log"))
        );
        assert_eq!(
            split_command("heartbeat message still  here "),
            ("heartbeat", Some("message still  here "))
        );
    }

    #[tokio::test]
    async fn stopping_flushes_and_ends_while_components_hold_persistence_handles() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.toml");
        let store = ConfigStore::new();
        let (persist_tx, persist_rx) = mpsc::unbounded_channel();
        let persistence = PersistenceHandle::new(persist_tx);
        let persister = tokio::spawn(persist::run(store.clone(), path.clone(), persist_rx));
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let context = component::Context {
            store: store.clone(),
            config_path: path.clone(),
            persistence: persistence.clone(),
            events,
            shutdown,
        };
        let running = start_components(&components(), &[], context).await;
        assert_eq!(running.len(), 3);

        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stop(shutdown_tx, running, persistence, persister),
        )
        .await
        .expect("shutdown must not wait for handles held by components")
        .unwrap();

        let saved = ConfigStore::from_file(&path).await.unwrap();
        for section in ["Heartbeat", "Reminders", "Journal"] {
            assert!(saved.has_section(section));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stopping_drains_queued_and_final_events_regardless_of_component_order() {
        for journal_first in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let config_path = directory.path().join("agent.toml");
            let journal_path = directory.path().join("journal.log");
            let source = format!(
                "[Journal]\nkeep = 10\n[Journal.file]\npath = {:?}\n",
                journal_path.to_string_lossy()
            );
            let store = ConfigStore::from_str(&source).unwrap();
            let (persist_tx, persist_rx) = mpsc::unbounded_channel();
            let persistence = PersistenceHandle::new(persist_tx);
            let persister =
                tokio::spawn(persist::run(store.clone(), config_path.clone(), persist_rx));
            let (shutdown_tx, shutdown) = watch::channel(false);
            let (events, _) = broadcast::channel(4);
            let context = component::Context {
                store,
                config_path,
                persistence: persistence.clone(),
                events: events.clone(),
                shutdown: shutdown.clone(),
            };
            let journal = components::journal::Journal.start(context).await.unwrap();
            let mut publisher_shutdown = shutdown;
            let publisher_events = events.clone();
            let (release, released) = tokio::sync::oneshot::channel();
            let publisher = FinalPublisher {
                release: Some(release),
                worker: Some(tokio::spawn(async move {
                    publisher_shutdown.changed().await.unwrap();
                    released.await.unwrap();
                    publisher_events
                        .send(events::Event {
                            source: "test".into(),
                            message: "during shutdown".into(),
                        })
                        .unwrap();
                })),
            };
            events
                .send(events::Event {
                    source: "test".into(),
                    message: "before shutdown".into(),
                })
                .unwrap();
            let running: Vec<Box<dyn RunningComponent>> = if journal_first {
                vec![journal, Box::new(publisher)]
            } else {
                vec![Box::new(publisher), journal]
            };

            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                stop(shutdown_tx, running, persistence, persister),
            )
            .await
            .expect("shutdown must finish")
            .unwrap();

            assert_eq!(
                tokio::fs::read_to_string(journal_path).await.unwrap(),
                "test: before shutdown\ntest: during shutdown\n"
            );
        }
    }

    #[tokio::test]
    async fn stopping_works_with_components_disabled() {
        for disabled in [
            vec!["journal".into()],
            vec!["heartbeat".into(), "reminders".into()],
            vec!["heartbeat".into(), "reminders".into(), "journal".into()],
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("agent.toml");
            let store = ConfigStore::new();
            let (persist_tx, persist_rx) = mpsc::unbounded_channel();
            let persistence = PersistenceHandle::new(persist_tx);
            let persister = tokio::spawn(persist::run(store.clone(), path.clone(), persist_rx));
            let (shutdown_tx, shutdown) = watch::channel(false);
            let (events, _) = broadcast::channel(4);
            let context = component::Context {
                store,
                config_path: path,
                persistence: persistence.clone(),
                events,
                shutdown,
            };
            let running = start_components(&components(), &disabled, context).await;

            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                stop(shutdown_tx, running, persistence, persister),
            )
            .await
            .expect("shutdown must finish with disabled components")
            .unwrap();
        }
    }

    #[tokio::test]
    async fn disabled_component_leaves_its_configuration_untouched() {
        let store =
            ConfigStore::from_str("[Journal]\nkeep = 7\n[Journal.file]\npath = \"journal.log\"\n")
                .unwrap();
        let before = store.save_str().unwrap();
        let (persist_tx, mut persist_rx) = mpsc::unbounded_channel();
        let persistence = PersistenceHandle::new(persist_tx);
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let context = component::Context {
            store: store.clone(),
            config_path: PathBuf::from("agent.toml"),
            persistence,
            events,
            shutdown,
        };
        let components = vec![Box::new(components::journal::Journal) as Box<dyn Component>];

        let running = start_components(&components, &["journal".into()], context).await;

        assert!(running.is_empty());
        assert_eq!(store.save_str().unwrap(), before);
        assert!(matches!(
            persist_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ));
        let _ = shutdown_tx.send(true);
    }

    #[tokio::test]
    async fn invalid_component_keeps_its_section_and_does_not_stop_others() {
        let store = ConfigStore::from_str("[Journal]\nkeep = 0\n").unwrap();
        let before = store.get_section("Journal").unwrap();
        let (persist_tx, _persist_rx) = mpsc::unbounded_channel();
        let persistence = PersistenceHandle::new(persist_tx);
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let context = component::Context {
            store: store.clone(),
            config_path: PathBuf::from("agent.toml"),
            persistence,
            events,
            shutdown,
        };

        let mut running = start_components(&components(), &[], context).await;

        assert_eq!(
            running
                .iter()
                .map(|component| component.name())
                .collect::<Vec<_>>(),
            ["heartbeat", "reminders"]
        );
        assert_eq!(store.get_section("Journal").unwrap(), before);
        let _ = shutdown_tx.send(true);
        for component in &mut running {
            component.quiesce().await.unwrap();
        }
        for component in &mut running {
            component.shutdown().await.unwrap();
        }
    }
}
