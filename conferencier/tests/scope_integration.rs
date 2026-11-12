use std::sync::atomic::{AtomicUsize, Ordering};

use conferencier::{ConfigScope, ConfigStore, Datetime, Error, Result};

#[derive(ConfigScope)]
#[confer(section = "Srv")]
struct Server {
    #[confer(rename = "p")]
    port: u16,
    #[confer(default = "0.0.0.0")]
    host: String,
    #[confer(default = 3)]
    retries: i32,
    #[confer(default = ["alpha", "beta"])]
    features: Vec<String>,
    #[confer(default = "2024-01-01T00:00:00Z")]
    started_at: Datetime,
    #[confer(default = "example.com")]
    endpoint: Option<String>,
    notes: Option<String>,
    #[confer(ignore = Vec::new())]
    cache: Vec<u8>,
}

#[tokio::test]
async fn module_from_store_loads_defaults_and_values() -> Result<()> {
    let store = ConfigStore::from_string(
        r#"[Srv]
extra = 7
p = 8080
notes = "temp"
"#,
    )?;

    let module = Server::from_store(store.clone()).await?;

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

    Server::save(&module, store.clone()).await?;

    assert_eq!(store.get_string("Srv", "host").await?, "127.0.0.1");
    assert_eq!(store.get_integer("Srv", "p").await?, 8080);
    assert_eq!(store.get_integer("Srv", "retries").await?, 3);
    assert_eq!(
        store.get_string_vec("Srv", "features").await?,
        vec!["gamma".to_string()]
    );
    assert!(store.get_value("Srv", "endpoint").await.is_none());
    assert!(store.get_value("Srv", "notes").await.is_none());
    assert!(store.get_value("Srv", "extra").await.is_none());
    Ok(())
}

#[derive(ConfigScope)]
#[confer(section = "Nested")]
struct NestedModule {
    #[confer(default = [1, 2, 3])]
    counters: Option<Vec<u16>>,
    #[confer(default = [4, 5])]
    thresholds: Option<Vec<u16>>,
    #[confer(rename = "aliases")]
    #[confer(default = ["primary", "secondary"])]
    aliases: Option<Vec<String>>,
}

#[tokio::test]
async fn option_vec_fields_load_modify_and_save() -> Result<()> {
    let store = ConfigStore::from_string(
        r#"[Nested]
counters = [10, 20]
"#,
    )?;

    let module = NestedModule::from_store(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.counters.as_deref(), Some(&[10, 20][..]));
        assert_eq!(guard.thresholds.as_deref(), Some(&[4, 5][..]));
        assert_eq!(
            guard.aliases.as_deref(),
            Some(&["primary".to_string(), "secondary".to_string()][..])
        );
    }

    {
        let mut guard = module.write().await;
        guard.counters = None;
        guard.thresholds = Some(vec![30, 40, 50]);
        guard.aliases = Some(vec!["blue".into(), "green".into()]);
    }

    NestedModule::save(&module, store.clone()).await?;

    assert!(store.get_value("Nested", "counters").await.is_none());
    assert_eq!(
        store.get_integer_vec("Nested", "thresholds").await?,
        vec![30, 40, 50]
    );
    assert_eq!(
        store.get_string_vec("Nested", "aliases").await?,
        vec!["blue".to_string(), "green".to_string()]
    );

    Ok(())
}

/// Field names matching identifiers used inside the generated code.
#[derive(ConfigScope)]
#[confer(section = "Hygiene")]
struct Hygiene {
    store: String,
    module: String,
    guard: i64,
    value: Option<String>,
    key: Vec<String>,
}

