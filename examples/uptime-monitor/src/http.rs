//! The HTTP interface that shows the monitor's state.

use std::fmt::Write;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use conferencier::{ConfigScope, ConfigStore, SharedScope};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::{HttpConfig, MonitorConfig};

#[derive(Clone)]
struct AppState {
    http: SharedScope<HttpConfig>,
    monitor: SharedScope<MonitorConfig>,
}

/// Serves `GET /` and `GET /config` until `shutdown` changes.
pub async fn serve(
    listener: TcpListener,
    http: SharedScope<HttpConfig>,
    monitor: SharedScope<MonitorConfig>,
    mut shutdown: watch::Receiver<()>,
) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(status))
        .route("/config", get(config))
        .with_state(AppState { http, monitor });
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown.changed().await;
        })
        .await
}

/// Lists the targets with their latest check results.
async fn status(State(state): State<AppState>) -> String {
    let served = count_request(&state.http).await;
    let monitor = state.monitor.read().await;

    let mut page = String::new();
    if monitor.enabled {
        writeln!(page, "Checking every {:?}.", monitor.interval()).unwrap();
    } else {
        writeln!(page, "Monitoring is paused.").unwrap();
    }
    writeln!(page).unwrap();
    if monitor.targets.is_empty() {
        writeln!(page, "No targets are configured.").unwrap();
    }
    for target in &monitor.targets {
        let state = match monitor.results.get(&target.name) {
            Some(result) if result.address == target.address => format!(
                "{} (checked {}s ago)",
                result.status,
                result.checked_at.elapsed().as_secs()
            ),
            _ => "not checked yet".into(),
        };
        writeln!(page, "{:<20} {:<24} {state}", target.name, target.address).unwrap();
    }
    writeln!(page, "\n{served} requests served.").unwrap();
    page
}

/// Shows the configuration the scopes currently hold, as TOML.
async fn config(State(state): State<AppState>) -> Result<String, (StatusCode, String)> {
    let served = count_request(&state.http).await;
    // Saving the scopes into a fresh store renders them without touching the
    // application's store. Ignored fields, such as the request counter, are not saved.
    let snapshot = ConfigStore::new();
    let rendered = async {
        HttpConfig::save(&state.http, &snapshot).await?;
        MonitorConfig::save(&state.monitor, &snapshot).await?;
        snapshot.save_str()
    }
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(format!(
        "# The configuration currently in use. Runtime state, such as the\n\
         # {served} requests served or the check results, is not part of it.\n\n{rendered}"
    ))
}

/// Counts a request in the runtime state of the HTTP scope, kept across reloads.
async fn count_request(http: &SharedScope<HttpConfig>) -> u64 {
    let mut config = http.write().await;
    config.requests_served += 1;
    config.requests_served
}
