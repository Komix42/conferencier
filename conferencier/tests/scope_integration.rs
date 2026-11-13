use std::sync::atomic::{AtomicUsize, Ordering};

use conferencier::{ConfigScope, ConfigStore, Datetime, Error, Result, SharedScope};

#[derive(ConfigScope)]
#[confer(section = "Srv")]
struct Server {
    #[confer(rename = "p")]
    port: u16,
    #[confer(default = "0.0.0.0".into())]
    host: String,
    #[confer(default = 3)]
    retries: i32,
    #[confer(default = vec!["alpha".into(), "beta".into()])]
    features: Vec<String>,
    #[confer(default = "2024-01-01T00:00:00Z".parse().unwrap())]
    started_at: Datetime,
    endpoint: Option<String>,
    notes: Option<String>,
    #[confer(ignore = Vec::new())]
    cache: Vec<u8>,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "App.server")]
struct DottedSection {
    port: u16,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Ports")]
struct Ports {
    ports: Vec<u8>,
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct WidePorts {
    ports: Vec<u64>,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Ratios")]
struct Ratios {
    ratios: Vec<f32>,
}

#[derive(Debug, ConfigScope)]
#[confer(deny_unknown_keys)]
struct StrictScope {
    #[confer(rename = "runtime")]
    name: String,
    #[confer(ignore)]
    runtime: String,
}

#[derive(Debug, ConfigScope)]
#[confer(deny_unknown_keys)]
struct StrictEmpty {
    #[confer(ignore)]
    runtime: String,
}

#[derive(Debug, ConfigScope)]
#[confer(deny_unknown_keys)]
struct StrictDefaultScope {
    #[confer(default = "fallback".into())]
    name: String,
}

#[tokio::test]
async fn module_from_store_loads_defaults_and_values() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[Srv]
extra = 7
p = 8080
notes = "temp"
endpoint = "example.com"
"#,
    )?;

    let module = Server::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.port, 8080);
        assert_eq!(guard.host, "0.0.0.0");
        assert_eq!(guard.retries, 3);
        assert_eq!(
            guard.features,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert!(
            guard
                .started_at
                .to_string()
                .starts_with("2024-01-01T00:00:00Z")
        );
        assert_eq!(guard.endpoint.as_deref(), Some("example.com"));
        assert_eq!(guard.notes.as_deref(), Some("temp"));
        assert!(guard.cache.is_empty());
    }

    {
        let mut guard = module.write().await;
        guard.host = "127.0.0.1".into();
        guard.endpoint = None;
        guard.notes = None;
        guard.features = vec!["gamma".into()];
    }

    Server::save(&module, &store).await?;

    assert_eq!(store.get_string("Srv", "host")?, "127.0.0.1");
    assert_eq!(store.get_integer("Srv", "p")?, 8080);
    assert_eq!(store.get_integer("Srv", "retries")?, 3);
    assert_eq!(
        store.get_string_vec("Srv", "features")?,
        vec!["gamma".to_string()]
    );
    assert!(store.get_value("Srv", "endpoint").is_none());
    assert!(store.get_value("Srv", "notes").is_none());
    assert!(store.get_value("Srv", "extra").is_none());
    Ok(())
}

async fn reload_in_task<D: ConfigScope>(scope: SharedScope<D>, store: ConfigStore) -> Result<()> {
    tokio::spawn(async move { D::load(&scope, &store).await })
        .await
        .expect("reload task panicked")
}

#[tokio::test]
async fn generic_code_can_spawn_load() -> Result<()> {
    let store = ConfigStore::from_str("[Srv]\np = 8080")?;
    let module = Server::from_store(&store)?;
    store.set_integer("Srv", "p", 9090)?;

    reload_in_task(module.clone(), store.clone()).await?;
    assert_eq!(module.read().await.port, 9090);
    Ok(())
}

#[test]
fn store_and_from_store_work_without_a_runtime() -> Result<()> {
    let store: ConfigStore = "[Srv]\np = 8080".parse()?;
    store.set_string("Srv", "host", String::from("localhost"))?;

    let module = Server::from_store(&store)?;
    let guard = module.try_read().expect("no other guard exists");
    assert_eq!(guard.port, 8080);
    assert_eq!(guard.host, "localhost");
    Ok(())
}

