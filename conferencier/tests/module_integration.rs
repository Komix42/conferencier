use conferencier::{confer_module::ConferModule, Confer, Result};
use toml::value::Datetime;

#[derive(conferencier::ConferModule)]
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
    #[confer(ignore, init = "Vec::new()")]
    cache: Vec<u8>,
}

#[tokio::test]
async fn module_from_confer_loads_defaults_and_values() -> Result<()> {
    let store = Confer::from_string(
        r#"[Srv]
extra = 7
p = 8080
notes = "temp"
"#,
    )?;

    let module = Server::from_confer(store.clone()).await?;

    {
        let guard = module.read().await;
        assert_eq!(guard.port, 8080);
        assert_eq!(guard.host, "0.0.0.0");
        assert_eq!(guard.retries, 3);
        assert_eq!(guard.features, vec!["alpha".to_string(), "beta".to_string()]);
        assert!(guard
            .started_at
            .to_string()
            .starts_with("2024-01-01T00:00:00Z"));
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

#[derive(conferencier::ConferModule)]
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
    let store = Confer::from_string(
        r#"[Nested]
counters = [10, 20]
"#,
    )?;

    let module = NestedModule::from_confer(store.clone()).await?;

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
