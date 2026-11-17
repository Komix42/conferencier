use std::collections::VecDeque;
use std::path::{Component as PathComponent, Path, PathBuf};

use conferencier::{ConfigScope, ConfigTable, SharedScope};
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

use crate::component::{Component, ComponentFuture, ComponentResult, Context, RunningComponent};
use crate::events::Event;

#[derive(ConfigScope)]
#[confer(section = "Journal", deny_unknown_keys)]
struct JournalConfig {
    #[confer(default = 100)]
    keep: u32,
    #[confer(table)]
    file: Option<JournalFile>,
    #[confer(ignore = VecDeque::new())]
    events: VecDeque<Event>,
}

#[derive(Clone, ConfigTable)]
#[confer(deny_unknown_keys)]
struct JournalFile {
    path: String,
}

pub struct Journal;

impl Component for Journal {
    fn name(&self) -> &'static str {
        "journal"
    }

    fn section(&self) -> &'static str {
        "Journal"
    }

    fn start<'a>(&'a self, context: Context) -> ComponentFuture<'a, Box<dyn RunningComponent>> {
        Box::pin(async move {
            let receiver = context.events.subscribe();
            let config =
                JournalConfig::from_store(&context.store).map_err(|error| error.to_string())?;
            validate(&*config.read().await)?;
            let file_path = config
                .read()
                .await
                .file
                .as_ref()
                .map(|file| file.path.clone());
            if let Some(path) = file_path {
                validate_output_path(Path::new(&path), &context.config_path).await?;
            }
            context.save_defaults("Journal", &config).await?;

            let (finish, finish_signal) = oneshot::channel();
            let worker = tokio::spawn(run_worker(config.clone(), receiver, finish_signal));
            Ok(Box::new(JournalInstance {
                config,
                context,
                finish: Some(finish),
                worker: Some(worker),
            }) as Box<dyn RunningComponent>)
        })
    }
}

struct JournalInstance {
    config: SharedScope<JournalConfig>,
    context: Context,
    finish: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl RunningComponent for JournalInstance {
    fn name(&self) -> &'static str {
        "journal"
    }

    fn handle_command<'a>(&'a mut self, args: &'a str) -> ComponentFuture<'a, ()> {
        Box::pin(async move {
            let (command, value) = split_command(args);
            match command {
                "show" if value.is_none() => {
                    let config = self.config.read().await;
                    if config.events.is_empty() {
                        println!("No events recorded");
                    }
                    for event in &config.events {
                        println!("{}: {}", event.source, event.message);
                    }
                }
                "file" => {
                    let value = value.ok_or("usage: journal file <path|off>")?;
                    let replacement = if value == "off" {
                        None
                    } else if value.trim().is_empty() {
                        return Err("journal file path must not be empty".into());
                    } else {
                        validate_output_path(Path::new(value), &self.context.config_path).await?;
                        Some(JournalFile {
                            path: value.to_owned(),
                        })
                    };
                    let previous = {
                        let mut config = self.config.write().await;
                        std::mem::replace(&mut config.file, replacement)
                    };
                    if let Err(error) = self.context.save(&self.config).await {
                        self.config.write().await.file = previous;
                        return Err(error);
                    }
                    match value {
                        "off" => println!("Journal file output disabled"),
                        path => println!("Journal events will be appended to {path:?}"),
                    }
                }
                "help" if value.is_none() => print_help(),
                _ => return Err("usage: journal <show|file <path|off>|help>".into()),
            }
            Ok(())
        })
    }