#[test]
fn derived_scope_diagnostics_format_the_literal_top_level_section() -> Result<()> {
    let error = DottedSection::from_store(&ConfigStore::new()).unwrap_err();
    assert!(matches!(
        error,
        Error::MissingKey { ref path, ref key }
            if path == "\"App.server\"" && key == "port"
    ));
    assert_eq!(error.to_string(), "missing key \"App.server\".port");
    let store = ConfigStore::from_str("[\"App.server\"]\nport = 80")?;
    assert_eq!(
        DottedSection::from_store(&store)?
            .try_read()
            .expect("no other guard exists")
            .port,
        80
    );
    Ok(())
}

#[tokio::test]
async fn scalar_array_range_errors_include_the_element_index() -> Result<()> {
    let store = ConfigStore::from_str("[Ports]\nports = [0, 256]")?;
    let error = Ports::from_store(&store).unwrap_err();
    assert!(matches!(
        error,
        Error::ValueParse { ref path, ref key, ref message }
            if path == "Ports"
                && key == "ports"
                && message == "value out of range for u8 (at index 1)"
    ));
    assert_eq!(
        error.to_string(),
        "invalid value at Ports.ports: value out of range for u8 (at index 1)"
    );
    assert_eq!(
        Ports::from_store(&ConfigStore::from_str("[Ports]\nports = [1]")?)?
            .try_read()
            .expect("no other guard exists")
            .ports,
        vec![1]
    );

    let store = ConfigStore::from_str("[App]\nports = [0]")?;
    let scope = WidePorts::from_store(&store)?;
    scope.write().await.ports = vec![0, u64::MAX];
    let before = store.save_str()?;
    let error = WidePorts::save(&scope, &store).await.unwrap_err();
    let expected_message = format!(
        "value `{}` out of range for TOML integer (at index 1)",
        u64::MAX
    );
    assert!(matches!(
        error,
        Error::ValueParse { ref path, ref key, message }
            if path == "App"
                && key == "ports"
                && message == expected_message
    ));
    assert_eq!(store.save_str()?, before);

    let store = ConfigStore::from_str("[Ratios]\nratios = [1.0, 1e100]")?;
    let error = Ratios::from_store(&store).unwrap_err();
    assert!(matches!(
        error,
        Error::ValueParse { ref path, ref key, ref message }
            if path == "Ratios"
                && key == "ratios"
                && message == "value out of range for f32 (at index 1)"
    ));
    assert_eq!(
        Ratios::from_store(&ConfigStore::from_str("[Ratios]\nratios = [1.0]")?)?
            .try_read()
            .expect("no other guard exists")
            .ratios,
        vec![1.0]
    );
    Ok(())
}

#[tokio::test]
async fn strict_scope_checks_known_keys_before_conversion_and_keeps_failed_load_atomic()
-> Result<()> {
    let error = StrictScope::from_store(&ConfigStore::from_str("StrictScope = 7")?).unwrap_err();
    assert!(matches!(
        error,
        Error::TypeMismatch { ref path, ref key, expected: "table", found: "integer" }
            if path.is_empty() && key == "StrictScope"
    ));

    let store = ConfigStore::from_str("[StrictScope]\nzeta = 1\nalpha = 2")?;
    let error = StrictScope::from_store(&store).unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "StrictScope" && key == "alpha"
    ));
    assert_eq!(error.to_string(), "unknown key StrictScope.alpha");

    let store = ConfigStore::from_str("[StrictScope]\nruntime = \"before\"")?;
    let scope = StrictScope::from_store(&store)?;
    assert_eq!(scope.read().await.runtime, String::new());
    scope.write().await.name = "unchanged".into();
    store.set_integer("StrictScope", "extra", 1)?;
    let error = StrictScope::load(&scope, &store).await.unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "StrictScope" && key == "extra"
    ));
    assert_eq!(scope.read().await.name, "unchanged");
    StrictScope::save(&scope, &store).await?;
    assert_eq!(store.get_string("StrictScope", "runtime")?, "unchanged");
    assert!(store.get_value("StrictScope", "extra").is_none());

    let error = StrictScope::from_store(&ConfigStore::from_str(
        "[StrictScope]\nruntime = \"valid\"\nname = \"Rust name is not a TOML key\"",
    )?)
    .unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref key, .. } if key == "name"
    ));

    let error = StrictEmpty::from_store(&ConfigStore::from_str("[StrictEmpty]\nextra = true")?)
        .unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "StrictEmpty" && key == "extra"
    ));
    assert!(
        StrictEmpty::from_store(&ConfigStore::new())?
            .try_read()
            .expect("no other guard exists")
            .runtime
            .is_empty()
    );
    assert_eq!(
        StrictDefaultScope::from_store(&ConfigStore::from_str(
            "[StrictDefualtScope]\nextra = true"
        )?)?
        .try_read()
        .expect("no other guard exists")
        .name,
        "fallback"
    );
    Ok(())
}

