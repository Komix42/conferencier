//! Snapshot-based load and save of derived modules (RFC-0004 R1–R5).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use conferencier::toml::{Table, Value};
use conferencier::{ConfigScope, ConfigStore, Error, Result};

#[derive(ConfigScope)]
#[confer(section = "App")]
struct App {
    name: String,
    port: u16,
    #[confer(ignore = 7)]
    state: u32,
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct RuntimeOnly {
    #[confer(ignore)]
    counter: u32,
}

#[derive(ConfigScope)]
#[confer(section = "Counters")]
struct Counters {
    label: String,
    total: u64,
}

#[derive(ConfigScope)]
#[confer(section = "Pair")]
struct Pair {
    left: i64,
    right: i64,
}

fn error_of<T>(result: Result<T>) -> Error {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error,
    }
}

fn assert_not_a_table(error: &Error, expected_section: &str) {
    assert!(
        matches!(
            error,
            Error::TypeMismatch { path, key, expected: "table", found: "integer" }
                if path.is_empty() && key == expected_section
        ),
        "unexpected error: {error:?}"
    );
}

fn pair_table(version: i64) -> Table {
    let mut table = Table::new();
    table.insert("left".into(), Value::Integer(version));
    table.insert("right".into(), Value::Integer(version));
    table
}

#[tokio::test]
async fn failing_load_leaves_the_module_unchanged() -> Result<()> {
    let store = ConfigStore::from_str("[App]\nname = \"first\"\nport = 80")?;
    let module = App::from_store(&store)?;
    module.write().await.state = 42;

    store.set_string("App", "name", "second".into())?;
    store.set_integer("App", "port", 70_000)?;
    let error = error_of(App::load(&module, &store).await);
    assert_eq!(
        error.to_string(),
        "invalid value at App.port: value out of range for u16"
    );

    let guard = module.read().await;
    assert_eq!(guard.name, "first");
    assert_eq!(guard.port, 80);
    assert_eq!(guard.state, 42);
    Ok(())
}

#[tokio::test]
async fn failing_save_leaves_the_store_unchanged() -> Result<()> {
    let store = ConfigStore::from_str("[Counters]\nlabel = \"old\"\ntotal = 1\nextra = true")?;
    let module = Counters::from_store(&store)?;
    {
        let mut guard = module.write().await;
        guard.label = "new".into();
        guard.total = u64::MAX;
    }
    let before = store.save_str()?;

    let error = error_of(Counters::save(&module, &store).await);
    assert!(matches!(
        &error,
        Error::ValueParse { path, key, .. } if path == "Counters" && key == "total"
    ));
    assert_eq!(
        error.to_string(),
        format!(
            "invalid value at Counters.total: value `{}` out of range for TOML integer",
            u64::MAX
        )
    );
    assert_eq!(store.save_str()?, before);
    Ok(())
}

#[tokio::test]
async fn save_reports_conversion_errors_before_a_non_table_section() -> Result<()> {
    let source = ConfigStore::from_str("[Counters]\nlabel = \"a\"\ntotal = 1")?;
    let module = Counters::from_store(&source)?;
    let store = ConfigStore::from_str("Counters = 5")?;
    let before = store.save_str()?;

    module.write().await.total = u64::MAX;
    let error = error_of(Counters::save(&module, &store).await);
    assert!(matches!(error, Error::ValueParse { .. }), "{error:?}");

    module.write().await.total = 2;
    let error = error_of(Counters::save(&module, &store).await);
    assert_not_a_table(&error, "Counters");
    assert_eq!(store.save_str()?, before);
    Ok(())
}

