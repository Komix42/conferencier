//! Nested configuration tables (RFC-0004).

use std::sync::atomic::{AtomicU32, Ordering};

use conferencier::toml::{Table, Value};
use conferencier::{ConfigScope, ConfigStore, ConfigTable, Error, Result};

fn error_of<T>(result: Result<T>) -> Error {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error,
    }
}

/// Saves `store` as TOML text and loads a fresh module from the text.
async fn reloaded<M: ConfigScope + Clone>(store: &ConfigStore) -> Result<M> {
    let text = store.save_str()?;
    let module = M::from_store(&ConfigStore::from_str(&text)?)?;
    let value = module.read().await.clone();
    Ok(value)
}

fn sorted_keys(table: &Table) -> Vec<&str> {
    let mut keys: Vec<_> = table.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

fn sub_table<'a>(table: &'a Table, key: &str) -> &'a Table {
    match table.get(key) {
        Some(Value::Table(table)) => table,
        other => panic!("expected a table at `{key}`, found {other:?}"),
    }
}

#[derive(Debug, Clone, PartialEq, Default, ConfigTable)]
struct DefaultTable {
    #[confer(default = 8080)]
    port: u16,
}

#[derive(Debug, ConfigTable)]
struct ManualDefaultTable {
    #[confer(default = 8080)]
    port: u16,
}

impl Default for ManualDefaultTable {
    fn default() -> Self {
        Self { port: 8080 }
    }
}

#[derive(Debug, ConfigScope)]
struct DefaultTables {
    #[confer(table, default)]
    rust_default: DefaultTable,
    #[confer(table, default)]
    manual_default: ManualDefaultTable,
    #[confer(table, default = DefaultTable { port: 42 })]
    expression: DefaultTable,
    #[confer(table, default = vec![DefaultTable { port: 99 }])]
    entries: Vec<DefaultTable>,
    #[confer(table, default)]
    empty: Vec<DefaultTable>,
}

#[tokio::test]
async fn table_defaults_use_the_field_type_and_roundtrip_empty_vectors() -> Result<()> {
    let store = ConfigStore::new();
    let instance = DefaultTables::from_store(&store)?;
    {
        let guard = instance.read().await;
        assert_eq!(guard.rust_default.port, 0);
        assert_eq!(guard.manual_default.port, 8080);
        assert_eq!(guard.expression.port, 42);
        assert_eq!(guard.entries, [DefaultTable { port: 99 }]);
        assert!(guard.empty.is_empty());
    }
    assert_eq!(DefaultTable::from_table("table", Table::new())?.port, 8080);

    instance.write().await.entries.clear();
    DefaultTables::save(&instance, &store).await?;
    let restored = DefaultTables::from_store(&ConfigStore::from_str(&store.save_str()?)?)?;
    let guard = restored.read().await;
    assert!(guard.entries.is_empty());
    assert!(guard.empty.is_empty());
    Ok(())
}

#[derive(Debug, ConfigScope)]
struct QualifiedOptions {
    std_port: std::option::Option<u16>,
    core_numbers: core::option::Option<Vec<u16>>,
    #[confer(table)]
    std_table: std::option::Option<DefaultTable>,
    #[confer(table)]
    core_table: core::option::Option<DefaultTable>,
}