#[derive(ConfigScope)]
#[confer(section = "Nested")]
struct NestedModule {
    counters: Option<Vec<u16>>,
    thresholds: Option<Vec<u16>>,
    #[confer(rename = "aliases")]
    aliases: Option<Vec<String>>,
}

#[tokio::test]
async fn option_vec_fields_load_modify_and_save() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[Nested]
counters = [10, 20]
"#,
    )?;

    let module = NestedModule::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.counters.as_deref(), Some(&[10, 20][..]));
        assert_eq!(guard.thresholds, None);
        assert_eq!(guard.aliases, None);
    }

    {
        let mut guard = module.write().await;
        guard.counters = None;
        guard.thresholds = Some(vec![30, 40, 50]);
        guard.aliases = Some(vec!["blue".into(), "green".into()]);
    }

    NestedModule::save(&module, &store).await?;

    assert!(store.get_value("Nested", "counters").is_none());
    assert_eq!(
        store.get_integer_vec("Nested", "thresholds")?,
        vec![30, 40, 50]
    );
    assert_eq!(
        store.get_string_vec("Nested", "aliases")?,
        vec!["blue".to_string(), "green".to_string()]
    );

    let restored = NestedModule::from_store(&store)?;
    let guard = restored.read().await;
    assert_eq!(guard.counters, None);
    assert_eq!(guard.thresholds.as_deref(), Some(&[30, 40, 50][..]));
    assert_eq!(
        guard.aliases.as_deref(),
        Some(&["blue".to_string(), "green".to_string()][..])
    );

    Ok(())
}

/// Field names matching identifiers used inside the generated code.
#[derive(ConfigScope)]
#[confer(section = "Hygiene")]
struct Hygiene {
    store: String,
    module: String,
    scope: String,
    guard: i64,
    value: Option<String>,
    key: Vec<String>,
}

#[tokio::test]
async fn field_names_do_not_collide_with_generated_locals() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[Hygiene]
store = "s"
module = "m"
scope = "sc"
guard = 1
value = "v"
key = ["k"]
"#,
    )?;

    let module = Hygiene::from_store(&store)?;

    {
        let mut guard = module.write().await;
        guard.store = "s2".into();
        guard.scope = "sc2".into();
        guard.guard = 2;
        guard.value = None;
    }

    Hygiene::save(&module, &store).await?;

    assert_eq!(store.get_string("Hygiene", "store")?, "s2");
    assert_eq!(store.get_string("Hygiene", "module")?, "m");
    assert_eq!(store.get_string("Hygiene", "scope")?, "sc2");
    assert_eq!(store.get_integer("Hygiene", "guard")?, 2);
    assert!(store.get_value("Hygiene", "value").is_none());
    assert_eq!(
        store.get_string_vec("Hygiene", "key")?,
        vec!["k".to_string()]
    );

    Ok(())
}

/// Raw identifiers map to TOML keys without the `r#` prefix.
#[derive(ConfigScope)]
#[confer(section = "Raw")]
struct RawIdents {
    r#type: String,
    #[confer(default = 3)]
    r#loop: u32,
}

#[tokio::test]
async fn raw_identifier_fields_use_unprefixed_keys() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[Raw]
type = "t"
"#,
    )?;

    let module = RawIdents::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.r#type, "t");
        assert_eq!(guard.r#loop, 3);
    }

    RawIdents::save(&module, &store).await?;

    assert_eq!(store.get_string("Raw", "type")?, "t");
    assert_eq!(store.get_integer("Raw", "loop")?, 3);
    assert!(store.get_value("Raw", "r#type").is_none());

    Ok(())
}

/// Negative numeric literals are accepted as defaults.
#[derive(ConfigScope)]
#[confer(section = "Negative")]
struct NegativeDefaults {
    #[confer(default = -1)]
    offset: i32,
    #[confer(default = -2.5)]
    ratio: f64,
    #[confer(default = -3.0)]
    bias: f32,
    #[confer(default = vec![-1, 2])]
    deltas: Vec<i64>,
    #[confer(default = -7)]
    floor: i64,
}

