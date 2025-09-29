use conferencier::{Confer, Result};

#[tokio::test]
async fn load_and_save_roundtrip() -> Result<()> {
    let store = Confer::from_string("[App]\nname = \"demo\"\n")?;
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
async fn concurrent_readers_and_writer() -> Result<()> {
    let store = Confer::new();
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
