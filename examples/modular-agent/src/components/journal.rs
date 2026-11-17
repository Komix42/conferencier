use std::collections::VecDeque;
use std::path::Path;

use conferencier::{ConfigScope, ConfigTable, SharedScope};
use tokio::io::AsyncWriteExt;
use tokio::sync::{broadcast, watch};
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
            context.save_defaults("Journal", &config).await?;

            let worker = tokio::spawn(run_worker(
                config.clone(),
                receiver,
                context.shutdown.clone(),
            ));
            Ok(Box::new(JournalInstance {
                config,
                context,
                worker: Some(worker),
            }) as Box<dyn RunningComponent>)
        })
    }
}

struct JournalInstance {
    config: SharedScope<JournalConfig>,
    context: Context,
    worker: Option<JoinHandle<()>>,
}

impl RunningComponent for JournalInstance {
    fn name(&self) -> &'static str {
        "journal"
    }

    fn handle_command<'a>(&'a mut self, args: &'a str) -> ComponentFuture<'a, ()> {
        Box::pin(async move {
            let mut words = args.splitn(2, ' ');
            let command = words.next().unwrap_or_default();
            let value = words.next().map(str::trim_start);
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
            if let Some(worker) = self.worker.take() {
                worker.await.map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    }
}

async fn run_worker(
    config: SharedScope<JournalConfig>,
    mut events: broadcast::Receiver<Event>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            result = events.recv() => {
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
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    return;
                }
            }
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
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
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
    use tokio::sync::broadcast;

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
            persistence: PersistenceHandle::new(persist_tx),
            events,
            shutdown,
        };
        let mut component = Journal.start(context).await.unwrap();
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        component.handle_command("file journal.log").await.unwrap();
        assert!(store.save_str().unwrap().contains("[Journal.file]"));
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        component.handle_command("file off").await.unwrap();
        assert!(!store.save_str().unwrap().contains("[Journal.file]"));
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        let _ = shutdown_tx.send(true);
        component.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn component_receives_published_events() {
        let scope = config("").unwrap();
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let worker = tokio::spawn(run_worker(scope.clone(), events.subscribe(), shutdown));
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

        let _ = shutdown_tx.send(true);
        worker.await.unwrap();
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
