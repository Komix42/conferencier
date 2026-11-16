//! The background worker that checks the configured targets.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use conferencier::SharedScope;
use tokio::net::TcpStream;
use tokio::sync::{Notify, watch};
use tokio::task::JoinSet;

use crate::config::{MonitorConfig, Target};

/// The latest result of checking one target. Runtime state, never saved.
#[derive(Clone)]
pub struct CheckResult {
    /// The checked address; a result for an old address is stale after a reload.
    pub address: String,
    pub status: Status,
    pub checked_at: Instant,
}

#[derive(Clone)]
pub enum Status {
    Up { latency: Duration },
    Down { error: String },
}

impl Status {
    fn is_up(&self) -> bool {
        matches!(self, Status::Up { .. })
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Up { latency } => write!(f, "up ({:.1} ms)", latency.as_secs_f64() * 1000.0),
            Status::Down { error } => write!(f, "down: {error}"),
        }
    }
}

/// Checks the targets in rounds until `shutdown` changes.
///
/// Each round reads the current settings, so a reload or `pause` takes effect
/// with the next round; `wake` starts it immediately instead of after the interval.
pub async fn run(
    monitor: SharedScope<MonitorConfig>,
    wake: Arc<Notify>,
    mut shutdown: watch::Receiver<()>,
) {
    loop {
        // Copy the settings and release the lock before any network I/O, so a slow
        // check never blocks the HTTP server or a reload.
        let (enabled, interval, timeout, targets) = {
            let config = monitor.read().await;
            (
                config.enabled,
                config.interval(),
                config.timeout(),
                config.targets.clone(),
            )
        };

        if enabled {
            tokio::select! {
                results = check_all(targets, timeout) => record(&monitor, results).await,
                _ = shutdown.changed() => return,
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = wake.notified() => {}
            _ = shutdown.changed() => return,
        }
    }
}

async fn check_all(targets: Vec<Target>, timeout: Duration) -> Vec<(String, CheckResult)> {
    let mut checks = JoinSet::new();
    for target in targets {
        checks.spawn(async move { (target.name, check(target.address, timeout).await) });
    }
    let mut results = Vec::new();
    while let Some(joined) = checks.join_next().await {
        if let Ok(result) = joined {
            results.push(result);
        }
    }
    results
}

/// A target is up when a TCP connection to its address opens within `timeout`.
async fn check(address: String, timeout: Duration) -> CheckResult {
    let started = Instant::now();
    let status = match tokio::time::timeout(timeout, TcpStream::connect(address.as_str())).await {
        Ok(Ok(_stream)) => Status::Up {
            latency: started.elapsed(),
        },
        Ok(Err(error)) => Status::Down {
            error: error.to_string(),
        },
        Err(_) => Status::Down {
            error: format!("no connection within {timeout:?}"),
        },
    };
    CheckResult {
        address,
        status,
        checked_at: Instant::now(),
    }
}

/// Stores the results in the scope's runtime state and reports status changes.
async fn record(monitor: &SharedScope<MonitorConfig>, results: Vec<(String, CheckResult)>) {
    let mut guard = monitor.write().await;
    let config = &mut *guard;
    for (name, result) in results {
        // A reload during the checks may have removed the target or changed its address.
        let configured = config
            .targets
            .iter()
            .any(|target| target.name == name && target.address == result.address);
        if !configured {
            continue;
        }
        let is_up = result.status.is_up();
        let previous = config.results.insert(name.clone(), result);
        if previous.is_none_or(|previous| previous.status.is_up() != is_up) {
            println!("{name} is {}", if is_up { "up" } else { "down" });
        }
    }
    let names: HashSet<&str> = config.targets.iter().map(|t| t.name.as_str()).collect();
    config
        .results
        .retain(|name, _| names.contains(name.as_str()));
}
