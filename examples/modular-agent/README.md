# Modular agent

A small console application configured with [conferencier](../../README.md). It is made of independent components: each component owns its configuration section, defaults, validation, runtime state and commands. The host knows which components are registered, but does not know their settings.

The example shows:

- **A component owns its configuration.** `heartbeat`, `reminders` and `journal` each live in a separate module and load, validate and save only their own scope from the shared `ConfigStore`.
- **Independent startup.** A disabled component or one with an invalid section is skipped without changing that section; other components continue running.
- **Unowned configuration is preserved.** Saving a scope replaces only its section. A component disabled with `--without` can be enabled again later with its previous settings.
- **Components communicate without depending on each other.** `heartbeat` and `reminders` publish events; `journal` subscribes to them through a bounded broadcast channel.
- **One serialized writer.** Components request persistence after saving their scope to the store. A single background task writes the complete store to the file, avoiding overlapping file saves.

It is a separate, unpublished workspace package, so its dependencies do not become dependencies of the library.

## Running

From anywhere in the repository:

```text
cargo run -p modular-agent
```

By default, the application uses `examples/modular-agent/agent.toml`, independent of the working directory. If the file does not exist, each component adds its defaults and the application writes the resulting configuration.

To select another file, pass its path:

```text
cargo run -p modular-agent -- --config path/to/agent.toml
```

A path after `--config` is resolved from the working directory. Disable one or more components with repeated `--without` arguments:

```text
cargo run -p modular-agent -- --without journal --without reminders
```

The disabled components' sections remain untouched. Unknown component names and invalid command-line arguments are errors.

## Commands

The host provides:

| Command | Effect |
|---------|--------|
| `components` | List running components. |
| `config` | Print the complete configuration currently in the store. |
| `help` | List host and component commands. |
| `quit` | Stop workers and save the final configuration. Ctrl+C and end of input also stop the application. |

Component commands use the component name as a prefix:

| Command | Effect |
|---------|--------|
| `heartbeat status` | Show whether the heartbeat is enabled, its interval and number of messages sent. |
| `heartbeat interval <seconds>` | Change and save the interval; it must be positive. |
| `heartbeat message <text>` | Change and save the message. |
| `heartbeat pause` / `heartbeat resume` | Disable or enable heartbeat messages and save the setting. |
| `reminders list` | List reminders and their remaining time. |
| `reminders add <name> <seconds> <message>` | Add a uniquely named reminder and save it. Its timer starts now. |
| `reminders remove <name>` | Remove and save a reminder. |
| `journal show` | Show recent events held in memory. |
| `journal file <path>` | Append future events to a file. The path is relative to the working directory and must not resolve to the configuration file. |
| `journal file off` | Stop writing events to a file; does not delete an existing file. |

For example, from the repository root:

```text
reminders add tea 10 Tea is ready
heartbeat interval 3
journal file examples/modular-agent/journal.log
```

After a few seconds, `journal show` lists heartbeat and reminder events. The generated `agent.toml` and the `examples/modular-agent/journal.log` path used above are ignored by Git.

## Configuration

The first run writes defaults for all enabled components:

```toml
[Heartbeat]
enabled = true
interval_secs = 10
message = "still here"

[Reminders]
items = []

[Journal]
keep = 100
```

An application command can add reminder tables or `[Journal.file]`. For example:

```toml
[Heartbeat]
enabled = true
interval_secs = 10
message = "still here"

[[Reminders.items]]
name = "tea"
after_secs = 60
message = "Tea is ready"

[Journal]
keep = 100

[Journal.file]
path = "examples/modular-agent/journal.log"
```

`after_secs` is measured from application start for reminders loaded from the file, and from the `add` command for new reminders. It is not a wall-clock schedule.

| Section | Key | Default | Meaning |
|---------|-----|---------|---------|
| `Heartbeat` | `enabled` | `true` | Whether the periodic message is printed and published. |
| `Heartbeat` | `interval_secs` | `10` | Seconds between messages; must be greater than zero. |
| `Heartbeat` | `message` | `"still here"` | Text printed and published on each tick. |
| `Reminders` | `items` | `[]` | Array of tables with unique `name`, positive `after_secs` and nonempty `message`. |
| `Journal` | `keep` | `100` | Maximum number of recent events retained in memory; must be greater than zero. |
| `Journal.file` | `path` | absent | Optional append-only event output path, relative to the working directory. |

`Reminders` and its item tables, `Journal` and `Journal.file` reject unknown keys. `Heartbeat` is lenient when loading, but saving it replaces the whole section, so unrecognized keys in `[Heartbeat]` are removed by a later heartbeat command.

## Architecture

```text
agent.toml ──► ConfigStore ──┬──► [Heartbeat] ──► heartbeat worker
                             ├──► [Reminders] ──► reminders worker
                             └──► [Journal] ────► journal worker

heartbeat and reminders ── events ──► bounded broadcast channel ──► journal

component command ──► save its scope to ConfigStore ──► persistence request
                                                     └──► single persister ──► agent.toml
```

Each component's `start` method creates its typed scope from the shared store, validates its own rules and starts its worker. When its section is missing, it saves its defaults and requests persistence. The host dispatches commands by component name and does not inspect the component's settings.

Disabling a component skips its `start` method. A component that fails to load or validate never saves its scope. In both cases its existing section stays in the store and is written unchanged when another component saves or the application exits. Sections not owned by any registered component are also preserved.

The persister serializes complete-store writes. A failed write is reported and the in-memory store remains available; another save request or the final shutdown flush retries it. Runtime state, including heartbeat ticks, fired reminders and recent journal events, is not written to TOML.

| File | Responsibility |
|------|----------------|
| [`src/main.rs`](src/main.rs) | Registry, startup, console dispatch, shutdown and tests for disabled or invalid components. |
| [`src/component.rs`](src/component.rs) | Component interfaces and shared context for configuration, events, persistence and shutdown. |
| [`src/persist.rs`](src/persist.rs) | Single task that serializes requests to write the complete store. |
| [`src/events.rs`](src/events.rs) | Event type shared by publishers and subscribers. |
| [`src/components/heartbeat.rs`](src/components/heartbeat.rs) | Heartbeat settings, commands and periodic worker. |
| [`src/components/reminders.rs`](src/components/reminders.rs) | Reminder tables, commands, validation and timer worker. |
| [`src/components/journal.rs`](src/components/journal.rs) | Event subscriber, in-memory history and optional file output. |

## Limitations

- The first whitespace separates a command from its arguments; internal spaces in messages and journal paths are preserved. A reminder name cannot contain spaces.
- The journal keeps only the configured number of recent events in memory. Its file output appends every received event and does not rotate or truncate the file.
- The broadcast channel has a bounded capacity. If the journal cannot keep up, it reports how many events were missed.
- If writing or flushing the journal file fails, the error is printed and the event remains in the in-memory history.
- Relative paths for `--config` and `Journal.file.path` use the working directory. Parent directories are not created.
- Every save, including the final flush on exit, rewrites the whole configuration file from the store. Comments and formatting in a hand-edited `agent.toml` are not preserved.
- This is an illustrative agent, not a production scheduler or logging service.