#[tokio::test]
async fn negative_numeric_defaults() -> Result<()> {
    let store = ConfigStore::from_str("[Negative]\n")?;

    let module = NegativeDefaults::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.offset, -1);
        assert_eq!(guard.ratio, -2.5);
        assert_eq!(guard.bias, -3.0);
        assert_eq!(guard.deltas, vec![-1, 2]);
        assert_eq!(guard.floor, -7);
    }

    NegativeDefaults::save(&module, &store).await?;

    assert_eq!(store.get_integer("Negative", "offset")?, -1);
    assert_eq!(store.get_float("Negative", "ratio")?, -2.5);
    assert_eq!(store.get_float("Negative", "bias")?, -3.0);
    assert_eq!(store.get_integer_vec("Negative", "deltas")?, vec![-1, 2]);
    assert_eq!(store.get_integer("Negative", "floor")?, -7);

    Ok(())
}

/// Without `section`, the section is the full struct name (RFC-0005 N7). The temporary
/// migration guard rejects only names that 0.0.4 shortened, i.e. `Confer` followed by an
/// uppercase letter; these names are not affected.
#[derive(ConfigScope)]
struct ConferenceSettings {
    room: String,
}

#[derive(ConfigScope)]
struct Confer {
    level: u8,
}

#[allow(non_camel_case_types)]
#[derive(ConfigScope)]
struct Confer_App {
    name: String,
}

#[derive(ConfigScope)]
struct r#AppSettings {
    port: u16,
}

#[tokio::test]
async fn default_section_name_is_the_full_struct_name() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[ConferenceSettings]
room = "A1"

[Confer]
level = 2

[Confer_App]
name = "demo"

[AppSettings]
port = 8080
"#,
    )?;

    assert_eq!(
        ConferenceSettings::from_store(&store)?.read().await.room,
        "A1"
    );
    assert_eq!(Confer::from_store(&store)?.read().await.level, 2);
    assert_eq!(Confer_App::from_store(&store)?.read().await.name, "demo");
    assert_eq!(AppSettings::from_store(&store)?.read().await.port, 8080);
    Ok(())
}

/// The two ways out of the N7 guard for a name such as `ConferApp`.
mod explicit_confer_sections {
    use conferencier::{ConfigScope, ConfigStore, Result};

    pub mod old {
        #[derive(conferencier::ConfigScope)]
        #[confer(section = "App")]
        pub struct ConferApp {
            pub port: u16,
        }
    }

    pub mod full {
        #[derive(conferencier::ConfigScope)]
        #[confer(section = "ConferApp")]
        pub struct ConferApp {
            pub port: u16,
        }
    }

    #[tokio::test]
    async fn explicit_sections_load_and_save_the_chosen_section() -> Result<()> {
        let store = ConfigStore::from_str("[App]\nport = 1\n[ConferApp]\nport = 2")?;
        let old = old::ConferApp::from_store(&store)?;
        let full = full::ConferApp::from_store(&store)?;
        assert_eq!(old.read().await.port, 1);
        assert_eq!(full.read().await.port, 2);

        old.write().await.port = 10;
        full.write().await.port = 20;
        old::ConferApp::save(&old, &store).await?;
        full::ConferApp::save(&full, &store).await?;
        assert_eq!(store.get_integer("App", "port")?, 10);
        assert_eq!(store.get_integer("ConferApp", "port")?, 20);
        Ok(())
    }
}

#[derive(ConfigScope)]
#[confer(section = "Schedule")]
struct ScheduleDefaults {
    #[confer(default = "07:30".parse().unwrap())]
    alarm: Datetime,
    #[confer(default = vec!["07:30".parse().unwrap(), "08:45:00.125".parse().unwrap()])]
    reminders: Vec<Datetime>,
}

#[tokio::test]
async fn datetime_defaults_preserve_omitted_seconds() -> Result<()> {
    let store = ConfigStore::new();
    let module = ScheduleDefaults::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.alarm.to_string(), "07:30");
        assert_eq!(guard.alarm.time.as_ref().unwrap().second, None);
        assert_eq!(
            guard
                .reminders
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["07:30", "08:45:00.125"]
        );
    }

    ScheduleDefaults::save(&module, &store).await?;
    let restored = ConfigStore::from_str(&store.save_str()?)?;
    let reloaded = ScheduleDefaults::from_store(&restored)?;
    let original = module.read().await;
    let reloaded = reloaded.read().await;
    assert_eq!(reloaded.alarm, original.alarm);
    assert_eq!(reloaded.reminders, original.reminders);
    Ok(())
}

mod default_expressions {
    use super::*;

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static ORDER: AtomicUsize = AtomicUsize::new(0);
    static BEFORE_PANIC: AtomicUsize = AtomicUsize::new(0);
    static PANICS: AtomicUsize = AtomicUsize::new(0);
    const PORT: u16 = 8080;

