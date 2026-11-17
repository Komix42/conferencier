//! Timed reminders that publish events when due.

use std::time::{Duration, Instant};

use conferencier::{ConfigScope, ConfigTable, SharedScope};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::component::{Component, ComponentFuture, ComponentResult, Context, RunningComponent};
use crate::events::Event;
use crate::split_command;

const SECTION: &str = "Reminders";

/// Settings of reminders, the `[Reminders]` section.
#[derive(ConfigScope)]
#[confer(section = "Reminders", deny_unknown_keys)]
struct ReminderConfig {
    /// A missing `[[Reminders.items]]` array loads as an empty list.
    #[confer(table, default)]
    items: Vec<Reminder>,
}

/// One `[[Reminders.items]]` table; unknown keys are rejected here too.
#[derive(Clone, ConfigTable)]
#[confer(deny_unknown_keys)]
struct Reminder {
    name: String,
    after_secs: u64,
    message: String,
    /// Runtime timer, initialized when loaded or added; not saved in TOML.
    #[confer(ignore = Instant::now())]
    added_at: Instant,
    /// Runtime state inside a `ConfigTable`: a saved reminder fires again on restart.
    #[confer(ignore = false)]
    fired: bool,
}

pub struct Reminders;

impl Component for Reminders {
    fn name(&self) -> &'static str {
        "reminders"
    }

    fn section(&self) -> &'static str {
        SECTION
    }

    fn start<'a>(&'a self, context: Context) -> ComponentFuture<'a, Box<dyn RunningComponent>> {
        Box::pin(async move {
            let config =
                ReminderConfig::from_store(&context.store).map_err(|error| error.to_string())?;
            validate(&*config.read().await)?;
            context.save_defaults(SECTION, &config).await?;

            let worker = tokio::spawn(run_worker(
                config.clone(),
                context.clone(),
                context.shutdown.clone(),
            ));
            Ok(Box::new(RemindersInstance {
                config,
                context,
                worker: Some(worker),
            }) as Box<dyn RunningComponent>)
        })
    }
}

struct RemindersInstance {
    config: SharedScope<ReminderConfig>,
    context: Context,
    worker: Option<JoinHandle<()>>,
}

