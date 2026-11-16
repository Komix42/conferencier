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

## Reloading configuration

Conferencier never reloads anything on its own. To apply an edited configuration file, load it into the store and then reload every scope that uses it:

```rust
use std::path::Path;

use conferencier::{ConfigScope, ConfigStore, SharedScope};

async fn reload<S: ConfigScope>(
    store: &ConfigStore,
    scope: &SharedScope<S>,
    path: &Path,
) -> conferencier::Result<()> {
    store.load_file(path).await?;
    S::load(scope, store).await
}
```

- `ConfigStore::load_file` and `ConfigStore::load_str` replace the whole store, including sections that no scope owns and values saved into the store but not yet written to a file. If the new document cannot be read or parsed, the store keeps its previous content.
- `load` updates a scope only if all its loaded fields convert successfully; otherwise it returns the error and the scope keeps its previous values. Ignored fields of the scope keep their values in both cases.
- A scope that is not reloaded keeps its previous values, even though the store has changed.
- Reloading several scopes is not one transaction: if a later scope fails to load, the store and scopes already reloaded keep their new values.

## Configuration and runtime state

A scope can also hold runtime state that does not belong in the configuration file, such as counters, caches or client handles. Mark such fields with `#[confer(ignore)]` or `#[confer(ignore = <expr>)]`: `from_store` initializes them, later `load` calls keep their values, and `save` never writes them. Tasks then share the settings and the state through one `SharedScope<T>` handle (`Arc<RwLock<T>>`); clone the handle to pass it to another task.

```rust
use conferencier::{ConfigStore, ConfigScope};

#[derive(ConfigScope)]
#[confer(section = "Server")]
struct Server {
    port: u16,
    #[confer(ignore)]
    requests_served: u64,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_str("[Server]\nport = 8080\n")?;
    let server = Server::from_store(&store)?;

    let handle = server.clone();
    tokio::spawn(async move {
        handle.write().await.requests_served += 1;
    })
    .await
    .expect("task panicked");

    // A reload updates the port and keeps the counter.
    store.load_str("[Server]\nport = 9090\n")?;
    Server::load(&server, &store).await?;

    let state = server.read().await;
    assert_eq!(state.port, 9090);
    assert_eq!(state.requests_served, 1);
    Ok(())
}
```

Keep state that must survive reloads directly in the scope: ignored fields inside `ConfigTable` values are initialized again on every load (see [Nested tables](crate::guide::tables)).

The handle's Tokio `RwLock` is not reentrant, so drop every guard of a scope before calling its `load` or `save` in the same task. `load` locks the scope for writing and would wait for a held guard forever. `save` locks it for reading, which waits forever behind a held write guard and, because Tokio's lock is fair, also behind a held read guard while another task is waiting to write. Copy what you need out of a short-lived guard instead, for example `let port = server.read().await.port;`.