    fn computed() -> u16 {
        CALLS.fetch_add(1, Ordering::SeqCst) as u16 + 1
    }

    fn panicking_default() -> u16 {
        PANICS.fetch_add(1, Ordering::SeqCst);
        panic!("default failed")
    }

    fn ordered_default() -> usize {
        ORDER.fetch_add(1, Ordering::SeqCst)
    }

    fn before_panic() -> u16 {
        BEFORE_PANIC.fetch_add(1, Ordering::SeqCst);
        3
    }

    #[derive(Debug, ConfigScope)]
    #[confer(section = "Computed")]
    struct Computed {
        required: u16,
        #[confer(default = computed())]
        generated: u16,
        later: u16,
        #[confer(default = PORT)]
        port: u16,
        #[confer(default = "localhost".to_owned())]
        host: String,
        #[confer(default = vec!["a".into(), "b".into()])]
        names: Vec<String>,
        #[confer(default, rename = "empty-values")]
        empty: Vec<u16>,
        #[confer(default = { return 7; })]
        closure_return: u16,
        #[confer(default = 3 + 4)]
        arithmetic: u16,
    }

    #[tokio::test]
    async fn defaults_run_only_for_missing_keys_and_are_saved() -> Result<()> {
        CALLS.store(0, Ordering::SeqCst);
        let config_store = ConfigStore::from_str("[Computed]")?;
        assert!(matches!(
            Computed::from_store(&config_store).unwrap_err(),
            Error::MissingKey { key, .. } if key == "required"
        ));
        assert_eq!(CALLS.load(Ordering::SeqCst), 0);

        config_store.load_str("[Computed]\nrequired = 1\nlater = 2")?;
        let instance = Computed::from_store(&config_store)?;
        {
            let guard = instance.read().await;
            assert_eq!(guard.generated, 1);
            assert_eq!(guard.port, PORT);
            assert_eq!(guard.host, "localhost");
            assert_eq!(guard.names, ["a", "b"]);
            assert!(guard.empty.is_empty());
            assert_eq!(guard.closure_return, 7);
            assert_eq!(guard.arithmetic, 7);
        }
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);

        Computed::save(&instance, &config_store).await?;
        assert_eq!(config_store.get_integer("Computed", "generated")?, 1);
        Computed::load(&instance, &config_store).await?;
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);

        config_store.load_str("[Computed]\nrequired = 3\nlater = 4")?;
        Computed::load(&instance, &config_store).await?;
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(instance.read().await.generated, 2);

        config_store.load_str("[Computed]\nrequired = 5\ngenerated = \"bad\"\nlater = 6")?;
        assert!(Computed::load(&instance, &config_store).await.is_err());
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
        assert_eq!(instance.read().await.required, 3);

        config_store.load_str("[Computed]\nrequired = 6")?;
        assert!(Computed::load(&instance, &config_store).await.is_err());
        assert_eq!(CALLS.load(Ordering::SeqCst), 3);
        let guard = instance.read().await;
        assert_eq!((guard.required, guard.generated, guard.later), (3, 2, 4));
        Ok(())
    }

    #[derive(Debug, ConfigScope)]
    struct OrderedDefaults {
        #[confer(default = ordered_default())]
        first: usize,
        required: u16,
        #[confer(default = ordered_default())]
        last: usize,
    }

    #[tokio::test]
    async fn defaults_run_in_declaration_order_and_stop_at_an_error() -> Result<()> {
        ORDER.store(0, Ordering::SeqCst);
        assert!(matches!(
            OrderedDefaults::from_store(&ConfigStore::new()).unwrap_err(),
            Error::MissingKey { key, .. } if key == "required"
        ));
        assert_eq!(ORDER.load(Ordering::SeqCst), 1);

        let store = ConfigStore::from_str("[OrderedDefaults]\nrequired = 4")?;
        let scope = OrderedDefaults::from_store(&store)?;
        let guard = scope.read().await;
        assert_eq!((guard.first, guard.required, guard.last), (1, 4, 2));
        assert_eq!(ORDER.load(Ordering::SeqCst), 3);
        Ok(())
    }

    #[derive(Debug, ConfigScope)]
    #[confer(section = "Panicking")]
    struct Panicking {
        first: u16,
        #[confer(default = before_panic())]
        second: u16,
        #[confer(default = panicking_default())]
        third: u16,
    }

    #[tokio::test]
    async fn a_panicking_default_leaves_the_scope_unchanged() -> Result<()> {
        BEFORE_PANIC.store(0, Ordering::SeqCst);
        PANICS.store(0, Ordering::SeqCst);
        let config_store = ConfigStore::from_str("[Panicking]\nfirst = 1\nsecond = 2\nthird = 3")?;
        let instance = Panicking::from_store(&config_store)?;
        config_store.load_str("[Panicking]\nfirst = 9")?;

        let task_scope = instance.clone();
        let task_store = config_store.clone();
        let failure = tokio::spawn(async move { Panicking::load(&task_scope, &task_store).await })
            .await
            .unwrap_err();
        assert!(failure.is_panic());
        assert_eq!(BEFORE_PANIC.load(Ordering::SeqCst), 1);
        assert_eq!(PANICS.load(Ordering::SeqCst), 1);
        let guard = instance.read().await;
        assert_eq!((guard.first, guard.second, guard.third), (1, 2, 3));
        assert_eq!(config_store.get_integer("Panicking", "first")?, 9);
        Ok(())
    }

    #[derive(Debug, ConfigScope)]
    #[confer(section = "NonFinite")]
    struct NonFinite {
        #[confer(default = f32::NAN)]
        number: f32,
    }

    #[tokio::test]
    async fn non_finite_default_can_be_saved_but_not_reloaded() -> Result<()> {
        let config_store = ConfigStore::new();
        let instance = NonFinite::from_store(&config_store)?;
        assert!(instance.read().await.number.is_nan());
        NonFinite::save(&instance, &config_store).await?;
        let restored = ConfigStore::from_str(&config_store.save_str()?)?;
        assert!(matches!(
            NonFinite::from_store(&restored).unwrap_err(),
            Error::ValueParse { .. }
        ));
        Ok(())
    }
}