#[tokio::test]
async fn field_names_do_not_collide_with_generated_locals() -> Result<()> {
    let store = ConfigStore::from_string(
        r#"[Hygiene]
store = "s"
module = "m"
guard = 1
value = "v"
key = ["k"]
"#,
    )?;

    let module = Hygiene::from_store(store.clone()).await?;

    {
        let mut guard = module.write().await;
        guard.store = "s2".into();
        guard.guard = 2;
        guard.value = None;
    }

    Hygiene::save(&module, store.clone()).await?;

    assert_eq!(store.get_string("Hygiene", "store").await?, "s2");
    assert_eq!(store.get_string("Hygiene", "module").await?, "m");
    assert_eq!(store.get_integer("Hygiene", "guard").await?, 2);
    assert!(store.get_value("Hygiene", "value").await.is_none());
    assert_eq!(
        store.get_string_vec("Hygiene", "key").await?,
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
    let store = ConfigStore::from_string(
        r#"[Raw]
type = "t"
"#,
    )?;

    let module = RawIdents::from_store(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.r#type, "t");
        assert_eq!(guard.r#loop, 3);
    }

    RawIdents::save(&module, store.clone()).await?;

    assert_eq!(store.get_string("Raw", "type").await?, "t");
    assert_eq!(store.get_integer("Raw", "loop").await?, 3);
    assert!(store.get_value("Raw", "r#type").await.is_none());

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
    #[confer(default = -3)]
    bias: f32,
    #[confer(default = [-1, 2])]
    deltas: Vec<i64>,
    #[confer(default = -7)]
    floor: Option<i64>,
}

#[tokio::test]
async fn negative_numeric_defaults() -> Result<()> {
    let store = ConfigStore::from_string("[Negative]\n")?;

    let module = NegativeDefaults::from_store(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.offset, -1);
        assert_eq!(guard.ratio, -2.5);
        assert_eq!(guard.bias, -3.0);
        assert_eq!(guard.deltas, vec![-1, 2]);
        assert_eq!(guard.floor, Some(-7));
    }

    NegativeDefaults::save(&module, store.clone()).await?;

    assert_eq!(store.get_integer("Negative", "offset").await?, -1);
    assert_eq!(store.get_float("Negative", "ratio").await?, -2.5);
    assert_eq!(store.get_float("Negative", "bias").await?, -3.0);
    assert_eq!(
        store.get_integer_vec("Negative", "deltas").await?,
        vec![-1, 2]
    );
    assert_eq!(store.get_integer("Negative", "floor").await?, -7);

    Ok(())
}

/// Default section names strip a leading `ConfigStore` word only.
#[derive(ConfigScope)]
struct ConferenceSettings {
    room: String,
}

#[derive(ConfigScope)]
struct ConferServer {
    port: u16,
}

#[tokio::test]
async fn default_section_name_strips_confer_word_only() -> Result<()> {
    let store = ConfigStore::from_string(
        r#"[ConferenceSettings]
room = "A1"

[Server]
port = 8080
"#,
    )?;

    let settings = ConferenceSettings::from_store(store.clone()).await?;
    let server = ConferServer::from_store(store.clone()).await?;

    assert_eq!(settings.read().await.room, "A1");
    assert_eq!(server.read().await.port, 8080);

    Ok(())
}

#[derive(ConfigScope)]
#[confer(section = "Schedule")]
struct ScheduleDefaults {
    #[confer(default = "07:30")]
    alarm: Datetime,
    #[confer(default = ["07:30", "08:45:00.125"])]
    reminders: Vec<Datetime>,
}

#[tokio::test]
async fn datetime_defaults_preserve_omitted_seconds() -> Result<()> {
    let store = ConfigStore::new();
    let module = ScheduleDefaults::from_store(store.clone()).await?;

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

    ScheduleDefaults::save(&module, store.clone()).await?;
    let restored = ConfigStore::from_string(&store.save_str().await?)?;
    let reloaded = ScheduleDefaults::from_store(restored).await?;
    let original = module.read().await;
    let reloaded = reloaded.read().await;
    assert_eq!(reloaded.alarm, original.alarm);
    assert_eq!(reloaded.reminders, original.reminders);
    Ok(())
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
    let store = ConfigStore::from_string(
        r#"[Clock]
started_at = 2024-05-01T10:30:00Z
laps = [2024-05-01T10:31:00Z]
"#,
    )?;

    let module = Clock::from_store(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.started_at.to_string(), "2024-05-01T10:30:00Z");
        assert!(guard.stopped_at.is_none());
        assert_eq!(guard.laps.len(), 1);
    }

    Clock::save(&module, store.clone()).await?;
    let restored = ConfigStore::from_string(&store.save_str().await?)?;
    let reloaded = Clock::from_store(restored).await?;
    assert_eq!(
        reloaded.read().await.started_at,
        module.read().await.started_at
    );

    Ok(())
}

