use std::sync::Arc;
use std::time::Duration;

use conferencier::{ConfigScope, SharedScope};
use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;

use crate::component::{Component, ComponentFuture, ComponentResult, Context, RunningComponent};
use crate::events::Event;

#[derive(ConfigScope)]
#[confer(section = "Heartbeat")]
struct HeartbeatConfig {
    #[confer(default = true)]
    enabled: bool,
    #[confer(default = 10)]
    interval_secs: u64,
    #[confer(default = "still here".into())]
    message: String,
    #[confer(ignore = 0)]
    ticks: u64,
}

pub struct Heartbeat;

impl Component for Heartbeat {
    fn name(&self) -> &'static str {
        "heartbeat"
    }

    fn section(&self) -> &'static str {
        "Heartbeat"
    }

    fn start<'a>(&'a self, context: Context) -> ComponentFuture<'a, Box<dyn RunningComponent>> {
        Box::pin(async move {
            let config =
                HeartbeatConfig::from_store(&context.store).map_err(|error| error.to_string())?;
            validate(&*config.read().await)?;
            context.save_defaults("Heartbeat", &config).await?;

            let wake = Arc::new(Notify::new());
            let worker = tokio::spawn(run_worker(
                config.clone(),
                context.clone(),
                wake.clone(),
                context.shutdown.clone(),
            ));
            Ok(Box::new(HeartbeatInstance {
                config,
                context,
                wake,
                worker: Some(worker),
            }) as Box<dyn RunningComponent>)
        })
    }
}

struct HeartbeatInstance {
    config: SharedScope<HeartbeatConfig>,
    context: Context,
    wake: Arc<Notify>,
    worker: Option<JoinHandle<()>>,
}