#[tokio::test]
async fn non_table_sections_are_rejected_even_without_loaded_fields() -> Result<()> {
    let store = ConfigStore::from_str("App = 7")?;
    assert_not_a_table(&error_of(App::from_store(&store)), "App");
    assert_not_a_table(&error_of(RuntimeOnly::from_store(&store)), "App");

    let app = App::from_store(&ConfigStore::from_str("[App]\nname = \"a\"\nport = 1")?)?;
    let runtime = RuntimeOnly::from_store(&ConfigStore::new())?;
    runtime.write().await.counter = 3;
    let before = store.save_str()?;

    assert_not_a_table(&error_of(App::load(&app, &store).await), "App");
    assert_not_a_table(&error_of(RuntimeOnly::load(&runtime, &store).await), "App");
    assert_not_a_table(&error_of(App::save(&app, &store).await), "App");
    assert_not_a_table(&error_of(RuntimeOnly::save(&runtime, &store).await), "App");

    assert_eq!(store.save_str()?, before);
    assert_eq!(app.read().await.name, "a");
    assert_eq!(runtime.read().await.counter, 3);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_loads_observe_whole_section_versions() -> Result<()> {
    let store = ConfigStore::new();
    store.set_section("Pair", pair_table(0))?;
    let module = Pair::from_store(&store)?;
    let done = Arc::new(AtomicBool::new(false));

    let writer = tokio::spawn({
        let (store, done) = (store.clone(), done.clone());
        async move {
            for version in 1..=2_000 {
                store.set_section("Pair", pair_table(version % 2))?;
                tokio::task::yield_now().await;
            }
            done.store(true, Ordering::SeqCst);
            Ok::<_, Error>(())
        }
    });
    let constructor = tokio::spawn({
        let (store, done) = (store.clone(), done.clone());
        async move {
            loop {
                let finished = done.load(Ordering::SeqCst);
                let module = Pair::from_store(&store)?;
                let guard = module.read().await;
                assert_eq!(guard.left, guard.right, "from_store mixed two versions");
                drop(guard);
                if finished {
                    return Ok::<_, Error>(());
                }
                tokio::task::yield_now().await;
            }
        }
    });
    let loader = tokio::spawn({
        let (store, module, done) = (store.clone(), module.clone(), done.clone());
        async move {
            loop {
                let finished = done.load(Ordering::SeqCst);
                Pair::load(&module, &store).await?;
                if finished {
                    return Ok::<_, Error>(());
                }
                tokio::task::yield_now().await;
            }
        }
    });
    let observer = tokio::spawn({
        let (module, done) = (module.clone(), done.clone());
        async move {
            loop {
                let finished = done.load(Ordering::SeqCst);
                let guard = module.read().await;
                assert_eq!(guard.left, guard.right, "load assigned fields separately");
                drop(guard);
                if finished {
                    return;
                }
                tokio::task::yield_now().await;
            }
        }
    });

    writer.await.expect("writer panicked")?;
    constructor.await.expect("constructor panicked")?;
    loader.await.expect("loader panicked")?;
    observer.await.expect("observer panicked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_saves_publish_whole_section_versions() -> Result<()> {
    let store = ConfigStore::new();
    store.set_section("Pair", pair_table(0))?;
    let module = Pair::from_store(&store)?;
    let done = Arc::new(AtomicBool::new(false));

    let writer = tokio::spawn({
        let (store, module, done) = (store.clone(), module.clone(), done.clone());
        async move {
            for version in 1..=2_000 {
                {
                    let mut guard = module.write().await;
                    guard.left = version % 2;
                    guard.right = version % 2;
                }
                Pair::save(&module, &store).await?;
                tokio::task::yield_now().await;
            }
            done.store(true, Ordering::SeqCst);
            Ok::<_, Error>(())
        }
    });
    let reader = tokio::spawn({
        let (store, done) = (store.clone(), done.clone());
        async move {
            loop {
                let finished = done.load(Ordering::SeqCst);
                let table = store
                    .get_section("Pair")
                    .expect("the section is never removed");
                assert_eq!(
                    table.get("left"),
                    table.get("right"),
                    "save mixed two versions"
                );
                if finished {
                    return;
                }
                tokio::task::yield_now().await;
            }
        }
    });

    writer.await.expect("writer panicked")?;
    reader.await.expect("reader panicked");
    Ok(())
}
