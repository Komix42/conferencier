# Uptime monitor

A small but complete application configured with [conferencier](../../README.md). A background worker periodically checks whether TCP services are reachable, an HTTP server shows the results, and console commands reload, change and export the configuration while the application runs.

The example shows how conferencier fits into a Tokio application with several components:

- **One store, one scope per component.** The HTTP server owns the `[Http]` section and the worker owns `[Monitor]`; both scopes come from one `ConfigStore` loaded from one file.
- **Nested tables and strict keys.** Targets are an array of tables (`[[Monitor.targets]]`), and a misspelled key such as `adress` is an error instead of being silently ignored.
- **Runtime state next to configuration.** The check results and the request counter live in ignored fields of the scopes. They are never loaded or saved, and they survive a reload.
- **Validation in the application.** Conferencier converts values to the field types; rules such as a nonzero interval or unique target names are checked by the application.
- **Explicit reload that is all or nothing.** A changed file is applied only if all of it is valid; otherwise the application keeps running with its previous configuration.
- **Export of the effective configuration.** Changes made at runtime are saved to a separate file, leaving the hand-written one untouched.

It is a separate, unpublished workspace package, so its dependencies, such as Axum, do not become dependencies of the library.

## Running

From anywhere in the repository:

```text
cargo run -p uptime-monitor
```

The application loads [`config/monitor.toml`](config/monitor.toml), prints where it serves its pages and waits for commands:

```text
Loaded configuration from .../examples/uptime-monitor/config/monitor.toml
Status at http://127.0.0.1:8080/ and the configuration in use at http://127.0.0.1:8080/config
Commands: reload, pause, resume, export, help, quit (or Ctrl+C)
self is up
local-database is down
```

The configuration directory is found relative to the package, not to the working directory. If port 8080 is taken, change `Http.port` in `config/monitor.toml`, and the address of the `self` target with it.

### HTTP pages

- **`GET /`** lists the targets with their latest results:

  ```text
  Checking every 10s.

  self                 127.0.0.1:8080           up (0.4 ms) (checked 3s ago)
  local-database       127.0.0.1:5432           down: no connection within 2s (checked 1s ago)

  1 requests served.
  ```

- **`GET /config`** shows the configuration the application currently uses, as TOML. It includes runtime changes such as `pause`, but not the runtime state.

### Console commands

| Command  | Effect |
|----------|--------|
| `reload` | Reads the configuration file again and applies it if it is valid; otherwise prints the error and keeps the previous configuration. |
| `pause`  | Stops checking after the current round. The change exists only at runtime. |
| `resume` | Starts checking again, immediately. |
| `export` | Saves the configuration in use, including runtime changes, to `config/effective.toml`. |
| `help`   | Lists the commands. |
| `quit`   | Stops the worker and the server gracefully and exits. Ctrl+C and the end of input do the same. |

### A walk through

1. Start the application and open <http://127.0.0.1:8080/>.
2. Edit `config/monitor.toml`: set `interval_secs = 3` and uncomment the `crates.io` target. Type `reload`. The new target is checked immediately and every three seconds after that; the request counter on the page keeps counting.
3. Misspell a key, for example `intervl_secs = 3`, and type `reload`. The application prints `error: unknown key Monitor.intervl_secs` and keeps checking with the previous settings. Fix the key again.
4. Type `pause`. The page says that monitoring is paused, and `/config` shows `enabled = false`.
5. Type `export`, then `quit`, and start the application again. It now loads `config/effective.toml`, so it starts paused. Type `resume`.
6. Delete `config/effective.toml` to return to the hand-written configuration. The file is ignored by Git.

## Configuration

The application loads `config/effective.toml` if it exists, otherwise `config/monitor.toml`. The same rule applies at startup and on `reload`.

| Key | Default | Meaning |
|-----|---------|---------|
| `Http.address` | `"127.0.0.1"` | Address the HTTP server listens on. |
| `Http.port` | `8080` | Port the HTTP server listens on. |
| `Monitor.enabled` | `true` | Whether the worker checks the targets. |
| `Monitor.interval_secs` | `30` | Seconds between two rounds of checks; must be greater than zero. |
| `Monitor.timeout_secs` | `5` | Seconds to wait for a connection before a target counts as down; must be greater than zero. |
| `Monitor.targets` | required | Array of tables with `name` (unique, not empty) and `address` (`host:port`); may be empty (`targets = []`). |