impl RunningComponent for HeartbeatInstance {
    fn name(&self) -> &'static str {
        "heartbeat"
    }

    fn handle_command<'a>(&'a mut self, args: &'a str) -> ComponentFuture<'a, ()> {
        Box::pin(async move {
            let (command, value) = split_command(args);
            match command {
                "status" if value.is_none() => {
                    let config = self.config.read().await;
                    println!(
                        "Heartbeat is {}, interval {}s, message {:?}, {} ticks",
                        if config.enabled { "running" } else { "paused" },
                        config.interval_secs,
                        config.message,
                        config.ticks
                    );
                }
                "interval" => {
                    let seconds = value
                        .ok_or("usage: heartbeat interval <seconds>")?
                        .parse::<u64>()
                        .map_err(|_| "interval must be a positive integer")?;
                    if seconds == 0 || seconds > i64::MAX as u64 {
                        return Err("interval must be between 1 and i64::MAX seconds".into());
                    }
                    let previous = {
                        let mut config = self.config.write().await;
                        std::mem::replace(&mut config.interval_secs, seconds)
                    };
                    if let Err(error) = self.context.save(&self.config).await {
                        self.config.write().await.interval_secs = previous;
                        return Err(error);
                    }
                    self.wake.notify_one();
                    println!("Heartbeat interval set to {seconds}s");
                }
                "message" => {
                    let message = value.ok_or("usage: heartbeat message <text>")?;
                    if message.trim().is_empty() {
                        return Err("message must not be empty".into());
                    }
                    let previous = {
                        let mut config = self.config.write().await;
                        std::mem::replace(&mut config.message, message.to_owned())
                    };
                    if let Err(error) = self.context.save(&self.config).await {
                        self.config.write().await.message = previous;
                        return Err(error);
                    }
                    println!("Heartbeat message updated");
                }
                "pause" if value.is_none() => {
                    let previous = {
                        let mut config = self.config.write().await;
                        std::mem::replace(&mut config.enabled, false)
                    };
                    if let Err(error) = self.context.save(&self.config).await {
                        self.config.write().await.enabled = previous;
                        return Err(error);
                    }
                    self.wake.notify_one();
                    println!("Heartbeat paused");
                }
                "resume" if value.is_none() => {
                    let previous = {
                        let mut config = self.config.write().await;
                        std::mem::replace(&mut config.enabled, true)
                    };
                    if let Err(error) = self.context.save(&self.config).await {
                        self.config.write().await.enabled = previous;
                        return Err(error);
                    }
                    self.wake.notify_one();
                    println!("Heartbeat resumed");
                }
                "help" if value.is_none() => print_help(),
                _ => {
                    return Err(
                        "usage: heartbeat <status|interval|message|pause|resume|help>".into(),
                    );
                }
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

/// Sleeps for the current interval; a saved command wakes it to apply new settings at once.
async fn run_worker(
    config: SharedScope<HeartbeatConfig>,
    context: Context,
    wake: Arc<Notify>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let interval = Duration::from_secs(config.read().await.interval_secs);
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                let mut config = config.write().await;
                if config.enabled {
                    config.ticks += 1;
                    let message = config.message.clone();
                    println!("heartbeat: {message}");
                    context.publish(Event {
                        source: "heartbeat".into(),
                        message,
                    });
                }
            }
            _ = wake.notified() => {}
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }
    }
}

fn validate(config: &HeartbeatConfig) -> ComponentResult<()> {
    if config.interval_secs == 0 {
        return Err("Heartbeat.interval_secs must be greater than zero".into());
    }
    if config.message.trim().is_empty() {
        return Err("Heartbeat.message must not be empty".into());
    }
    Ok(())
}

fn split_command(args: &str) -> (&str, Option<&str>) {
    args.split_once(' ')
        .map_or((args, None), |(command, value)| {
            (command, Some(value.trim_start()))
        })
}

fn print_help() {
    println!("heartbeat commands: status, interval <seconds>, message <text>, pause, resume, help");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::PersistenceHandle;
    use conferencier::ConfigStore;
    use tokio::sync::broadcast;

    fn config(source: &str) -> Result<SharedScope<HeartbeatConfig>, conferencier::Error> {
        HeartbeatConfig::from_store(&ConfigStore::from_str(source)?)
    }

    #[tokio::test]
    async fn defaults_are_loaded_and_validated() {
        let config = config("").unwrap();
        let config = config.read().await;
        validate(&config).unwrap();
        assert!(config.enabled);
        assert_eq!(config.interval_secs, 10);
        assert_eq!(config.message, "still here");
        assert_eq!(config.ticks, 0);
    }

    #[tokio::test]
    async fn invalid_values_are_rejected() {
        let scope = config("[Heartbeat]\ninterval_secs = 0\n").unwrap();
        assert!(validate(&*scope.read().await).is_err());
        let scope = config("[Heartbeat]\nmessage = \" \"\n").unwrap();
        assert!(validate(&*scope.read().await).is_err());
    }

    #[tokio::test]
    async fn starting_adds_only_its_defaults_and_command_saves_its_section() {
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
        let mut component = Heartbeat.start(context).await.unwrap();
        assert!(store.has_section("Heartbeat"));
        assert!(store.has_section("Other"));
        assert!(matches!(
            persist_rx.try_recv(),
            Ok(crate::persist::PersistMessage::Changed)
        ));

        component.handle_command("interval 3").await.unwrap();
        assert_eq!(store.get_integer("Heartbeat", "interval_secs").unwrap(), 3);
        assert!(matches!(
            persist_rx.try_recv(),
            Ok(crate::persist::PersistMessage::Changed)
        ));

        let _ = shutdown_tx.send(true);
        component.quiesce().await.unwrap();
    }

    #[tokio::test]
    async fn commands_apply_to_the_running_worker_immediately() {
        let store = ConfigStore::from_str("[Heartbeat]\ninterval_secs = 3600\n").unwrap();
        let (persist_tx, _persist_rx) = tokio::sync::mpsc::unbounded_channel();
        let (shutdown_tx, shutdown) = watch::channel(false);
        let (events, mut received) = broadcast::channel(16);
        let context = Context {
            store,
            config_path: std::path::PathBuf::from("agent.toml"),
            persistence: PersistenceHandle::new(persist_tx),
            events,
            shutdown,
        };
        let mut component = Heartbeat.start(context).await.unwrap();

        component.handle_command("interval 1").await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), received.recv())
            .await
            .expect("the new interval must replace the pending one-hour sleep")
            .unwrap();

        component.handle_command("pause").await.unwrap();
        while received.try_recv().is_ok() {}
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(matches!(
            received.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));

        let _ = shutdown_tx.send(true);
        component.quiesce().await.unwrap();
    }

    #[test]
    fn splits_command_arguments_without_losing_message_spaces() {
        assert_eq!(
            split_command("message a longer message"),
            ("message", Some("a longer message"))
        );
        assert_eq!(split_command("pause"), ("pause", None));
    }
}