mod default_expression_hygiene {
    use super::*;
    use conferencier::ConfigTable;

    #[allow(non_upper_case_globals)]
    const store: u16 = 1;
    #[allow(non_upper_case_globals)]
    const scope: u16 = 2;
    #[allow(non_upper_case_globals)]
    const table: u16 = 3;
    #[allow(non_upper_case_globals)]
    const path: u16 = 4;
    #[allow(non_upper_case_globals)]
    const value: u16 = 5;
    #[allow(non_upper_case_globals)]
    const raw: u16 = 6;
    #[allow(non_upper_case_globals)]
    const index: u16 = 7;
    #[allow(non_upper_case_globals)]
    const array: u16 = 8;
    #[allow(non_upper_case_globals)]
    const array_path: u16 = 9;
    #[allow(non_upper_case_globals)]
    const out: u16 = 10;
    #[allow(non_upper_case_globals)]
    const v: u16 = 11;

    #[derive(ConfigScope)]
    struct Scope {
        #[confer(default = store)]
        a: u16,
        #[confer(default = scope)]
        b: u16,
        #[confer(default = table)]
        c: u16,
        #[confer(default = path)]
        d: u16,
        #[confer(default = value)]
        e: u16,
        #[confer(default = vec![raw, index, array, array_path, out, v])]
        others: Vec<u16>,
        #[confer(default = 12)]
        internal_store: u16,
        #[confer(default = 13)]
        internal_scope: u16,
    }

    #[derive(ConfigTable)]
    struct Table {
        #[confer(default = store + scope + table + path + value)]
        total: u16,
        #[confer(default = vec![raw, index, array, array_path, out, v])]
        others: Vec<u16>,
    }

    #[tokio::test]
    async fn user_constants_do_not_capture_generated_locals() -> Result<()> {
        let config_store = ConfigStore::new();
        let instance = Scope::from_store(&config_store)?;
        {
            let guard = instance.read().await;
            assert_eq!(
                (guard.a, guard.b, guard.c, guard.d, guard.e),
                (1, 2, 3, 4, 5)
            );
            assert_eq!(guard.others, [6, 7, 8, 9, 10, 11]);
            assert_eq!((guard.internal_store, guard.internal_scope), (12, 13));
        }
        Scope::load(&instance, &config_store).await?;
        Scope::save(&instance, &config_store).await?;

        let decoded = Table::from_table("Inner", conferencier::toml::Table::new())?;
        assert_eq!(decoded.total, 15);
        assert_eq!(decoded.others, [6, 7, 8, 9, 10, 11]);
        let encoded = decoded.to_table("Inner")?;
        let decoded = Table::from_table("Inner", encoded)?;
        assert_eq!(decoded.total, 15);
        assert_eq!(decoded.others, [6, 7, 8, 9, 10, 11]);
        Ok(())
    }
}