    fn shutdown<'a>(&'a mut self) -> ComponentFuture<'a, ()> {
        Box::pin(async move {
            if let Some(finish) = self.finish.take() {
                let _ = finish.send(());
            }
            if let Some(worker) = self.worker.take() {
                worker.await.map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    }
}

fn split_command(args: &str) -> (&str, Option<&str>) {
    match args.find(char::is_whitespace) {
        Some(index) => (
            &args[..index],
            Some(args[index..].trim_start_matches(char::is_whitespace)),
        ),
        None => (args, None),
    }
}

async fn run_worker(
    config: SharedScope<JournalConfig>,
    mut events: broadcast::Receiver<Event>,
    mut finish: oneshot::Receiver<()>,
) {
    let mut draining = false;
    loop {
        let result = if draining {
            match events.try_recv() {
                Ok(event) => Ok(event),
                Err(broadcast::error::TryRecvError::Lagged(count)) => {
                    Err(broadcast::error::RecvError::Lagged(count))
                }
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => return,
            }
        } else {
            tokio::select! {
                biased;
                _ = &mut finish => {
                    draining = true;
                    continue;
                }
                result = events.recv() => result,
            }
        };
        match result {
            Ok(event) => {
                if let Err(error) = record_event(&config, event).await {
                    eprintln!("journal: {error}");
                }
            }
            Err(broadcast::error::RecvError::Lagged(count)) => {
                eprintln!("journal missed {count} events because its receiver lagged");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

async fn record_event(config: &SharedScope<JournalConfig>, event: Event) -> ComponentResult<()> {
    let line = format!("{}: {}\n", event.source, event.message);
    let path = {
        let mut config = config.write().await;
        config.events.push_back(event);
        while config.events.len() > config.keep as usize {
            config.events.pop_front();
        }
        config.file.as_ref().map(|file| file.path.clone())
    };
    if let Some(path) = path {
        append_line(Path::new(&path), line.as_bytes()).await?;
    }
    Ok(())
}

async fn append_line(path: &Path, line: &[u8]) -> ComponentResult<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    file.write_all(line)
        .await
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    file.flush()
        .await
        .map_err(|error| format!("cannot flush {}: {error}", path.display()))
}

async fn validate_output_path(path: &Path, config_path: &Path) -> ComponentResult<()> {
    let output_path = resolve_path(path).await?;
    let config_path_resolved = resolve_path(config_path).await?;
    if same_path(&output_path, &config_path_resolved) {
        return Err(format!(
            "journal file path {} resolves to the configuration file {}",
            path.display(),
            config_path.display()
        ));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

async fn resolve_path(path: &Path) -> ComponentResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?
            .join(path)
    };
    let absolute = normalize_path(&absolute);
    let mut ancestor = absolute.as_path();
    let mut suffix = Vec::new();

    loop {
        match tokio::fs::canonicalize(ancestor).await {
            Ok(mut resolved) => {
                for component in suffix.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = ancestor.file_name() else {
                    return Err(format!(
                        "cannot resolve {}: no existing parent directory",
                        path.display()
                    ));
                };
                suffix.push(name.to_owned());
                ancestor = ancestor.parent().ok_or_else(|| {
                    format!(
                        "cannot resolve {}: no existing parent directory",
                        path.display()
                    )
                })?;
            }
            Err(error) => {
                return Err(format!("cannot resolve {}: {error}", path.display()));
            }
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            PathComponent::CurDir => {}
            PathComponent::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn validate(config: &JournalConfig) -> ComponentResult<()> {
    if config.keep == 0 {
        return Err("Journal.keep must be greater than zero".into());
    }
    if let Some(file) = &config.file {
        if file.path.trim().is_empty() {
            return Err("Journal.file.path must not be empty".into());
        }
    }
    Ok(())
}

fn print_help() {
    println!("journal commands: show, file <path|off>, help");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::{PersistMessage, PersistenceHandle};
    use conferencier::{ConfigStore, Error};
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::{broadcast, watch};

    fn config(source: &str) -> Result<SharedScope<JournalConfig>, Error> {
        JournalConfig::from_store(&ConfigStore::from_str(source)?)
    }

    #[tokio::test]
    async fn defaults_and_invalid_values() {
        let scope = config("").unwrap();
        let current = scope.read().await;
        validate(&current).unwrap();
        assert_eq!(current.keep, 100);
        assert!(current.file.is_none());
        assert!(current.events.is_empty());
        drop(current);

        let scope = config("[Journal]\nkeep = 0\n").unwrap();
        assert!(validate(&*scope.read().await).is_err());
        let empty_path = "[Journal.file]\npath = \" \"\n";
        let scope = config(empty_path).unwrap();
        assert!(validate(&*scope.read().await).is_err());
    }

    #[test]
    fn rejects_unknown_keys_in_section_and_file_table() {
        assert!(matches!(
            config("[Journal]\nunknwon = true\n"),
            Err(Error::UnknownKey { .. })
        ));
        assert!(matches!(
            config("[Journal.file]\npath = \"journal.log\"\nunknown = true\n"),
            Err(Error::UnknownKey { .. })
        ));
    }

    #[tokio::test]
    async fn records_only_the_most_recent_events_and_appends_to_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("journal.log");
        let source = format!(
            "[Journal]\nkeep = 1\n[Journal.file]\npath = {:?}\n",
            path.to_string_lossy()
        );
        let config = config(&source).unwrap();

        record_event(
            &config,
            Event {
                source: "heartbeat".into(),
                message: "first".into(),
            },
        )
        .await
        .unwrap();
        record_event(
            &config,
            Event {
                source: "reminders".into(),
                message: "second".into(),
            },
        )
        .await
        .unwrap();

        let current = config.read().await;
        let events = &current.events;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, "reminders");
        assert_eq!(events[0].message, "second");
        assert_eq!(
            tokio::fs::read_to_string(path).await.unwrap(),
            "heartbeat: first\nreminders: second\n"
        );
    }

    #[tokio::test]
    async fn file_command_adds_and_removes_the_optional_table() {
        let store = ConfigStore::new();
        let (persist_tx, mut persist_rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let context = Context {
            store: store.clone(),
            config_path: std::path::PathBuf::from("agent.toml"),
            persistence: PersistenceHandle::new(persist_tx),
            events,
            shutdown,
        };
        let mut component = Journal.start(context).await.unwrap();
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        component
            .handle_command("file events  archive.log")
            .await
            .unwrap();
        assert!(store.save_str().unwrap().contains("[Journal.file]"));
        let journal_section = store.get_section("Journal").unwrap();
        let journal_file = journal_section
            .get("file")
            .and_then(conferencier::toml::Value::as_table)
            .unwrap()
            .get("path")
            .and_then(conferencier::toml::Value::as_str);
        assert_eq!(journal_file, Some("events  archive.log"));
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        component.handle_command("file off").await.unwrap();
        assert!(!store.save_str().unwrap().contains("[Journal.file]"));
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        let _ = shutdown_tx.send(true);
        component.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn refuses_to_use_the_configuration_file_as_journal_output() {
        let directory = tempdir().unwrap();
        let config_path = directory.path().join("agent.toml");
        tokio::fs::write(&config_path, "[Journal]\nkeep = 100\n")
            .await
            .unwrap();
        let store = ConfigStore::from_file(&config_path).await.unwrap();
        let (persist_tx, mut persist_rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let context = Context {
            store: store.clone(),
            config_path: config_path.clone(),
            persistence: PersistenceHandle::new(persist_tx),
            events,
            shutdown,
        };
        let mut component = Journal.start(context).await.unwrap();

        let alias = config_path
            .parent()
            .unwrap()
            .join("nested")
            .join("..")
            .join("agent.toml");
        tokio::fs::create_dir(config_path.parent().unwrap().join("nested"))
            .await
            .unwrap();
        let error = component
            .handle_command(&format!("file {}", alias.display()))
            .await
            .unwrap_err();
        assert!(
            error.contains("resolves to the configuration file"),
            "{error}"
        );
        assert!(store.get_section("Journal").unwrap().get("file").is_none());
        assert!(matches!(
            persist_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));

        component
            .handle_command(&format!("file {}", config_path.display()))
            .await
            .unwrap_err();
        assert!(store.get_section("Journal").unwrap().get("file").is_none());
        let _ = shutdown_tx.send(true);
        component.shutdown().await.unwrap();
        assert_eq!(
            tokio::fs::read_to_string(config_path).await.unwrap(),
            "[Journal]\nkeep = 100\n"
        );
    }

    #[tokio::test]
    async fn skips_start_when_existing_journal_path_is_the_configuration_file() {
        let directory = tempdir().unwrap();
        let config_path = directory.path().join("agent.toml");
        tokio::fs::write(&config_path, "[Journal]\nkeep = 100\n")
            .await
            .unwrap();
        let source = format!(
            "[Journal]\nkeep = 100\n[Journal.file]\npath = {:?}\n",
            config_path.to_string_lossy()
        );
        let store = ConfigStore::from_str(&source).unwrap();
        let (persist_tx, _persist_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let context = Context {
            store,
            config_path,
            persistence: PersistenceHandle::new(persist_tx),
            events,
            shutdown,
        };

        match Journal.start(context).await {
            Err(error) => assert!(error.contains("resolves to the configuration file")),
            Ok(_) => panic!("journal started with the configuration file as its output"),
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn rejects_case_variant_of_a_not_yet_created_configuration_path() {
        let directory = tempdir().unwrap();
        let config_path = directory.path().join("agent.toml");
        let output_path = directory.path().join("AGENT.TOML");

        let error = validate_output_path(&output_path, &config_path)
            .await
            .unwrap_err();
        assert!(
            error.contains("resolves to the configuration file"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn component_receives_published_events() {
        let scope = config("").unwrap();
        let (finish, finish_signal) = oneshot::channel();
        let (events, _) = broadcast::channel(4);
        let worker = tokio::spawn(run_worker(scope.clone(), events.subscribe(), finish_signal));
        events
            .send(Event {
                source: "heartbeat".into(),
                message: "still here".into(),
            })
            .unwrap();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if scope.read().await.events.len() == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let _ = finish.send(());
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_drains_events_already_queued() {
        let scope = config("").unwrap();
        let (finish, finish_signal) = oneshot::channel();
        let (events, _) = broadcast::channel(4);
        let receiver = events.subscribe();
        events
            .send(Event {
                source: "test".into(),
                message: "queued".into(),
            })
            .unwrap();
        finish.send(()).unwrap();

        run_worker(scope.clone(), receiver, finish_signal).await;

        assert_eq!(scope.read().await.events[0].message, "queued");
    }

    #[tokio::test]
    async fn shutdown_drains_retained_events_after_receiver_lags() {
        let scope = config("").unwrap();
        let (finish, finish_signal) = oneshot::channel();
        let (events, _) = broadcast::channel(2);
        let receiver = events.subscribe();
        for index in 0..4 {
            events
                .send(Event {
                    source: "test".into(),
                    message: index.to_string(),
                })
                .unwrap();
        }
        finish.send(()).unwrap();
        run_worker(scope.clone(), receiver, finish_signal).await;

        let config = scope.read().await;
        let messages: Vec<&str> = config
            .events
            .iter()
            .map(|event| event.message.as_str())
            .collect();
        assert_eq!(messages, ["2", "3"]);
    }

    #[tokio::test]
    async fn reports_file_errors_without_dropping_the_runtime_event() {
        let config = config("[Journal]\nkeep = 1\n[Journal.file]\npath = \"\"\n").unwrap();
        let error = record_event(
            &config,
            Event {
                source: "test".into(),
                message: "event".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("cannot open"));
        assert_eq!(config.read().await.events.len(), 1);
    }
}
