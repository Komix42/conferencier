use conferencier::toml::{Table, Value};
use conferencier::{ConfigStore, Error, Result};

#[tokio::test]
async fn load_and_save_roundtrip() -> Result<()> {
    let store = ConfigStore::from_string("[App]\nname = \"demo\"\n")?;
    assert_eq!(store.get_string("App", "name").await?, "demo");

    store.set_integer("App", "port", 8080).await?;
    store
        .set_string_vec("App", "langs", vec!["en".into(), "de".into()])
        .await?;

    let output = store.save_str().await?;
    assert!(output.contains("name = \"demo\""));
    assert!(output.contains("port = 8080"));
    assert!(output.contains("langs = ["));
    Ok(())
}

#[tokio::test]
async fn legacy_toml_values_and_raw_api_roundtrip() -> Result<()> {
    let store = ConfigStore::from_string(
        r#"[App]
name = "demo"
minimum = -9223372036854775808
maximum = 9223372036854775807
ratio = 1.25
enabled = true
started = 2024-01-01T00:00:00Z
date = 2024-01-01
time = 07:30:00.125
values = [1, 2.5]
dates = [2024-01-01, 2024-12-31]
empty = []
nested = { host = "localhost", port = 8080 }
"#,
    )?;
    store
        .set_value(
            "App",
            "raw",
            Value::Array(vec![Value::String("custom".into()), Value::Integer(42)]),
        )
        .await?;

    let before: Table = store.get_section_table("App").await.unwrap();
    let serialized = store.save_str().await?;
    let restored = ConfigStore::from_string(&serialized)?;

    assert_eq!(restored.get_section_table("App").await, Some(before));
    Ok(())
}

#[tokio::test]
async fn toml_1_1_datetimes_preserve_omitted_components() -> Result<()> {
    let store = ConfigStore::from_string(
        r#"[Schedule]
minute = 07:30
second = 07:30:00
fraction = 07:30:00.125
timestamp = 2024-01-01T07:30Z
text = "08:45"
times = ["07:30", 08:45:00.125]
"#,
    )?;
    let cases = [
        ("minute", "07:30", None, None),
        ("second", "07:30:00", Some(0), None),
        ("fraction", "07:30:00.125", Some(0), Some(125_000_000)),
        ("timestamp", "2024-01-01T07:30Z", None, None),
    ];

    for (key, expected, second, nanosecond) in cases {
        let value = store.get_datetime("Schedule", key).await?;
        let time = value.time.as_ref().unwrap();
        assert_eq!(time.second, second);
        assert_eq!(time.nanosecond, nanosecond);
        assert_eq!(value.to_string(), expected);
    }
    assert_eq!(
        store.get_datetime("Schedule", "text").await?.to_string(),
        "08:45"
    );
    let times = store.get_datetime_vec("Schedule", "times").await?;
    assert_eq!(
        times.iter().map(ToString::to_string).collect::<Vec<_>>(),
        vec!["07:30", "08:45:00.125"]
    );

    let restored = ConfigStore::from_string(&store.save_str().await?)?;
    assert_eq!(
        restored.get_section_table("Schedule").await,
        store.get_section_table("Schedule").await
    );
    Ok(())
}

#[test]
fn malformed_radix_numbers_return_parse_errors() {
    for literal in ["0x_g", "0b_2", "0o_8"] {
        let source = format!("[App]\nvalue = {literal}\n");
        assert!(
            matches!(ConfigStore::from_string(&source), Err(Error::Parse(_))),
            "expected a parse error for {literal}"
        );
    }
}

#[tokio::test]
async fn concurrent_readers_and_writer() -> Result<()> {
    let store = ConfigStore::new();
    store.add_section("Metrics").await?;

    let writer = store.clone();
    let writer_task = tokio::spawn(async move {
        for value in 0..10 {
            writer
                .set_integer("Metrics", "counter", value)
                .await
                .unwrap();
        }
    });

    let reader = store.clone();
    let reader_task = tokio::spawn(async move {
        let mut last = -1;
        loop {
            match reader.get_integer("Metrics", "counter").await {
                Ok(current) => {
                    if current == 9 {
                        break;
                    }
                    if current > last {
                        last = current;
                    }
                }
                Err(_) => tokio::task::yield_now().await,
            }
        }
    });

    writer_task.await.unwrap();
    reader_task.await.unwrap();
    assert_eq!(store.get_integer("Metrics", "counter").await?, 9);
    Ok(())
}