impl RunningComponent for RemindersInstance {
    fn name(&self) -> &'static str {
        "reminders"
    }

    fn handle_command<'a>(&'a mut self, args: &'a str) -> ComponentFuture<'a, ()> {
        Box::pin(async move {
            let (command, value) = split_command(args);
            match command {
                "list" if value.is_none() => {
                    let config = self.config.read().await;
                    if config.items.is_empty() {
                        println!("No reminders configured");
                    }
                    for reminder in &config.items {
                        let state = if reminder.fired {
                            "done".to_owned()
                        } else {
                            let remaining = Duration::from_secs(reminder.after_secs)
                                .saturating_sub(reminder.added_at.elapsed());
                            format!("in {}s", remaining.as_secs())
                        };
                        println!("{}: {} ({state})", reminder.name, reminder.message);
                    }
                }
                "add" => {
                    let (name, rest) = value
                        .map(split_command)
                        .and_then(|(name, rest)| Some((name, rest?)))
                        .ok_or("usage: reminders add <name> <seconds> <message>")?;
                    let (seconds, message) = match split_command(rest) {
                        (seconds, Some(message)) => (seconds, message),
                        _ => return Err("usage: reminders add <name> <seconds> <message>".into()),
                    };
                    let after_secs = seconds
                        .parse::<u64>()
                        .map_err(|_| "seconds must be a positive integer")?;
                    let reminder = Reminder {
                        name: name.to_owned(),
                        after_secs,
                        message: message.to_owned(),
                        added_at: Instant::now(),
                        fired: false,
                    };
                    {
                        let mut config = self.config.write().await;
                        config.items.push(reminder);
                        if let Err(error) = validate(&config) {
                            config.items.pop();
                            return Err(error);
                        }
                    }
                    if let Err(error) = self.context.save(SECTION, &self.config).await {
                        let mut config = self.config.write().await;
                        if let Some(index) = config
                            .items
                            .iter()
                            .position(|reminder| reminder.name == name)
                        {
                            config.items.remove(index);
                        }
                        return Err(error);
                    }
                    println!("Added reminder {name:?}");
                }
                "remove" => {
                    let name = value.ok_or("usage: reminders remove <name>")?;
                    let (index, removed) = {
                        let mut config = self.config.write().await;
                        let index = config
                            .items
                            .iter()
                            .position(|reminder| reminder.name == name)
                            .ok_or_else(|| format!("no reminder named {name:?}"))?;
                        (index, config.items.remove(index))
                    };
                    if let Err(error) = self.context.save(SECTION, &self.config).await {
                        self.config.write().await.items.insert(index, removed);
                        return Err(error);
                    }
                    println!("Removed reminder {name:?}");
                }
                "help" if value.is_none() => print_help(),
                _ => return Err("usage: reminders <list|add|remove|help>".into()),
            }
            Ok(())
        })
    }

    fn quiesce<'a>(&'a mut self) -> ComponentFuture<'a, ()> {
        Box::pin(async move {
            if let Some(worker) = self.worker.take() {
                worker.await.map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    }
}

async fn run_worker(
    config: SharedScope<ReminderConfig>,
    context: Context,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = timer.tick() => {
                let due = {
                    let mut config = config.write().await;
                    config.items.iter_mut()
                        .filter(|reminder| {
                            !reminder.fired
                                && reminder.added_at.elapsed() >= Duration::from_secs(reminder.after_secs)
                        })
                        .map(|reminder| {
                            reminder.fired = true;
                            (reminder.name.clone(), reminder.message.clone())
                        })
                        .collect::<Vec<_>>()
                };
                for (name, message) in due {
                    println!("reminder {name}: {message}");
                    context.publish(Event {
                        source: "reminders".into(),
                        message: format!("{name}: {message}"),
                    });
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

/// Checks application rules after conferencier converts the field types.
fn validate(config: &ReminderConfig) -> ComponentResult<()> {
    let mut names = std::collections::HashSet::new();
    for (index, reminder) in config.items.iter().enumerate() {
        if reminder.name.trim().is_empty() {
            return Err(format!("Reminders.items[{index}].name must not be empty"));
        }
        if reminder.after_secs == 0 || reminder.after_secs > i64::MAX as u64 {
            return Err(format!(
                "Reminders.items[{index}].after_secs must be between 1 and i64::MAX"
            ));
        }
        if reminder.message.trim().is_empty() {
            return Err(format!(
                "Reminders.items[{index}].message must not be empty"
            ));
        }
        if !names.insert(reminder.name.as_str()) {
            return Err(format!("duplicate reminder name {:?}", reminder.name));
        }
    }
    Ok(())
}

fn print_help() {
    println!("reminders commands: list, add <name> <seconds> <message>, remove <name>, help");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::{PersistMessage, PersistenceHandle};
    use conferencier::{ConfigStore, Error};
    use tokio::sync::broadcast;

    fn config(source: &str) -> Result<SharedScope<ReminderConfig>, Error> {
        ReminderConfig::from_store(&ConfigStore::from_str(source)?)
    }

    #[tokio::test]
    async fn missing_section_defaults_to_empty_items() {
        let config = config("").unwrap();
        assert!(config.read().await.items.is_empty());
    }

    #[tokio::test]
    async fn starting_adds_an_empty_strict_section_without_changing_other_sections() {
        let store = ConfigStore::from_str("[Other]\nvalue = 1\n").unwrap();
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
        let mut component = Reminders.start(context).await.unwrap();

        assert!(store.has_section("Reminders"));
        assert!(store.has_section("Other"));
        assert!(matches!(persist_rx.try_recv(), Ok(PersistMessage::Changed)));

        let _ = shutdown_tx.send(true);
        component.quiesce().await.unwrap();
    }

    #[tokio::test]
    async fn add_and_remove_commands_save_the_reminder_table() {
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
        let mut component = Reminders.start(context).await.unwrap();
        persist_rx.try_recv().unwrap();

        component
            .handle_command("add tea 60 water is ready")
            .await
            .unwrap();
        assert!(
            store
                .save_str()
                .unwrap()
                .contains("message = \"water is ready\"")
        );
        persist_rx.try_recv().unwrap();

        component
            .handle_command("add\tcoffee\t30\tcoffee  is ready")
            .await
            .unwrap();
        assert!(
            store
                .save_str()
                .unwrap()
                .contains("message = \"coffee  is ready\"")
        );
        persist_rx.try_recv().unwrap();

        component.handle_command("remove tea").await.unwrap();
        assert!(!store.save_str().unwrap().contains("name = \"tea\""));
        persist_rx.try_recv().unwrap();

        let _ = shutdown_tx.send(true);
        component.quiesce().await.unwrap();
    }

    #[tokio::test]
    async fn failed_commands_restore_reminders_and_store() {
        let store = ConfigStore::from_str(
            "[[Reminders.items]]\nname = 'tea'\nafter_secs = 60\nmessage = 'ready'\n\
             [Other]\nvalue = 1\n",
        )
        .unwrap();
        let before = store.save_str().unwrap();
        let config = ReminderConfig::from_store(&store).unwrap();
        config.write().await.items[0].fired = true;
        let (persist_tx, persist_rx) = tokio::sync::mpsc::unbounded_channel();
        drop(persist_rx);
        let (_shutdown_tx, shutdown) = watch::channel(false);
        let (events, _) = broadcast::channel(4);
        let mut component = RemindersInstance {
            config: config.clone(),
            context: Context {
                store: store.clone(),
                config_path: std::path::PathBuf::from("agent.toml"),
                persistence: PersistenceHandle::new(persist_tx),
                events,
                shutdown,
            },
            worker: None,
        };

        for command in ["add coffee 30 ready", "remove tea"] {
            assert_eq!(
                component.handle_command(command).await.unwrap_err(),
                "configuration persister has stopped"
            );
            let config = config.read().await;
            assert_eq!(config.items.len(), 1);
            assert_eq!(config.items[0].name, "tea");
            assert!(config.items[0].fired);
            assert_eq!(store.save_str().unwrap(), before);
        }
    }

    #[tokio::test]
    async fn validates_required_fields_unique_names_and_positive_delays() {
        let duplicate = "[[Reminders.items]]\nname = \"tea\"\nafter_secs = 2\nmessage = \"ready\"\n[[Reminders.items]]\nname = \"tea\"\nafter_secs = 3\nmessage = \"again\"\n";
        let scope = config(duplicate).unwrap();
        assert!(validate(&*scope.read().await).is_err());

        let zero = "[[Reminders.items]]\nname = \"tea\"\nafter_secs = 0\nmessage = \"ready\"\n";
        let scope = config(zero).unwrap();
        assert!(validate(&*scope.read().await).is_err());

        let empty = "[[Reminders.items]]\nname = \" \"\nafter_secs = 1\nmessage = \"ready\"\n";
        let scope = config(empty).unwrap();
        assert!(validate(&*scope.read().await).is_err());
    }

    #[test]
    fn rejects_unknown_keys_in_section_and_item_tables() {
        assert!(matches!(
            config("[Reminders]\nunknwon = true\n"),
            Err(Error::UnknownKey { .. })
        ));
        assert!(matches!(
            config(
                "[[Reminders.items]]\nname = \"tea\"\nafter_secs = 2\nmessage = \"ready\"\nunknwon = true\n"
            ),
            Err(Error::UnknownKey { .. })
        ));
    }
}
