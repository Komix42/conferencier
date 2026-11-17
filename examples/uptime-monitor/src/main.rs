//! Uptime monitor: an example application configured with conferencier.
//!
//! A background worker checks the configured TCP targets and an HTTP server
//! shows the results. Console commands reload, pause, resume and export the
//! configuration. See `README.md` for usage and architecture.

mod config;
mod http;
mod monitor;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use conferencier::{ConfigScope, ConfigStore, SharedScope};
use tokio::net::TcpListener;
use tokio::sync::{Notify, mpsc, watch};

use crate::config::{HttpConfig, MonitorConfig};

type AppResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// What the console commands work with.
struct App {
    store: ConfigStore,
    http: SharedScope<HttpConfig>,
    monitor: SharedScope<MonitorConfig>,
    /// Wakes the worker to recheck its settings without waiting for the interval.
    wake: Arc<Notify>,
    /// The address and port requested when binding.
    listener_config: (String, u16),
    /// The address actually bound, including any port chosen by the OS.
    listening: SocketAddr,
}

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
    let path = config_path();
    let store = ConfigStore::from_file(&path).await?;
    let http = HttpConfig::from_store(&store)?;
    let monitor = MonitorConfig::from_store(&store)?;
    config::validate(&*monitor.read().await)?;
    println!("Loaded configuration from {}", path.display());

    let listener_config = {
        let http = http.read().await;
        (http.address.clone(), http.port)
    };
    let listener = TcpListener::bind((listener_config.0.as_str(), listener_config.1))
        .await
        .map_err(|error| {
            format!(
                "cannot listen on {}:{}: {error}",
                listener_config.0, listener_config.1
            )
        })?;
    let listening = listener.local_addr()?;
    let url = format!("http://{listening}/");
    println!("Status at {url} and the configuration in use at {url}config");

    let (shutdown, shutdown_signal) = watch::channel(());
    let wake = Arc::new(Notify::new());
    let worker = tokio::spawn(monitor::run(
        monitor.clone(),
        wake.clone(),
        shutdown_signal.clone(),
    ));
    let mut server = tokio::spawn(http::serve(
        listener,
        http.clone(),
        monitor.clone(),
        shutdown_signal,
    ));

    let app = App {
        store,
        http,
        monitor,
        wake,
        listener_config,
        listening,
    };
    print_help();
    let mut console = spawn_console();
    let outcome = loop {
        tokio::select! {
            line = console.recv() => {
                // End of input quits, like the `quit` command.
                let Some(line) = line else { break Ok(()) };
                match handle(&app, line.trim()).await {
                    Ok(Continue::Yes) => {}
                    Ok(Continue::No) => break Ok(()),
                    Err(error) => eprintln!("error: {error}"),
                }
            }
            _ = tokio::signal::ctrl_c() => break Ok(()),
            result = &mut server => {
                let _ = shutdown.send(());
                let _ = worker.await;
                return match result {
                    Ok(Ok(())) => Err("the HTTP server stopped unexpectedly".into()),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(error.into()),
                };
            }
        }
    };

    println!("Shutting down");
    let _ = shutdown.send(());
    worker.await?;
    server.await??;
    outcome
}

enum Continue {
    Yes,
    No,
}

async fn handle(app: &App, command: &str) -> AppResult<Continue> {
    match command {
        "" => {}
        "reload" => reload(app).await?,
        "pause" => set_enabled(app, false).await,
        "resume" => set_enabled(app, true).await,
        "export" => export(app).await?,
        "help" => print_help(),
        "quit" => return Ok(Continue::No),
        other => println!("Unknown command {other:?}; type `help` for the list."),
    }
    Ok(Continue::Yes)
}

/// Reads the configuration file again and applies it to the running application.
async fn reload(app: &App) -> AppResult<()> {
    let path = config_path();
    // Read the file once, so the version that is checked is the one that is applied.
    let source = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    config::reload(&app.store, &app.http, &app.monitor, &source).await?;
    app.wake.notify_one();
    println!("Reloaded configuration from {}", path.display());

    let http = app.http.read().await;
    if let Some(notice) = listener_restart_notice(&http, &app.listener_config, app.listening) {
        println!("{notice}");
    }
    Ok(())
}

fn listener_restart_notice(
    http: &HttpConfig,
    listener_config: &(String, u16),
    listening: SocketAddr,
) -> Option<String> {
    if (http.address.as_str(), http.port) == (listener_config.0.as_str(), listener_config.1) {
        return None;
    }
    Some(format!(
        "The HTTP server keeps listening on {listening}; restart the application to use {}:{}.",
        http.address, http.port
    ))
}

/// Changes a setting at runtime. The change lives in the scope only: `export`
/// saves it, while `reload` replaces it with the value from the file.
async fn set_enabled(app: &App, enabled: bool) {
    app.monitor.write().await.enabled = enabled;
    app.wake.notify_one();
    println!("Monitoring {}", if enabled { "resumed" } else { "paused" });
}

/// Saves the scopes into the store and the store into `effective.toml`.
async fn export(app: &App) -> AppResult<()> {
    HttpConfig::save(&app.http, &app.store).await?;
    MonitorConfig::save(&app.monitor, &app.store).await?;
    let path = effective_config_path();
    app.store.save_file(&path).await?;
    println!(
        "Exported the configuration in use to {}; it is loaded instead of monitor.toml from now on.",
        path.display()
    );
    Ok(())
}

fn print_help() {
    println!("Commands: reload, pause, resume, export, help, quit (or Ctrl+C)");
}

/// Reads stdin on a dedicated thread: unlike Tokio's blocking stdin worker,
/// it does not hold up runtime shutdown.
fn spawn_console() -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel(8);
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if sender.blocking_send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

/// The exported configuration if it exists, otherwise the bundled configuration.
fn config_path() -> PathBuf {
    let effective = effective_config_path();
    if effective.exists() {
        effective
    } else {
        config_dir().join("monitor.toml")
    }
}

fn effective_config_path() -> PathBuf {
    config_dir().join("effective.toml")
}

/// The configuration directory, independent of the working directory.
fn config_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn listener_notice_uses_bound_address_and_compares_requested_settings() {
        let listener_config = ("127.0.0.1".to_owned(), 0);
        let listener = TcpListener::bind((listener_config.0.as_str(), listener_config.1))
            .await
            .unwrap();
        let listening = listener.local_addr().unwrap();
        assert_ne!(listening.port(), 0);
        let store = ConfigStore::from_str("[Http]\nport = 0\n").unwrap();
        let http = HttpConfig::from_store(&store).unwrap();
        let mut http = http.write().await;

        assert!(listener_restart_notice(&http, &listener_config, listening).is_none());
        http.port = listening.port();
        assert_eq!(
            listener_restart_notice(&http, &listener_config, listening).unwrap(),
            format!(
                "The HTTP server keeps listening on {listening}; restart the application to use 127.0.0.1:{}.",
                listening.port()
            )
        );
        http.port = 0;
        http.address = "localhost".into();
        assert_eq!(
            listener_restart_notice(&http, &listener_config, listening).unwrap(),
            format!(
                "The HTTP server keeps listening on {listening}; restart the application to use localhost:0."
            )
        );
        http.address = "127.0.0.1".into();
        assert!(listener_restart_notice(&http, &listener_config, listening).is_none());
    }
}