#[tokio::test]
async fn qualified_options_load_missing_and_present_values() -> Result<()> {
    let empty = QualifiedOptions::from_store(&ConfigStore::new())?;
    {
        let guard = empty.read().await;
        assert_eq!(guard.std_port, None);
        assert_eq!(guard.core_numbers, None);
        assert_eq!(guard.std_table, None);
        assert_eq!(guard.core_table, None);
    }

    let store = ConfigStore::from_str(
        "[QualifiedOptions]\nstd_port = 1\ncore_numbers = [2, 3]\n\
         [QualifiedOptions.std_table]\nport = 4\n\
         [QualifiedOptions.core_table]\nport = 5",
    )?;
    let present = QualifiedOptions::from_store(&store)?;
    let guard = present.read().await;
    assert_eq!(guard.std_port, Some(1));
    assert_eq!(guard.core_numbers.as_deref(), Some(&[2, 3][..]));
    assert_eq!(guard.std_table, Some(DefaultTable { port: 4 }));
    assert_eq!(guard.core_table, Some(DefaultTable { port: 5 }));
    Ok(())
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct Window {
    seconds: u32,
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct Limits {
    #[confer(default = 10)]
    burst: u32,
    rate: Option<f64>,
    #[confer(table)]
    window: Option<Window>,
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct Server {
    #[confer(default = "localhost".into())]
    host: String,
    #[confer(default = 8080)]
    port: u16,
    #[confer(table)]
    limits: Limits,
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct LenientChild {
    port: u16,
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
#[confer(deny_unknown_keys)]
struct StrictChild {
    port: u16,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Outer", deny_unknown_keys)]
struct StrictOuter {
    #[confer(table)]
    child: LenientChild,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Outer")]
struct LenientOuter {
    #[confer(table)]
    child: StrictChild,
    #[confer(table)]
    children: Vec<StrictChild>,
}

#[derive(Debug, PartialEq, ConfigTable)]
#[confer(deny_unknown_keys)]
struct StrictIgnoredTable {
    #[confer(ignore)]
    runtime: String,
}

#[derive(Debug)]
struct ManualChild {
    value: u16,
}

impl ConfigTable for ManualChild {
    fn from_table(path: &str, mut table: Table) -> Result<Self> {
        let value = match table.remove("value") {
            Some(Value::Integer(value)) => u16::try_from(value)
                .map_err(|_| Error::value_parse(path, "value", "value out of range for u16"))?,
            Some(other) => {
                return Err(Error::type_mismatch(
                    path,
                    "value",
                    "integer",
                    other.type_str(),
                ));
            }
            None => return Err(Error::missing_key(path, "value")),
        };
        Ok(Self { value })
    }

    fn to_table(&self, _path: &str) -> Result<Table> {
        let mut table = Table::new();
        table.insert("value".into(), Value::Integer(i64::from(self.value)));
        Ok(table)
    }
}

#[derive(Debug)]
struct ManualScope {
    child: ManualChild,
}

impl ConfigScope for ManualScope {
    fn from_store(store: &ConfigStore) -> Result<conferencier::SharedScope<Self>> {
        let path = conferencier::table_path::section_path("Manual");
        let mut section = store.get_section("Manual").unwrap_or_default();
        let child = match section.remove("child") {
            Some(Value::Table(table)) => table,
            Some(other) => {
                return Err(Error::type_mismatch(
                    &path,
                    "child",
                    "table",
                    other.type_str(),
                ));
            }
            None => return Err(Error::missing_key(&path, "child")),
        };
        let child_path = conferencier::table_path::child_path(&path, "child");
        let child = ManualChild::from_table(&child_path, child)?;
        Ok(std::sync::Arc::new(tokio::sync::RwLock::new(Self {
            child,
        })))
    }

    async fn load(_scope: &conferencier::SharedScope<Self>, _store: &ConfigStore) -> Result<()> {
        Ok(())
    }

    async fn save(_scope: &conferencier::SharedScope<Self>, store: &ConfigStore) -> Result<()> {
        store.set_section("Manual", Table::new())
    }
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct Tls {
    cert: String,
    key: String,
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, ConfigScope)]
#[confer(section = "App")]
struct App {
    name: String,
    #[confer(table)]
    server: Server,
    #[confer(table, rename = "tls-config")]
    tls: Option<Tls>,
    #[confer(table)]
    upstream: Vec<Upstream>,
}

const FULL: &str = r#"
[App]
name = "demo"

[App.server]
host = "example.com"
port = 443

[App.server.limits]
burst = 20
rate = 1.5

[App.server.limits.window]
seconds = 60

[App.tls-config]
cert = "cert.pem"
key = "key.pem"

[[App.upstream]]
url = "http://a"
weight = 3
tags = ["primary"]

[[App.upstream]]
url = "http://b"
"#;

fn full_app() -> App {
    App {
        name: "demo".into(),
        server: Server {
            host: "example.com".into(),
            port: 443,
            limits: Limits {
                burst: 20,
                rate: Some(1.5),
                window: Some(Window { seconds: 60 }),
            },
        },
        tls: Some(Tls {
            cert: "cert.pem".into(),
            key: "key.pem".into(),
        }),
        upstream: vec![
            Upstream {
                url: "http://a".into(),
                weight: 3,
                tags: Some(vec!["primary".into()]),
            },
            Upstream {
                url: "http://b".into(),
                weight: 1,
                tags: None,
            },
        ],
    }
}

#[tokio::test]
async fn all_shapes_roundtrip_through_three_levels() -> Result<()> {
    let store = ConfigStore::from_str(FULL)?;
    let module = App::from_store(&store)?;
    assert_eq!(*module.read().await, full_app());

    let saved = ConfigStore::new();
    App::save(&module, &saved).await?;
    let section = saved.get_section("App").expect("saved section");
    assert_eq!(
        sorted_keys(&section),
        ["name", "server", "tls-config", "upstream"]
    );
    let limits = sub_table(sub_table(&section, "server"), "limits");
    assert_eq!(
        sub_table(limits, "window").get("seconds"),
        Some(&Value::Integer(60))
    );
    assert!(matches!(section.get("upstream"), Some(Value::Array(array)) if array.len() == 2));

    assert_eq!(reloaded::<App>(&saved).await?, full_app());
    Ok(())
}

#[tokio::test]
async fn multi_line_inline_tables_load_and_roundtrip() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"
[App]
name = "demo"
server = {
    host = "example.com", # the public host
    port = 443,
    limits = {
        burst = 20,
        rate = 1.5,
        window = { seconds = 60, },
    },
}
tls-config = { cert = "cert.pem", key = "key.pem" }
upstream = [
    { url = "http://a", weight = 3, tags = ["primary"] }, # preferred
    { url = "http://b" },
]
"#,
    )?;
    let module = App::from_store(&store)?;
    assert_eq!(*module.read().await, full_app());

    App::save(&module, &store).await?;
    assert_eq!(reloaded::<App>(&store).await?, full_app());
    Ok(())
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Strict")]
struct Strict {
    #[confer(table)]
    tls: Tls,
}

#[tokio::test]
async fn absent_tables_follow_their_shape() -> Result<()> {
    let module = App::from_store(&ConfigStore::from_str("[App]\nname = \"demo\"")?)?;
    let app = module.read().await;
    assert_eq!(
        app.server,
        Server {
            host: "localhost".into(),
            port: 8080,
            limits: Limits {
                burst: 10,
                rate: None,
                window: None,
            },
        }
    );
    assert_eq!(app.tls, None);
    assert!(app.upstream.is_empty());

    let error = error_of(Strict::from_store(&ConfigStore::from_str("[Strict]")?));
    assert!(
        matches!(&error, Error::MissingKey { path, key }
            if path == "Strict.tls" && key == "cert"),
        "unexpected error: {error:?}"
    );
    assert_eq!(error.to_string(), "missing key Strict.tls.cert");
    Ok(())
}

#[derive(Debug, ConfigTable)]
struct Extras {
    note: Option<String>,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Sparse")]
struct Sparse {
    #[confer(table)]
    extras: Extras,
}

#[tokio::test]
async fn save_writes_owned_data_only() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"
[App]
name = "demo"
stale = 1

[App.server]
host = "example.com"
extra = true

[App.server.limits]
rate = 1.5
unknown = "x"

[App.server.unknown]
x = 1

[App.tls-config]
cert = "cert.pem"
key = "key.pem"
legacy = true

[[App.upstream]]
url = "http://a"
retired = true

[App.other]
y = 2
"#,
    )?;
    let module = App::from_store(&store)?;

    // Unknown keys and sub-tables are removed at every level (O1–O2).
    App::save(&module, &store).await?;
    let section = store.get_section("App").expect("saved section");
    assert_eq!(
        sorted_keys(&section),
        ["name", "server", "tls-config", "upstream"]
    );
    let server = sub_table(&section, "server");
    assert_eq!(sorted_keys(server), ["host", "limits", "port"]);
    assert_eq!(sorted_keys(sub_table(server, "limits")), ["burst", "rate"]);
    assert_eq!(
        sorted_keys(sub_table(&section, "tls-config")),
        ["cert", "key"]
    );
    match section.get("upstream") {
        Some(Value::Array(array)) => match array.as_slice() {
            [Value::Table(element)] => assert_eq!(sorted_keys(element), ["url", "weight"]),
            other => panic!("unexpected upstream elements: {other:?}"),
        },
        other => panic!("unexpected upstream: {other:?}"),
    }

    // `None` removes its key; an empty vector of tables is written as an empty array.
    {
        let mut guard = module.write().await;
        guard.tls = None;
        guard.upstream.clear();
        guard.server.limits.rate = None;
    }
    App::save(&module, &store).await?;
    let section = store.get_section("App").expect("saved section");
    assert_eq!(sorted_keys(&section), ["name", "server", "upstream"]);
    assert!(matches!(section.get("upstream"), Some(Value::Array(array)) if array.is_empty()));
    assert_eq!(
        sorted_keys(sub_table(sub_table(&section, "server"), "limits")),
        ["burst"]
    );
    assert!(reloaded::<App>(&store).await?.upstream.is_empty());

    // A plain table is always written, even when empty.
    let store = ConfigStore::new();
    let module = Sparse::from_store(&store)?;
    assert_eq!(module.read().await.extras.note, None);
    Sparse::save(&module, &store).await?;
    let section = store.get_section("Sparse").expect("saved section");
    assert!(sub_table(&section, "extras").is_empty());
    assert!(store.save_str()?.contains("[Sparse.extras]"));
    Ok(())
}

#[derive(Debug, ConfigTable)]
struct Endpoint {
    port: u16,
}

#[derive(Debug, ConfigTable)]
struct Weighted {
    url: String,
    weight: u32,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "App")]
struct Checked {
    #[confer(table)]
    server: Option<Endpoint>,
    #[confer(table)]
    upstream: Vec<Weighted>,
    #[confer(table, rename = "my key")]
    spaced: Option<Endpoint>,
}

async fn checked_error(source: &str) -> Result<Error> {
    Ok(error_of(Checked::from_store(&ConfigStore::from_str(
        source,
    )?)))
}

#[tokio::test]
async fn errors_report_nested_paths() -> Result<()> {
    let error = checked_error("[App.server]\nother = 1").await?;
    assert!(
        matches!(&error, Error::MissingKey { path, key }
            if path == "App.server" && key == "port"),
        "unexpected error: {error:?}"
    );
    assert_eq!(error.to_string(), "missing key App.server.port");

    let error = checked_error("[App]\nserver = 5").await?;
    assert!(
        matches!(&error, Error::TypeMismatch { path, key, expected: "table", found: "integer" }
            if path == "App" && key == "server"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "expected table at App.server but found integer"
    );

    let error = checked_error("[App.upstream]\nurl = \"a\"\nweight = 1").await?;
    assert!(
        matches!(&error, Error::TypeMismatch { path, key, expected: "array", found: "table" }
            if path == "App" && key == "upstream"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "expected array at App.upstream but found table"
    );

    let error = checked_error("[App]\nupstream = [{ url = \"a\", weight = 1 }, \"b\"]").await?;
    assert!(
        matches!(&error, Error::ValueParse { path, key, message }
            if path == "App"
                && key == "upstream"
                && message == "expected array elements of type table, found string (at index 1)"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "invalid value at App.upstream: expected array elements of type table, found string (at index 1)"
    );

    let error = checked_error(
        "[[App.upstream]]\nurl = \"a\"\nweight = 1\n[[App.upstream]]\nurl = \"b\"\nweight = -1",
    )
    .await?;
    assert!(
        matches!(&error, Error::ValueParse { path, key, message }
            if path == "App.upstream[1]"
                && key == "weight"
                && message == "value out of range for u32"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "invalid value at App.upstream[1].weight: value out of range for u32"
    );

    let error = checked_error("[App]\n\"my key\" = { port = \"bad\" }").await?;
    assert!(
        matches!(&error, Error::TypeMismatch { path, key, expected: "integer", found: "string" }
            if path == "App.\"my key\"" && key == "port"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "expected integer at App.\"my key\".port but found string"
    );
    Ok(())
}

#[tokio::test]
async fn strictness_is_per_struct_and_reports_nested_paths() -> Result<()> {
    let error = LenientOuter::from_store(&ConfigStore::from_str("[Outer]")?).unwrap_err();
    assert!(matches!(
        error,
        Error::MissingKey { ref path, ref key }
            if path == "Outer.child" && key == "port"
    ));

    let store = ConfigStore::from_str("[Outer]\nchild = { port = 80, typo = 1 }")?;
    let scope = StrictOuter::from_store(&store)?;
    assert_eq!(scope.read().await.child.port, 80);

    let store = ConfigStore::from_str(
        "[Outer]\nchild = { port = 80, typo = 1 }\nchildren = [{ port = 81 }]",
    )?;
    let error = LenientOuter::from_store(&store).unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "Outer.child" && key == "typo"
    ));

    let store = ConfigStore::from_str(
        "[Outer]\nchild = { port = 80 }\nchildren = [{ port = 81, typo = 1 }]",
    )?;
    let error = LenientOuter::from_store(&store).unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "Outer.children[0]" && key == "typo"
    ));

    let mut table = Table::new();
    table.insert("extra".into(), Value::Boolean(true));
    let error = StrictChild::from_table("Manual", table).unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "Manual" && key == "extra"
    ));

    assert_eq!(
        StrictIgnoredTable::from_table("Manual", Table::new())?,
        StrictIgnoredTable {
            runtime: String::new(),
        }
    );
    let mut table = Table::new();
    table.insert("extra".into(), Value::Boolean(true));
    let error = StrictIgnoredTable::from_table("Manual", table).unwrap_err();
    assert!(matches!(
        error,
        Error::UnknownKey { ref path, ref key }
            if path == "Manual" && key == "extra"
    ));
    Ok(())
}

#[test]
fn manual_scope_and_table_helpers_build_matching_error_paths() -> Result<()> {
    let error = ManualScope::from_store(&ConfigStore::from_str("[Manual.child]")?).unwrap_err();
    assert!(matches!(
        error,
        Error::MissingKey { ref path, ref key }
            if path == "Manual.child" && key == "value"
    ));
    assert_eq!(error.to_string(), "missing key Manual.child.value");

    let scope = ManualScope::from_store(&ConfigStore::from_str("[Manual.child]\nvalue = 7")?)?;
    assert_eq!(
        scope.try_read().expect("no other guard exists").child.value,
        7
    );
    Ok(())
}

#[tokio::test]
async fn array_elements_are_converted_depth_first() -> Result<()> {
    // The first element misses a required key and the second is not a table (L5).
    let error = checked_error("[App]\nupstream = [{ url = \"a\" }, \"b\"]").await?;
    assert!(
        matches!(&error, Error::MissingKey { path, key }
            if path == "App.upstream[0]" && key == "weight"),
        "unexpected error: {error:?}"
    );
    Ok(())
}

static TOKENS: AtomicU32 = AtomicU32::new(0);

fn next_token() -> u32 {
    TOKENS.fetch_add(1, Ordering::SeqCst) + 1
}

#[derive(Debug, ConfigTable)]
struct Session {
    id: String,
    #[confer(ignore = next_token())]
    token: u32,
    #[confer(ignore)]
    hits: u32,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Sessions")]
struct Sessions {
    #[confer(table)]
    main: Session,
    #[confer(table)]
    extra: Vec<Session>,
    port: u16,
    #[confer(ignore = 100)]
    local: u32,
}

#[tokio::test]
async fn ignored_fields_follow_value_semantics() -> Result<()> {
    let store = ConfigStore::from_str("[Sessions]\nport = 1\n[Sessions.main]\nid = \"a\"")?;
    let module = Sessions::from_store(&store)?;
    {
        let mut guard = module.write().await;
        assert_eq!(guard.local, 100);
        assert_eq!(guard.main.hits, 0);
        assert!(guard.extra.is_empty());
        guard.main.hits = 5;
        guard.local = 7;
    }

    // I1: a successful load builds new table values; I2: module-level ignored fields stay.
    store.load_str(
        "[Sessions]\nport = 2\n[Sessions.main]\nid = \"b\"\n[[Sessions.extra]]\nid = \"x\"",
    )?;
    let before = TOKENS.load(Ordering::SeqCst);
    Sessions::load(&module, &store).await?;
    {
        let mut guard = module.write().await;
        assert_eq!(guard.main.id, "b");
        assert_eq!(guard.main.hits, 0);
        assert!(guard.main.token > before);
        assert_eq!(guard.extra.len(), 1);
        assert_eq!(guard.extra[0].id, "x");
        assert_eq!(guard.extra[0].hits, 0);
        assert!(guard.extra[0].token > before);
        assert_eq!(guard.port, 2);
        assert_eq!(guard.local, 7);
        guard.main.hits = 9;
    }

    // R2: a failing load changes nothing, but initializers of temporary values already ran.
    store.load_str("[Sessions]\nport = 70000\n[Sessions.main]\nid = \"c\"")?;
    let token = module.read().await.main.token;
    let before = TOKENS.load(Ordering::SeqCst);
    let error = error_of(Sessions::load(&module, &store).await);
    assert_eq!(
        error.to_string(),
        "invalid value at Sessions.port: value out of range for u16"
    );
    assert!(TOKENS.load(Ordering::SeqCst) > before);

    let guard = module.read().await;
    assert_eq!(guard.main.id, "b");
    assert_eq!(guard.main.hits, 9);
    assert_eq!(guard.main.token, token);
    assert_eq!(guard.extra.len(), 1);
    assert_eq!(guard.port, 2);
    assert_eq!(guard.local, 7);
    Ok(())
}

#[derive(Debug, ConfigTable)]
struct Counter {
    total: u64,
}

#[derive(Debug, ConfigScope)]
#[confer(section = "Stats")]
struct Stats {
    label: String,
    #[confer(table)]
    counters: Vec<Counter>,
}

#[tokio::test]
async fn nested_save_errors_leave_the_store_unchanged() -> Result<()> {
    let store = ConfigStore::from_str("[Stats]\nlabel = \"a\"\n[[Stats.counters]]\ntotal = 1")?;
    let module = Stats::from_store(&store)?;
    {
        let mut guard = module.write().await;
        guard.label = "b".into();
        guard.counters.push(Counter { total: u64::MAX });
    }

    let before = store.get_section("Stats");
    let error = error_of(Stats::save(&module, &store).await);
    assert!(
        matches!(&error, Error::ValueParse { path, key, .. }
            if path == "Stats.counters[1]" && key == "total"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "invalid value at Stats.counters[1].total: value `{}` out of range for TOML integer",
            u64::MAX
        )
    );
    assert_eq!(store.get_section("Stats"), before);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, ConfigTable)]
struct TreeNode {
    name: String,
    #[confer(table)]
    children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, ConfigScope)]
#[confer(section = "Tree")]
struct Tree {
    #[confer(table)]
    root: TreeNode,
}

fn node(name: &str, children: Vec<TreeNode>) -> TreeNode {
    TreeNode {
        name: name.into(),
        children,
    }
}

#[tokio::test]
async fn recursive_tables_roundtrip() -> Result<()> {
    let store = ConfigStore::from_str("[Tree.root]\nname = \"root\"")?;
    let module = Tree::from_store(&store)?;
    assert_eq!(module.read().await.root, node("root", vec![]));

    let expected = node(
        "root",
        vec![
            node("a", vec![node("a1", vec![]), node("a2", vec![])]),
            node("b", vec![]),
        ],
    );
    module.write().await.root = expected.clone();
    Tree::save(&module, &store).await?;
    assert_eq!(reloaded::<Tree>(&store).await?.root, expected);

    let error = error_of(Tree::from_store(&ConfigStore::from_str(
        "[Tree.root]\nname = \"root\"\n[[Tree.root.children]]\nname = \"a\"\n[[Tree.root.children.children]]\nname = 5",
    )?));
    assert_eq!(
        error.to_string(),
        "expected string at Tree.root.children[0].children[0].name but found integer"
    );
    Ok(())
}

/// Neither `Default` nor `Clone` is required by either derive.
#[derive(ConfigTable)]
struct Bare {
    id: String,
    #[confer(table)]
    window: Option<Window>,
}

#[derive(ConfigScope)]
#[confer(section = "Bare")]
struct BareHolder {
    #[confer(table)]
    one: Bare,
    #[confer(table)]
    many: Vec<Bare>,
}

#[tokio::test]
async fn tables_need_neither_default_nor_clone() -> Result<()> {
    let store = ConfigStore::from_str(
        "[Bare.one]\nid = \"one\"\n[Bare.one.window]\nseconds = 5\n[[Bare.many]]\nid = \"m\"",
    )?;
    let module = BareHolder::from_store(&store)?;
    let saved = ConfigStore::new();
    BareHolder::save(&module, &saved).await?;

    let module = BareHolder::from_store(&ConfigStore::from_str(&saved.save_str()?)?)?;
    let guard = module.read().await;
    assert_eq!(guard.one.id, "one");
    assert_eq!(guard.one.window, Some(Window { seconds: 5 }));
    assert_eq!(guard.many.len(), 1);
    assert_eq!(guard.many[0].id, "m");
    assert_eq!(guard.many[0].window, None);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, ConfigScope, ConfigTable)]
#[confer(section = "Database")]
struct Database {
    url: String,
    #[confer(default = 4)]
    pool: u32,
}

#[derive(Debug, Clone, PartialEq, ConfigScope, ConfigTable)]
struct Cache {
    #[confer(default = 64)]
    size: u32,
}

#[derive(Debug, Clone, PartialEq, ConfigScope)]
#[confer(section = "Services")]
struct Services {
    #[confer(table)]
    primary: Database,
    #[confer(table)]
    replicas: Vec<Database>,
    #[confer(table)]
    cache: Option<Cache>,
}

#[tokio::test]
async fn one_struct_can_derive_both_traits() -> Result<()> {
    let store = ConfigStore::from_str(
        r#"
[Database]
url = "top"

[Cache]
size = 128

[Services.primary]
url = "primary"
pool = 8

[[Services.replicas]]
url = "replica"

[Services.cache]
size = 32
"#,
    )?;

    let database = Database::from_store(&store)?;
    assert_eq!(
        *database.read().await,
        Database {
            url: "top".into(),
            pool: 4,
        }
    );
    let cache = Cache::from_store(&store)?;
    assert_eq!(cache.read().await.size, 128);

    let expected = Services {
        primary: Database {
            url: "primary".into(),
            pool: 8,
        },
        replicas: vec![Database {
            url: "replica".into(),
            pool: 4,
        }],
        cache: Some(Cache { size: 32 }),
    };
    let services = Services::from_store(&store)?;
    assert_eq!(*services.read().await, expected);

    let saved = ConfigStore::new();
    Services::save(&services, &saved).await?;
    assert_eq!(saved.list_sections(), ["Services"]);
    assert_eq!(reloaded::<Services>(&saved).await?, expected);
    Ok(())
}

/// Stored as `{ address = "host:port" }`.
#[derive(Debug, Clone, PartialEq)]
struct Address {
    host: String,
    port: u16,
}

impl ConfigTable for Address {
    fn from_table(path: &str, mut table: Table) -> Result<Self> {
        let raw = match table.remove("address") {
            Some(Value::String(raw)) => raw,
            Some(other) => {
                return Err(Error::type_mismatch(
                    path,
                    "address",
                    "string",
                    other.type_str(),
                ));
            }
            None => return Err(Error::missing_key(path, "address")),
        };
        let (host, port) = raw
            .rsplit_once(':')
            .ok_or_else(|| Error::value_parse(path, "address", "expected host:port"))?;
        let port = port
            .parse()
            .map_err(|_| Error::value_parse(path, "address", "invalid port"))?;
        Ok(Self {
            host: host.into(),
            port,
        })
    }

    fn to_table(&self, _path: &str) -> Result<Table> {
        let mut table = Table::new();
        table.insert(
            "address".into(),
            Value::String(format!("{}:{}", self.host, self.port)),
        );
        Ok(table)
    }
}

#[derive(Debug, Clone, PartialEq, ConfigScope)]
#[confer(section = "Net")]
struct Net {
    #[confer(table)]
    listen: Address,
    #[confer(table)]
    peers: Vec<Address>,
}

#[tokio::test]
async fn manual_implementations_can_be_table_fields() -> Result<()> {
    let store = ConfigStore::from_str(
        "[Net.listen]\naddress = \"0.0.0.0:80\"\n[[Net.peers]]\naddress = \"a:1\"\n[[Net.peers]]\naddress = \"b:2\"",
    )?;
    let module = Net::from_store(&store)?;
    let expected = Net {
        listen: Address {
            host: "0.0.0.0".into(),
            port: 80,
        },
        peers: vec![
            Address {
                host: "a".into(),
                port: 1,
            },
            Address {
                host: "b".into(),
                port: 2,
            },
        ],
    };
    assert_eq!(*module.read().await, expected);

    Net::save(&module, &store).await?;
    assert_eq!(reloaded::<Net>(&store).await?, expected);

    store.load_str("[Net.listen]\naddress = \"0.0.0.0:80\"\n[[Net.peers]]\naddress = \"b\"")?;
    let error = error_of(Net::load(&module, &store).await);
    assert_eq!(
        error.to_string(),
        "invalid value at Net.peers[0].address: expected host:port"
    );
    Ok(())
}

mod hygiene {
    use conferencier::{ConfigScope, ConfigStore, ConfigTable, Result};

    // A local `toml` module must not shadow the TOML types used by generated code.
    mod toml {}

    /// Field names matching identifiers used inside the generated code.
    #[derive(Debug, Clone, PartialEq, ConfigTable)]
    struct Names {
        path: String,
        table: i64,
        store: Option<String>,
        module: Vec<String>,
        guard: bool,
        value: f64,
        key: u64,
        #[confer(table)]
        index: Vec<Names>,
    }

    #[derive(Debug, Clone, PartialEq, ConfigScope)]
    #[confer(section = "Hygiene")]
    struct HygieneModule {
        path: String,
        table: i64,
        store: String,
        module: String,
        scope: String,
        guard: i64,
        value: Option<String>,
        key: Vec<String>,
        #[confer(table)]
        names: Names,
        #[confer(table)]
        array: Vec<Names>,
    }

    #[tokio::test]
    async fn field_names_do_not_collide_with_generated_code() -> Result<()> {
        let store = ConfigStore::from_str(
            r#"
[Hygiene]
path = "p"
table = 1
store = "s"
module = "m"
scope = "sc"
guard = 2
value = "v"
key = ["k"]

[Hygiene.names]
path = "np"
table = 3
module = ["nm"]
guard = true
value = 1.5
key = 4

[[Hygiene.names.index]]
path = "ip"
table = 7
module = []
guard = false
value = 0.5
key = 8

[[Hygiene.array]]
path = "ap"
table = 5
module = []
guard = false
value = 2.5
key = 6
"#,
        )?;
        let module = HygieneModule::from_store(&store)?;
        let loaded = module.read().await.clone();
        assert_eq!(loaded.path, "p");
        assert_eq!(loaded.names.table, 3);
        assert_eq!(loaded.names.index[0].key, 8);
        assert_eq!(loaded.array[0].key, 6);

        HygieneModule::save(&module, &store).await?;
        let text = store.save_str()?;
        let reloaded = HygieneModule::from_store(&ConfigStore::from_str(&text)?)?;
        assert_eq!(*reloaded.read().await, loaded);
        Ok(())
    }
}
