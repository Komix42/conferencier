//! Configuration scopes of the application components.
//!
//! Each component owns one TOML section of the shared `ConfigStore`:
//! the HTTP server owns `[Http]`, the monitor worker owns `[Monitor]`.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use conferencier::{ConfigScope, ConfigStore, ConfigTable, SharedScope};

use crate::monitor::CheckResult;

/// Settings of the HTTP server, the `[Http]` section.
#[derive(ConfigScope)]
#[confer(section = "Http")]
pub struct HttpConfig {
    #[confer(default = "127.0.0.1".into())]
    pub address: String,
    #[confer(default = 8080)]
    pub port: u16,
    /// Runtime state shared by the request handlers; never loaded or saved,
    /// and kept when the configuration is reloaded.
    #[confer(ignore = 0)]
    pub requests_served: u64,
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
    /// The latest check result of each target by name. Runtime state written by
    /// the worker and read by the HTTP server.
    #[confer(ignore = HashMap::new())]
    pub results: HashMap<String, CheckResult>,
    /// Runtime token compared by `Arc` identity; reload replaces it.
    #[confer(ignore = Arc::new(()))]
    pub generation: Arc<()>,
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

/// Replaces the configuration with `source`, but only if all of it is valid.
///
/// The source is first loaded into a separate store and checked there, so on
/// any error the application's store and scopes keep their previous values.
/// The request counter and results for unchanged targets are kept. Once the
/// generation changes, results from earlier checks are discarded.
pub async fn reload(
    store: &ConfigStore,
    http: &SharedScope<HttpConfig>,
    monitor: &SharedScope<MonitorConfig>,
    source: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let candidate = ConfigStore::from_str(source)?;
    HttpConfig::from_store(&candidate)?;
    let candidate_monitor = MonitorConfig::from_store(&candidate)?;
    validate(&*candidate_monitor.read().await)?;

    // The same source has just converted successfully, so these do not fail.
    store.load_str(source)?;
    HttpConfig::load(http, store).await?;
    MonitorConfig::load(monitor, store).await?;
    // `load` leaves ignored fields untouched; replace the generation and
    // remove results for targets that were removed or changed.
    let mut monitor = monitor.write().await;
    let MonitorConfig {
        targets,
        results,
        generation,
        ..
    } = &mut *monitor;
    *generation = Arc::new(());
    results.retain(|name, result| {
        targets
            .iter()
            .any(|target| target.name == *name && target.address == result.address)
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use conferencier::Error;

    use super::*;
    use crate::monitor::Status;

    const BUNDLED: &str = include_str!("../config/monitor.toml");

    fn monitor_from(source: &str) -> conferencier::Result<SharedScope<MonitorConfig>> {
        MonitorConfig::from_store(&ConfigStore::from_str(source)?)
    }

    async fn validate_source(source: &str) -> Result<(), String> {
        validate(&*monitor_from(source).unwrap().read().await)
    }

    #[tokio::test]
    async fn bundled_configuration_loads_and_validates() {
        let store = ConfigStore::from_str(BUNDLED).unwrap();
        let http = HttpConfig::from_store(&store).unwrap();
        let monitor = MonitorConfig::from_store(&store).unwrap();
        assert_eq!(http.read().await.port, 8080);
        let monitor = monitor.read().await;
        validate(&monitor).unwrap();
        assert!(!monitor.targets.is_empty());
    }

    #[tokio::test]
    async fn validate_rejects_application_rule_violations() {
        let target = "[[Monitor.targets]]\nname = \"a\"\naddress = \"127.0.0.1:1\"\n";

        let zero_interval = format!("[Monitor]\ninterval_secs = 0\n{target}");
        let error = validate_source(&zero_interval).await.unwrap_err();
        assert!(error.contains("interval_secs"), "{error}");

        let duplicate = format!("[Monitor]\n{target}{target}");
        let error = validate_source(&duplicate).await.unwrap_err();
        assert!(error.contains("duplicate target name"), "{error}");

        let empty_name = "[Monitor]\n[[Monitor.targets]]\nname = \" \"\naddress = \"x:1\"\n";
        let error = validate_source(empty_name).await.unwrap_err();
        assert!(error.contains("targets[0].name"), "{error}");
    }

    #[test]
    fn misspelled_target_key_is_rejected() {
        let source = "[Monitor]\n[[Monitor.targets]]\nname = \"a\"\nadress = \"x:1\"\n";
        match monitor_from(source) {
            Err(Error::UnknownKey { key, .. }) => assert_eq!(key, "adress"),
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("a misspelled key was accepted"),
        }
    }

    #[tokio::test]
    async fn reload_applies_valid_source_and_keeps_runtime_state() {
        let store = ConfigStore::from_str(BUNDLED).unwrap();
        let http = HttpConfig::from_store(&store).unwrap();
        let monitor = MonitorConfig::from_store(&store).unwrap();
        http.write().await.requests_served = 7;

        let source = "[Http]\nport = 9090\n[Monitor]\ninterval_secs = 3\ntargets = []\n";
        reload(&store, &http, &monitor, source).await.unwrap();

        let http = http.read().await;
        assert_eq!((http.port, http.requests_served), (9090, 7));
        let monitor = monitor.read().await;
        assert_eq!(monitor.interval_secs, 3);
        assert!(monitor.targets.is_empty());
    }

    #[tokio::test]
    async fn reload_keeps_unchanged_results_and_removes_changed_or_absent_targets() {
        let initial = "[Monitor]\nenabled = false\n\
            [[Monitor.targets]]\nname = 'keep'\naddress = 'keep:1'\n\
            [[Monitor.targets]]\nname = 'changed'\naddress = 'old:1'\n\
            [[Monitor.targets]]\nname = 'removed'\naddress = 'removed:1'\n";
        let store = ConfigStore::from_str(initial).unwrap();
        let http = HttpConfig::from_store(&store).unwrap();
        let monitor = MonitorConfig::from_store(&store).unwrap();
        let checked_at = std::time::Instant::now();
        {
            let mut monitor = monitor.write().await;
            for (name, address) in [
                ("keep", "keep:1"),
                ("changed", "old:1"),
                ("removed", "removed:1"),
            ] {
                monitor.results.insert(
                    name.into(),
                    CheckResult {
                        address: address.into(),
                        status: Status::Up {
                            latency: Duration::from_millis(1),
                        },
                        checked_at,
                    },
                );
            }
        }

        let changed = "[Monitor]\nenabled = false\n\
            [[Monitor.targets]]\nname = 'keep'\naddress = 'keep:1'\n\
            [[Monitor.targets]]\nname = 'changed'\naddress = 'new:1'\n";
        reload(&store, &http, &monitor, changed).await.unwrap();
        {
            let monitor = monitor.read().await;
            assert_eq!(monitor.results.len(), 1);
            assert_eq!(monitor.results["keep"].checked_at, checked_at);
            assert!(!monitor.results.contains_key("changed"));
            assert!(!monitor.results.contains_key("removed"));
        }

        reload(&store, &http, &monitor, initial).await.unwrap();
        let monitor = monitor.read().await;
        assert_eq!(monitor.results.len(), 1);
        assert_eq!(monitor.results["keep"].checked_at, checked_at);
        assert!(!monitor.results.contains_key("changed"));
        assert!(!monitor.results.contains_key("removed"));
    }

    #[tokio::test]
    async fn failed_reload_leaves_store_and_scopes_unchanged() {
        let store = ConfigStore::from_str(BUNDLED).unwrap();
        let http = HttpConfig::from_store(&store).unwrap();
        let monitor = MonitorConfig::from_store(&store).unwrap();
        let before = store.save_str().unwrap();
        let generation = monitor.read().await.generation.clone();

        let invalid = [
            "[Monitor\n",                                          // TOML syntax
            "[Http]\nport = 70000\n[Monitor]\ntargets = []\n",     // type range
            "[Monitor]\nintervl = 3\ntargets = []\n",              // unknown key
            "[Monitor]\ninterval_secs = 0\ntargets = []\n",        // application rule
            "[Http]\nport = 9090\n[Monitor]\ninterval_secs = 3\n", // missing targets
        ];
        for source in invalid {
            assert!(
                reload(&store, &http, &monitor, source).await.is_err(),
                "{source}"
            );
            assert_eq!(store.save_str().unwrap(), before, "{source}");
            assert_eq!(http.read().await.port, 8080, "{source}");
            let monitor = monitor.read().await;
            assert_eq!(monitor.interval_secs, 10, "{source}");
            assert_eq!(monitor.targets.len(), 2, "{source}");
            assert!(Arc::ptr_eq(&monitor.generation, &generation), "{source}");
        }
    }
}