#[tokio::test]
async fn required_datetime_without_default_reports_missing_key() -> Result<()> {
    let store = ConfigStore::from_string("[Clock]\n")?;

    let err = Clock::from_store(store)
        .await
        .err()
        .expect("a missing required key must fail");

    assert!(
        matches!(&err, Error::MissingKey { section, key } if section == "Clock" && key == "started_at"),
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
    let store = ConfigStore::from_string("[Tracked]\nretries = 1\n")?;
    let err = TrackedModule::from_store(store)
        .await
        .err()
        .expect("a missing required key must fail");
    assert!(matches!(&err, Error::MissingKey { key, .. } if key == "port"));
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 0);

    // A later field is out of range, and the first error wins in declaration order.
    let store = ConfigStore::from_string("[Tracked]\nport = 80\nretries = 300\n")?;
    let err = TrackedModule::from_store(store)
        .await
        .err()
        .expect("an out-of-range value must fail");
    assert!(matches!(&err, Error::ValueParse { key, .. } if key == "retries"));
    let store = ConfigStore::from_string("[Tracked]\nretries = 300\n")?;
    let err = TrackedModule::from_store(store)
        .await
        .err()
        .expect("a missing required key must fail");
    assert!(matches!(&err, Error::MissingKey { key, .. } if key == "port"));
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 0);

    // Success initializes the ignored field exactly once; load and save leave it alone.
    let store = ConfigStore::from_string("[Tracked]\nport = 80\nretries = 3\n")?;
    let module = TrackedModule::from_store(store.clone()).await?;
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 1);
    assert_eq!(module.read().await.state.0, 7);

    module.write().await.state = Tracked(42);
    TrackedModule::load(&module, store.clone()).await?;
    TrackedModule::save(&module, store.clone()).await?;
    assert_eq!(TRACKED_DEFAULTS.load(Ordering::SeqCst), 1);
    assert_eq!(module.read().await.state.0, 42);

    let mut keys = store.list_keys("Tracked").await?;
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
    let module = RuntimeOnly::from_store(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.counter, 0);
        assert!(guard.names.is_empty());
    }

    module.write().await.counter = 5;
    RuntimeOnly::load(&module, store.clone()).await?;
    assert_eq!(module.read().await.counter, 5);

    RuntimeOnly::save(&module, store.clone()).await?;
    assert!(store.list_keys("Runtime").await?.is_empty());

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
    let store = ConfigStore::from_string(
        r#"[Workers]
threads = 2
ids = [9]
"#,
    )?;

    let module = Workers::from_store(store.clone()).await?;

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

    Workers::load(&module, store.clone()).await?;
    {
        let guard = module.read().await;
        assert_eq!(guard.pool.size, 8);
        assert_eq!(guard.ids, [1, 2, 3, 4]);
    }

    // An ignored field has no TOML key, so `save` writes none and treats a stray one like any
    // other unknown key.
    Workers::save(&module, store.clone()).await?;
    assert_eq!(store.list_keys("Workers").await?, ["threads"]);

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
    let store = ConfigStore::from_string("[Collision]\nname = \"widget\"\n")?;
    let module = KeyCollision::from_store(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.title, "widget");
        assert_eq!(guard.name, 7);
    }

    module.write().await.title = "gadget".into();
    KeyCollision::save(&module, store.clone()).await?;
    assert_eq!(store.get_string("Collision", "name").await?, "gadget");

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
    let store = ConfigStore::from_string("[Ticketed]\nport = \"eighty\"\n")?;
    let err = Ticketed::from_store(store)
        .await
        .err()
        .expect("a mistyped value must fail");
    assert!(matches!(&err, Error::TypeMismatch { key, .. } if key == "port"));
    assert_eq!(TICKETS.load(Ordering::SeqCst), 0);

    let store = ConfigStore::from_string("[Ticketed]\nport = 80\n")?;
    let module = Ticketed::from_store(store.clone()).await?;
    {
        let guard = module.read().await;
        assert_eq!((guard.before, guard.after), (0, 1));
    }

    Ticketed::load(&module, store.clone()).await?;
    Ticketed::save(&module, store).await?;
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
    let module = Closures::from_store(ConfigStore::new()).await?;

    let guard = module.read().await;
    assert_eq!(guard.parsed, Some(7));
    assert_eq!(guard.early, 1);

    Ok(())
}