/// `Datetime` has no `Default` value; a required field of that type is built directly.
#[derive(ConfigScope)]
#[confer(section = "Clock")]
struct Clock {
    started_at: Datetime,
    stopped_at: Option<Datetime>,
    laps: Vec<Datetime>,
}

#[tokio::test]
async fn required_datetime_without_default_loads_and_saves() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[Clock]
started_at = 2024-05-01T10:30:00Z
laps = [2024-05-01T10:31:00Z]
"#,
    )?;

    let module = Clock::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.started_at.to_string(), "2024-05-01T10:30:00Z");
        assert!(guard.stopped_at.is_none());
        assert_eq!(guard.laps.len(), 1);
    }

    Clock::save(&module, &store).await?;
    let restored = ConfigStore::from_str(&store.save_str()?)?;
    let reloaded = Clock::from_store(&restored)?;
    assert_eq!(
        reloaded.read().await.started_at,
        module.read().await.started_at
    );

    Ok(())
}

#[tokio::test]
async fn required_datetime_without_default_reports_missing_key() -> Result<()> {
    let store = ConfigStore::from_str("[Clock]\n")?;

    let err = Clock::from_store(&store)
        .err()
        .expect("a missing required key must fail");

    assert!(
        matches!(&err, Error::MissingKey { path, key } if path == "Clock" && key == "started_at"),
        "unexpected error: {err}"
    );

    Ok(())
}

static TRACKED_DEFAULTS: AtomicUsize = AtomicUsize::new(0);

/// Counts how often its `Default` implementation runs.
struct Tracked(u32);

impl Default for Tracked {
    fn default() -> Self {
        TRACKED_DEFAULTS.fetch_add(1, Ordering::SeqCst);
        Tracked(7)
    }
}

#[derive(ConfigScope)]
#[confer(section = "Tracked")]
struct TrackedModule {
    port: u16,
    retries: u8,
    #[confer(ignore)]
    state: Tracked,
}

/// One test owns the counter, because tests in this file run in parallel.
#[tokio::test]
async fn ignored_fields_are_initialized_once_after_conversion_succeeds() -> Result<()> {
    // A required key is missing.
    let store = ConfigStore::from_str("[Tracked]\nretries = 1\n")?;
    let err = TrackedModule::from_store(&store)
        .err()
        .expect("a missing required key must fail");
    assert!(matches!(&err, Error::MissingKey { key, .. } if key == "port"));
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 0);

    // A later field is out of range, and the first error wins in declaration order.
    let store = ConfigStore::from_str("[Tracked]\nport = 80\nretries = 300\n")?;
    let err = TrackedModule::from_store(&store)
        .err()
        .expect("an out-of-range value must fail");
    assert!(matches!(&err, Error::ValueParse { key, .. } if key == "retries"));
    let store = ConfigStore::from_str("[Tracked]\nretries = 300\n")?;
    let err = TrackedModule::from_store(&store)
        .err()
        .expect("a missing required key must fail");
    assert!(matches!(&err, Error::MissingKey { key, .. } if key == "port"));
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 0);

    // Success initializes the ignored field exactly once; load and save leave it alone.
    let store = ConfigStore::from_str("[Tracked]\nport = 80\nretries = 3\n")?;
    let module = TrackedModule::from_store(&store)?;
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 1);
    assert_eq!(module.read().await.state.0, 7);

    module.write().await.state = Tracked(42);
    TrackedModule::load(&module, &store).await?;
    TrackedModule::save(&module, &store).await?;
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 1);
    assert_eq!(module.read().await.state.0, 42);

    let mut keys = store.list_keys("Tracked")?;
    keys.sort();
    assert_eq!(keys, ["port", "retries"]);

    Ok(())
}

/// A module without loaded fields needs nothing from the store.
#[derive(ConfigScope)]
#[confer(section = "Runtime")]
struct RuntimeOnly {
    #[confer(ignore)]
    counter: u64,
    #[confer(ignore)]
    names: Vec<String>,
}

#[tokio::test]
async fn module_without_loaded_fields_is_constructed_directly() -> Result<()> {
    let store = ConfigStore::new();
    let module = RuntimeOnly::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.counter, 0);
        assert!(guard.names.is_empty());
    }

    module.write().await.counter = 5;
    RuntimeOnly::load(&module, &store).await?;
    assert_eq!(module.read().await.counter, 5);

    RuntimeOnly::save(&module, &store).await?;
    assert!(store.list_keys("Runtime")?.is_empty());

    Ok(())
}

