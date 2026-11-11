//! Nested configuration tables (RFC-0004).

use std::sync::atomic::{AtomicU32, Ordering};

use conferencier::toml::{Table, Value};
use conferencier::{Confer, ConferError, ConferModule, ConferTable, Result, SharedConfer};

fn error_of<T>(result: Result<T>) -> ConferError {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error,
    }
}

/// Saves `store` as TOML text and loads a fresh module from the text.
async fn reloaded<M: ConferModule + Clone>(store: &SharedConfer) -> Result<M> {
    let text = store.save_str().await?;
    let module = M::from_confer(Confer::from_string(&text)?).await?;
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

#[derive(Debug, Clone, PartialEq, ConferTable)]
struct Window {
    seconds: u32,
}

#[derive(Debug, Clone, PartialEq, ConferTable)]
struct Limits {
    #[confer(default = 10)]
    burst: u32,
    rate: Option<f64>,
    #[confer(table)]
    window: Option<Window>,
}

#[derive(Debug, Clone, PartialEq, ConferTable)]
struct Server {
    #[confer(default = "localhost")]
    host: String,
    #[confer(default = 8080)]
    port: u16,
    #[confer(table)]
    limits: Limits,
}

#[derive(Debug, Clone, PartialEq, ConferTable)]
struct Tls {
    cert: String,
    key: String,
}

#[derive(Debug, Clone, PartialEq, ConferTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, ConferModule)]
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
    let store = Confer::from_string(FULL)?;
    let module = App::from_confer(store.clone()).await?;
    assert_eq!(*module.read().await, full_app());

    let saved = Confer::new();
    App::save(&module, saved.clone()).await?;
    let section = saved.get_section_table("App").await.expect("saved section");
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
    let store = Confer::from_string(
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
    let module = App::from_confer(store.clone()).await?;
    assert_eq!(*module.read().await, full_app());

    App::save(&module, store.clone()).await?;
    assert_eq!(reloaded::<App>(&store).await?, full_app());
    Ok(())
}

#[derive(Debug, ConferModule)]
#[confer(section = "Strict")]
struct Strict {
    #[confer(table)]
    tls: Tls,
}

#[tokio::test]
async fn absent_tables_follow_their_shape() -> Result<()> {
    let module = App::from_confer(Confer::from_string("[App]\nname = \"demo\"")?).await?;
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

    let error = error_of(Strict::from_confer(Confer::from_string("[Strict]")?).await);
    assert!(
        matches!(&error, ConferError::MissingKey { section, key }
            if section == "Strict.tls" && key == "cert"),
        "unexpected error: {error:?}"
    );
    assert_eq!(error.to_string(), "missing key Strict.tls.cert");
    Ok(())
}

#[derive(Debug, ConferTable)]
struct Extras {
    note: Option<String>,
}

#[derive(Debug, ConferModule)]
#[confer(section = "Sparse")]
struct Sparse {
    #[confer(table)]
    extras: Extras,
}

#[tokio::test]
async fn save_writes_owned_data_only() -> Result<()> {
    let store = Confer::from_string(
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
    let module = App::from_confer(store.clone()).await?;

    // Unknown keys and sub-tables are removed at every level (O1–O2).
    App::save(&module, store.clone()).await?;
    let section = store.get_section_table("App").await.expect("saved section");
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

    // `None` and an empty vector of tables remove their keys.
    {
        let mut guard = module.write().await;
        guard.tls = None;
        guard.upstream.clear();
        guard.server.limits.rate = None;
    }
    App::save(&module, store.clone()).await?;
    let section = store.get_section_table("App").await.expect("saved section");
    assert_eq!(sorted_keys(&section), ["name", "server"]);
    assert_eq!(
        sorted_keys(sub_table(sub_table(&section, "server"), "limits")),
        ["burst"]
    );

    // A plain table is always written, even when empty.
    let store = Confer::new();
    let module = Sparse::from_confer(store.clone()).await?;
    assert_eq!(module.read().await.extras.note, None);
    Sparse::save(&module, store.clone()).await?;
    let section = store
        .get_section_table("Sparse")
        .await
        .expect("saved section");
    assert!(sub_table(&section, "extras").is_empty());
    assert!(store.save_str().await?.contains("[Sparse.extras]"));
    Ok(())
}

#[derive(Debug, ConferTable)]
struct Endpoint {
    port: u16,
}

#[derive(Debug, ConferTable)]
struct Weighted {
    url: String,
    weight: u32,
}

#[derive(Debug, ConferModule)]
#[confer(section = "App")]
struct Checked {
    #[confer(table)]
    server: Option<Endpoint>,
    #[confer(table)]
    upstream: Vec<Weighted>,
    #[confer(table, rename = "my key")]
    spaced: Option<Endpoint>,
}

async fn checked_error(source: &str) -> Result<ConferError> {
    Ok(error_of(
        Checked::from_confer(Confer::from_string(source)?).await,
    ))
}

#[tokio::test]
async fn errors_report_nested_paths() -> Result<()> {
    let error = checked_error("[App.server]\nother = 1").await?;
    assert!(
        matches!(&error, ConferError::MissingKey { section, key }
            if section == "App.server" && key == "port"),
        "unexpected error: {error:?}"
    );
    assert_eq!(error.to_string(), "missing key App.server.port");

    let error = checked_error("[App]\nserver = 5").await?;
    assert!(
        matches!(&error, ConferError::TypeMismatch { section, key, expected: "table", found: "integer" }
            if section == "App" && key == "server"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "expected table at App.server but found integer"
    );

    let error = checked_error("[App.upstream]\nurl = \"a\"\nweight = 1").await?;
    assert!(
        matches!(&error, ConferError::TypeMismatch { section, key, expected: "array", found: "table" }
            if section == "App" && key == "upstream"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "expected array at App.upstream but found table"
    );

    let error = checked_error("[App]\nupstream = [{ url = \"a\", weight = 1 }, \"b\"]").await?;
    assert!(
        matches!(&error, ConferError::ValueParse { section, key, message }
            if section == "App"
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
        matches!(&error, ConferError::ValueParse { section, key, message }
            if section == "App.upstream[1]"
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
        matches!(&error, ConferError::TypeMismatch { section, key, expected: "integer", found: "string" }
            if section == "App.\"my key\"" && key == "port"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "expected integer at App.\"my key\".port but found string"
    );
    Ok(())
}

#[tokio::test]
async fn array_elements_are_converted_depth_first() -> Result<()> {
    // The first element misses a required key and the second is not a table (L5).
    let error = checked_error("[App]\nupstream = [{ url = \"a\" }, \"b\"]").await?;
    assert!(
        matches!(&error, ConferError::MissingKey { section, key }
            if section == "App.upstream[0]" && key == "weight"),
        "unexpected error: {error:?}"
    );
    Ok(())
}

static TOKENS: AtomicU32 = AtomicU32::new(0);

fn next_token() -> u32 {
    TOKENS.fetch_add(1, Ordering::SeqCst) + 1
}

#[derive(Debug, ConferTable)]
struct Session {
    id: String,
    #[confer(ignore = next_token())]
    token: u32,
    #[confer(ignore)]
    hits: u32,
}

#[derive(Debug, ConferModule)]
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
    let store = Confer::from_string("[Sessions]\nport = 1\n[Sessions.main]\nid = \"a\"")?;
    let module = Sessions::from_confer(store.clone()).await?;
    {
        let mut guard = module.write().await;
        assert_eq!(guard.local, 100);
        assert_eq!(guard.main.hits, 0);
        assert!(guard.extra.is_empty());
        guard.main.hits = 5;
        guard.local = 7;
    }

    // I1: a successful load builds new table values; I2: module-level ignored fields stay.
    store
        .load_str(
            "[Sessions]\nport = 2\n[Sessions.main]\nid = \"b\"\n[[Sessions.extra]]\nid = \"x\"",
        )
        .await?;
    let before = TOKENS.load(Ordering::SeqCst);
    Sessions::load(&module, store.clone()).await?;
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
    store
        .load_str("[Sessions]\nport = 70000\n[Sessions.main]\nid = \"c\"")
        .await?;
    let token = module.read().await.main.token;
    let before = TOKENS.load(Ordering::SeqCst);
    let error = error_of(Sessions::load(&module, store.clone()).await);
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

#[derive(Debug, ConferTable)]
struct Counter {
    total: u64,
}

#[derive(Debug, ConferModule)]
#[confer(section = "Stats")]
struct Stats {
    label: String,
    #[confer(table)]
    counters: Vec<Counter>,
}

#[tokio::test]
async fn nested_save_errors_leave_the_store_unchanged() -> Result<()> {
    let store = Confer::from_string("[Stats]\nlabel = \"a\"\n[[Stats.counters]]\ntotal = 1")?;
    let module = Stats::from_confer(store.clone()).await?;
    {
        let mut guard = module.write().await;
        guard.label = "b".into();
        guard.counters.push(Counter { total: u64::MAX });
    }

    let before = store.get_section_table("Stats").await;
    let error = error_of(Stats::save(&module, store.clone()).await);
    assert!(
        matches!(&error, ConferError::ValueParse { section, key, .. }
            if section == "Stats.counters[1]" && key == "total"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        error.to_string(),
        format!(
            "invalid value at Stats.counters[1].total: value `{}` out of range for TOML integer",
            u64::MAX
        )
    );
    assert_eq!(store.get_section_table("Stats").await, before);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, ConferTable)]
struct TreeNode {
    name: String,
    #[confer(table)]
    children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, ConferModule)]
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
    let store = Confer::from_string("[Tree.root]\nname = \"root\"")?;
    let module = Tree::from_confer(store.clone()).await?;
    assert_eq!(module.read().await.root, node("root", vec![]));

    let expected = node(
        "root",
        vec![
            node("a", vec![node("a1", vec![]), node("a2", vec![])]),
            node("b", vec![]),
        ],
    );
    module.write().await.root = expected.clone();
    Tree::save(&module, store.clone()).await?;
    assert_eq!(reloaded::<Tree>(&store).await?.root, expected);

    let error = error_of(
        Tree::from_confer(Confer::from_string(
            "[Tree.root]\nname = \"root\"\n[[Tree.root.children]]\nname = \"a\"\n[[Tree.root.children.children]]\nname = 5",
        )?)
        .await,
    );
    assert_eq!(
        error.to_string(),
        "expected string at Tree.root.children[0].children[0].name but found integer"
    );
    Ok(())
}

/// Neither `Default` nor `Clone` is required by either derive.
#[derive(ConferTable)]
struct Bare {
    id: String,
    #[confer(table)]
    window: Option<Window>,
}

#[derive(ConferModule)]
#[confer(section = "Bare")]
struct BareHolder {
    #[confer(table)]
    one: Bare,
    #[confer(table)]
    many: Vec<Bare>,
}

#[tokio::test]
async fn tables_need_neither_default_nor_clone() -> Result<()> {
    let store = Confer::from_string(
        "[Bare.one]\nid = \"one\"\n[Bare.one.window]\nseconds = 5\n[[Bare.many]]\nid = \"m\"",
    )?;
    let module = BareHolder::from_confer(store.clone()).await?;
    let saved = Confer::new();
    BareHolder::save(&module, saved.clone()).await?;

    let module = BareHolder::from_confer(Confer::from_string(&saved.save_str().await?)?).await?;
    let guard = module.read().await;
    assert_eq!(guard.one.id, "one");
    assert_eq!(guard.one.window, Some(Window { seconds: 5 }));
    assert_eq!(guard.many.len(), 1);
    assert_eq!(guard.many[0].id, "m");
    assert_eq!(guard.many[0].window, None);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, ConferModule, ConferTable)]
#[confer(section = "Database")]
struct Database {
    url: String,
    #[confer(default = 4)]
    pool: u32,
}

#[derive(Debug, Clone, PartialEq, ConferModule, ConferTable)]
struct ConferCache {
    #[confer(default = 64)]
    size: u32,
}

#[derive(Debug, Clone, PartialEq, ConferModule)]
#[confer(section = "Services")]
struct Services {
    #[confer(table)]
    primary: Database,
    #[confer(table)]
    replicas: Vec<Database>,
    #[confer(table)]
    cache: Option<ConferCache>,
}

#[tokio::test]
async fn one_struct_can_derive_both_traits() -> Result<()> {
    let store = Confer::from_string(
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

    let database = Database::from_confer(store.clone()).await?;
    assert_eq!(
        *database.read().await,
        Database {
            url: "top".into(),
            pool: 4,
        }
    );
    let cache = ConferCache::from_confer(store.clone()).await?;
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
        cache: Some(ConferCache { size: 32 }),
    };
    let services = Services::from_confer(store.clone()).await?;
    assert_eq!(*services.read().await, expected);

    let saved = Confer::new();
    Services::save(&services, saved.clone()).await?;
    assert_eq!(saved.list_sections().await, ["Services"]);
    assert_eq!(reloaded::<Services>(&saved).await?, expected);
    Ok(())
}

/// Stored as `{ address = "host:port" }`.
#[derive(Debug, Clone, PartialEq)]
struct Address {
    host: String,
    port: u16,
}

impl ConferTable for Address {
    fn from_table(path: &str, mut table: Table) -> Result<Self> {
        let raw = match table.remove("address") {
            Some(Value::String(raw)) => raw,
            Some(other) => {
                return Err(ConferError::type_mismatch(
                    path,
                    "address",
                    "string",
                    other.type_str(),
                ));
            }
            None => return Err(ConferError::missing_key(path, "address")),
        };
        let (host, port) = raw
            .rsplit_once(':')
            .ok_or_else(|| ConferError::value_parse(path, "address", "expected host:port"))?;
        let port = port
            .parse()
            .map_err(|_| ConferError::value_parse(path, "address", "invalid port"))?;
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

#[derive(Debug, Clone, PartialEq, ConferModule)]
#[confer(section = "Net")]
struct Net {
    #[confer(table)]
    listen: Address,
    #[confer(table)]
    peers: Vec<Address>,
}

#[tokio::test]
async fn manual_implementations_can_be_table_fields() -> Result<()> {
    let store = Confer::from_string(
        "[Net.listen]\naddress = \"0.0.0.0:80\"\n[[Net.peers]]\naddress = \"a:1\"\n[[Net.peers]]\naddress = \"b:2\"",
    )?;
    let module = Net::from_confer(store.clone()).await?;
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

    Net::save(&module, store.clone()).await?;
    assert_eq!(reloaded::<Net>(&store).await?, expected);

    store
        .load_str("[Net.listen]\naddress = \"0.0.0.0:80\"\n[[Net.peers]]\naddress = \"b\"")
        .await?;
    let error = error_of(Net::load(&module, store.clone()).await);
    assert_eq!(
        error.to_string(),
        "invalid value at Net.peers[0].address: expected host:port"
    );
    Ok(())
}

mod hygiene {
    use conferencier::{Confer, ConferModule, ConferTable, Result};

    // A local `toml` module must not shadow the TOML types used by generated code.
    mod toml {}

    /// Field names matching identifiers used inside the generated code.
    #[derive(Debug, Clone, PartialEq, ConferTable)]
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

    #[derive(Debug, Clone, PartialEq, ConferModule)]
    #[confer(section = "Hygiene")]
    struct HygieneModule {
        path: String,
        table: i64,
        store: String,
        module: String,
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
        let store = Confer::from_string(
            r#"
[Hygiene]
path = "p"
table = 1
store = "s"
module = "m"
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
        let module = HygieneModule::from_confer(store.clone()).await?;
        let loaded = module.read().await.clone();
        assert_eq!(loaded.path, "p");
        assert_eq!(loaded.names.table, 3);
        assert_eq!(loaded.names.index[0].key, 8);
        assert_eq!(loaded.array[0].key, 6);

        HygieneModule::save(&module, store.clone()).await?;
        let text = store.save_str().await?;
        let reloaded = HygieneModule::from_confer(Confer::from_string(&text)?).await?;
        assert_eq!(*reloaded.read().await, loaded);
        Ok(())
    }
}