`[Monitor]` and its targets reject unknown keys; `[Http]` ignores them. A missing key takes its default, except for `targets` and the keys of a target, which have none. The bundled `monitor.toml` sets shorter intervals than the defaults so that changes show up quickly.

## Architecture

```text
config/effective.toml (if present) or config/monitor.toml
        │  start, reload                    ▲  export
        ▼                                   │
   ConfigStore ─┬─ [Http]    ──► SharedScope<HttpConfig>     + requests_served
                └─ [Monitor] ──► SharedScope<MonitorConfig>  + results

   HTTP server   reads both scopes, counts requests in HttpConfig
   worker        reads its settings from MonitorConfig, writes results into it
   console       reload, pause, resume, export, quit
```

The `+` fields are runtime state, not configuration.

| File | Responsibility |
|------|----------------|
| [`src/config.rs`](src/config.rs) | The scopes `HttpConfig` and `MonitorConfig`, the table `Target`, application validation and the all-or-nothing `reload`, with tests. |
| [`src/monitor.rs`](src/monitor.rs) | The worker: checks all targets concurrently in rounds and records the results. |
| [`src/http.rs`](src/http.rs) | The Axum router with the two pages. |
| [`src/main.rs`](src/main.rs) | Startup, the console commands and graceful shutdown. |

### Startup

`main.rs` loads the store from the file, creates both scopes with `from_store` and validates `[Monitor]`; any error ends the application with a message such as `error: unknown key Monitor.targets[1].adress`. It then binds the listener, so a taken port is reported before anything else starts, spawns the worker and the server and reads console commands. The scopes are `Arc`s, so each component gets a clone of the scopes it needs instead of the whole configuration.

### Runtime state

Fields marked `#[confer(ignore = ...)]` belong to the scope but not to the file: `HttpConfig::requests_served` is incremented by the request handlers, `MonitorConfig::results` is written by the worker and read by the status page. `load` never changes them, so a reload keeps the counter and the results. A result remembers the address it was checked at, so the page and the worker can tell that it is stale after the address of a target changed; results of removed targets are dropped.

### Locking

The scopes are `tokio::sync::RwLock`s, and the components hold them only briefly:

- The worker copies its settings and the targets under a read lock and releases it before any network I/O. A slow check therefore never delays a page or a reload; a reload that happens during a round takes effect in the next round.
- After a round, the worker takes the write lock once to record all results, skipping targets that a reload removed or changed in the meantime.
- Handlers take each lock only to read or update a few fields.
- `load` takes the scope's write lock, so no guard of the same scope may be held while awaiting it.

### Reload and export

`reload` in `config.rs` first loads the new file into a separate `ConfigStore`, creates both scopes from it with `from_store`, which checks types and unknown keys, and validates the result. Only then does it replace the application's store with `load_str` and update the scopes with `load`. Because the candidate has already converted successfully, these cannot fail halfway, so any error leaves the running configuration as it was. After a successful reload the worker is woken to check the new targets at once.

`pause` and `resume` change `enabled` in the scope only. `export` writes such runtime changes back with `save`, which replaces each scope's section of the store, and then saves the store with `save_file`. Exporting to a separate file keeps the comments of `monitor.toml`, because conferencier does not preserve comments or layout when saving.

## Limitations

The example keeps to what it demonstrates; a production service would need more.

- **The listener is not reconfigured.** `Http.address` and `Http.port` take effect only after a restart; `reload` prints a note when they differ from the address in use.
- **The file is the source of truth.** `reload` replaces runtime changes: after `pause`, a reload sets `enabled` back to the file's value unless the change was exported.
- **Reload is explicit.** Conferencier does not watch files, so the application reloads only on the `reload` command.
- **A check only opens a TCP connection.** It does not send a request or check a response, which keeps HTTP client dependencies out of the example. Axum provides the server side with a small router and graceful shutdown.
- **Connection timing depends on the platform.** On Windows, a refused connection to a local port is retried for about two seconds, so with the bundled `timeout_secs = 2` the `local-database` target reports a timeout instead of a refused connection.
- **The results are not persisted.** They are lost when the application exits.