/// Implements neither `Default` nor a shape supported by the derive.
struct Pool {
    size: usize,
}

impl Pool {
    fn with_size(size: usize) -> Self {
        Pool { size }
    }
}

#[derive(ConfigScope)]
#[confer(section = "Workers")]
struct Workers {
    threads: u32,
    #[confer(ignore = Pool::with_size(4))]
    pool: Pool,
    #[confer(ignore = vec![1, 2, 3])]
    ids: Vec<u8>,
}

#[tokio::test]
async fn ignore_expression_fields_ignore_the_file_and_survive_load_and_save() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"[Workers]
threads = 2
ids = [9]
"#,
    )?;

    let module = Workers::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.threads, 2);
        assert_eq!(guard.pool.size, 4);
        assert_eq!(guard.ids, [1, 2, 3]);
    }

    {
        let mut guard = module.write().await;
        guard.pool = Pool::with_size(8);
        guard.ids.push(4);
    }

    Workers::load(&module, &store).await?;
    {
        let guard = module.read().await;
        assert_eq!(guard.pool.size, 8);
        assert_eq!(guard.ids, [1, 2, 3, 4]);
    }

    // An ignored field has no TOML key, so `save` writes none and treats a stray one like any
    // other unknown key.
    Workers::save(&module, &store).await?;
    assert_eq!(store.list_keys("Workers")?, ["threads"]);

    Ok(())
}

/// An ignored field has no TOML key, so its name may equal the key of another field.
#[derive(ConfigScope)]
#[confer(section = "Collision")]
struct KeyCollision {
    #[confer(rename = "name")]
    title: String,
    #[confer(ignore = 7)]
    name: u32,
}

#[tokio::test]
async fn ignored_field_may_share_its_name_with_the_key_of_another_field() -> Result<()> {
    let store = ConfigStore::from_str("[Collision]\nname = \"widget\"\n")?;
    let module = KeyCollision::from_store(&store)?;

    {
        let guard = module.read().await;
        assert_eq!(guard.title, "widget");
        assert_eq!(guard.name, 7);
    }

    module.write().await.title = "gadget".into();
    KeyCollision::save(&module, &store).await?;
    assert_eq!(store.get_string("Collision", "name")?, "gadget");

    Ok(())
}

static TICKETS: AtomicUsize = AtomicUsize::new(0);

fn next_ticket() -> usize {
    TICKETS.fetch_add(1, Ordering::SeqCst)
}

#[derive(ConfigScope)]
#[confer(section = "Ticketed")]
struct Ticketed {
    #[confer(ignore = next_ticket())]
    before: usize,
    port: u16,
    #[confer(ignore = next_ticket())]
    after: usize,
}

/// One test owns the counter, because tests in this file run in parallel.
#[tokio::test]
async fn ignore_expressions_run_once_in_declaration_order_after_conversion() -> Result<()> {
    let store = ConfigStore::from_str("[Ticketed]\nport = \"eighty\"\n")?;
    let err = Ticketed::from_store(&store)
        .err()
        .expect("a mistyped value must fail");
    assert!(matches!(&err, Error::TypeMismatch { key, .. } if key == "port"));
    assert_eq!(TICKETS.load(Ordering::SeqCst), 0);

    let store = ConfigStore::from_str("[Ticketed]\nport = 80\n")?;
    let module = Ticketed::from_store(&store)?;
    {
        let guard = module.read().await;
        assert_eq!((guard.before, guard.after), (0, 1));
    }

    Ticketed::load(&module, &store).await?;
    Ticketed::save(&module, &store).await?;
    assert_eq!(TICKETS.load(Ordering::SeqCst), 2);

    Ok(())
}

/// Initializers run in closures: `return` and `?` end the initializer, not `from_store`.
#[derive(ConfigScope)]
#[confer(section = "Closures")]
struct Closures {
    #[confer(ignore = {
        let digits = "7".parse::<u8>().ok()?;
        Some(digits)
    })]
    parsed: Option<u8>,
    #[confer(ignore = {
        let words: Vec<&str> = Vec::new();
        if words.is_empty() {
            return 1;
        }
        2
    })]
    early: u8,
}

#[tokio::test]
async fn ignore_expressions_cannot_return_from_from_store() -> Result<()> {
    let module = Closures::from_store(&ConfigStore::new())?;

    let guard = module.read().await;
    assert_eq!(guard.parsed, Some(7));
    assert_eq!(guard.early, 1);

    Ok(())
}
