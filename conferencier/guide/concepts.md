Conferencier keeps the store and scopes separate; data moves between them only when you ask for it. Tables represent nested values within a scope.

- A [`ConfigStore`](crate::ConfigStore) holds one TOML document in memory. It is a cheap-to-clone handle whose clones share the document. Its in-memory methods are synchronous; only file I/O is async.
- A **scope** is a struct implementing [`ConfigScope`](crate::ConfigScope), usually through `#[derive(ConfigScope)]`. A derived scope maps its loaded fields to one top-level TOML section; its ignored fields hold runtime state that never reaches the file. Async tasks share a scope through [`SharedScope<T>`](crate::SharedScope).
- A **table** is a plain struct implementing [`ConfigTable`](crate::ConfigTable). It maps a nested table or an array of tables inside a scope.

Nothing runs in the background. The main operations move data explicitly:

| Call | Direction |
|---|---|
| [`ConfigStore::from_file`](crate::ConfigStore::from_file), [`ConfigStore::load_file`](crate::ConfigStore::load_file) | file → store |
| [`ConfigStore::from_str`](crate::ConfigStore::from_str), [`ConfigStore::load_str`](crate::ConfigStore::load_str) | TOML string → store |
| [`ConfigScope::from_store`](crate::ConfigScope::from_store) | store → new scope |
| [`ConfigScope::load`](crate::ConfigScope::load) | store → existing scope |
| [`ConfigScope::save`](crate::ConfigScope::save) | scope → store |
| [`ConfigStore::save_file`](crate::ConfigStore::save_file) | store → file |
| [`ConfigStore::save_str`](crate::ConfigStore::save_str) | store → TOML string |

Store getters and setters also read and change the in-memory document directly.

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

## Reading a file before the runtime starts

Conferencier does not start a Tokio runtime or choose its flavor; applications enable the Tokio features they need, such as `macros` and `rt-multi-thread`. The file methods work on any Tokio runtime, including a current-thread runtime. `ConfigStore::from_file` and `load_file` panic if polled without a runtime when they reach the Tokio file read. `save_file` serializes first, so a serialization error is returned without a runtime; a successful serialization requires a runtime when file I/O starts.

All in-memory work, including [`ConfigScope::from_store`](crate::ConfigScope::from_store), runs without a Tokio runtime. To read the configuration before the runtime starts, combine [`std::fs::read_to_string`] with `ConfigStore::from_str`. The standard library's error does not carry the path; attach it with [`Error::io_error_with_operation`](crate::Error::io_error_with_operation) if needed:

```rust,no_run
use conferencier::{ConfigStore, Error, IoOperation};

# fn main() -> conferencier::Result<()> {
let path = std::path::Path::new("config.toml");
let source = std::fs::read_to_string(path).map_err(|source| {
    Error::io_error_with_operation(Some(path.to_path_buf()), IoOperation::Read, source)
})?;
let store = ConfigStore::from_str(&source)?;
# let _ = store;
# Ok(())
# }
```

## Nesting limits

Parsing uses the default recursion limit of `toml` (currently 80 levels with `toml` 1.2); that exact limit is not part of conferencier's API. Deeper arrays, inline tables, dotted keys and table headers return a parse error. Conferencier does not enable `toml/unbounded`, but Cargo features are additive, so another dependency can remove this limit. Applications loading untrusted TOML should check the feature graph with `cargo tree -e features -i toml`.

Values created in code and inserted with [`ConfigStore::set_value`](crate::ConfigStore::set_value) or [`ConfigStore::set_section`](crate::ConfigStore::set_section) are not subject to the parser's limit. Very deeply nested values can overflow the stack when serialized, cloned or dropped; a stack overflow aborts the process.

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

## Locking

Scopes and the store use different locks, chosen for their different uses.

A scope's `SharedScope<T>` handle uses Tokio's `RwLock`, because tasks hold its guards while they work with the configuration and runtime state, possibly across an `.await`. The lock is not reentrant, so drop every guard of a scope before calling its `load` or `save` in the same task. For a derived scope with loaded fields, `load` locks the scope for writing and would wait for a held guard forever. `save` locks it for reading, which waits forever behind a held write guard and, because Tokio's lock is fair, also behind a held read guard while another task is waiting to write. Copy what you need out of a short-lived guard instead, for example `let port = server.read().await.port;`.

The store uses a synchronous [`std::sync::RwLock`] that it holds only inside a single method call, so its in-memory methods need no runtime and cannot deadlock. This assumes relatively small documents and low contention, in line with [Tokio's guidance on synchronous locks](https://tokio.rs/tokio/tutorial/shared-state#on-using-stdsyncmutex-and-tokiosyncmutex):

- Work under the lock is not constant-time: cloning a section and serializing the document scale with their contents.
- A contended acquisition blocks the worker thread instead of yielding to other tasks.
- Unlike Tokio's `RwLock`, the standard lock gives no reader/writer priority guarantee; its policy depends on the platform.

Should real workloads exceed these assumptions, the implementation and possibly the API have to be reevaluated based on measurements. Switching to an async lock would not be a transparent change: acquiring it has to be awaited, and it would not offload the CPU work of serialization.
