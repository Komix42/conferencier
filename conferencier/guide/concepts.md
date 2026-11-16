Conferencier keeps configuration in two kinds of values, which are synchronized only when you ask for it.

- A [`ConfigStore`](crate::ConfigStore) holds one TOML document in memory. It is a cheap-to-clone handle whose clones share the document. Its in-memory methods are synchronous; only file I/O is async.
- A **scope** is a struct implementing [`ConfigScope`](crate::ConfigScope), usually through `#[derive(ConfigScope)]`. Its loaded fields own one top-level TOML section, and its ignored fields hold runtime state that never reaches the file. Async tasks share a scope through [`SharedScope<T>`](crate::SharedScope).
- A **table** is a plain struct implementing [`ConfigTable`](crate::ConfigTable). It maps a nested table or an array of tables inside a scope.

Nothing runs in the background. Data moves only on these calls:

| Call | Direction |
|---|---|
| [`ConfigStore::from_file`](crate::ConfigStore::from_file), [`ConfigStore::load_file`](crate::ConfigStore::load_file) | file → store |
| [`ConfigScope::from_store`](crate::ConfigScope::from_store) | store → new scope |
| [`ConfigScope::load`](crate::ConfigScope::load) | store → existing scope |
| [`ConfigScope::save`](crate::ConfigScope::save) | scope → store |
| [`ConfigStore::save_file`](crate::ConfigStore::save_file) | store → file |

A change in the store is therefore invisible to a scope until the scope is loaded again:

```rust
use conferencier::{ConfigScope, ConfigStore};

#[derive(ConfigScope)]
#[confer(section = "App")]
struct App {
    #[confer(default = 8080)]
    port: i64,
}

# #[tokio::main(flavor = "current_thread")]
# async fn main() -> conferencier::Result<()> {
let store = ConfigStore::from_str("[App]\nport = 9090\n")?;
let app = App::from_store(&store)?;

store.set_integer("App", "port", 7070)?;
assert_eq!(app.read().await.port, 9090);

App::load(&app, &store).await?;
assert_eq!(app.read().await.port, 7070);
# Ok(())
# }
```
