//! Configuration scopes of the application components.
//!
//! Each component owns one TOML section of the shared `ConfigStore`:
//! the HTTP server owns `[Http]`, the monitor worker owns `[Monitor]`.

use std::collections::HashSet;
use std::time::Duration;

use conferencier::{ConfigScope, ConfigTable};

/// Settings of the HTTP server, the `[Http]` section.
#[derive(ConfigScope)]
#[confer(section = "Http")]
pub struct HttpConfig {
    #[confer(default = "127.0.0.1".into())]
    pub address: String,
    #[confer(default = 8080)]
    pub port: u16,
}

/// Settings of the monitor worker, the `[Monitor]` section.
///
/// Unknown keys are rejected, so a misspelled setting such as `interval_sec`
/// fails loading instead of silently falling back to the default.
#[derive(ConfigScope)]
#[confer(section = "Monitor", deny_unknown_keys)]
pub struct MonitorConfig {
    #[confer(default = true)]
    pub enabled: bool,
    #[confer(default = 30)]
    pub interval_secs: u64,
    #[confer(default = 5)]
    pub timeout_secs: u64,
    /// The `[[Monitor.targets]]` array of tables; required, but may be empty.
    #[confer(table)]
    pub targets: Vec<Target>,
}

/// One monitored service, an element of `[[Monitor.targets]]`.
#[derive(Clone, ConfigTable)]
#[confer(deny_unknown_keys)]
pub struct Target {
    pub name: String,
    /// A `host:port` address checked by opening a TCP connection.
    pub address: String,
}

impl MonitorConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs)
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

/// Checks application rules that the types alone do not express.
///
/// Conferencier converts values to the field types but deliberately does not
/// validate application-specific rules, so the application checks them itself
/// before using a loaded configuration.
pub fn validate(monitor: &MonitorConfig) -> Result<(), String> {
    if monitor.interval_secs == 0 {
        return Err("Monitor.interval_secs must be greater than zero".into());
    }
    if monitor.timeout_secs == 0 {
        return Err("Monitor.timeout_secs must be greater than zero".into());
    }
    let mut names = HashSet::new();
    for (index, target) in monitor.targets.iter().enumerate() {
        if target.name.trim().is_empty() {
            return Err(format!("Monitor.targets[{index}].name must not be empty"));
        }
        if target.address.trim().is_empty() {
            return Err(format!(
                "Monitor.targets[{index}].address must not be empty"
            ));
        }
        if !names.insert(target.name.as_str()) {
            return Err(format!("duplicate target name {:?}", target.name));
        }
    }
    Ok(())
}
