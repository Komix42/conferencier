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

By default, the application uses `examples/modular-agent/agent.toml`, independent of the working directory. If the file does not exist, each component adds its defaults and the application writes the resulting configuration. The file is ignored by Git.

To select another file, pass its path:

```text
cargo run -p modular-agent -- --config path/to/agent.toml
```

A relative `--config` path is resolved from the working directory; a relative journal path stored in the configuration is resolved from the configuration file's directory. Disable one or more components with repeated `--without` arguments:

```text
cargo run -p modular-agent -- --without journal --without reminders
```

The disabled components' sections remain untouched. Unknown component names and invalid command-line arguments are errors.

The application prints whether it created or loaded the file and waits for console commands:

```text
Starting with an empty configuration at .../examples/modular-agent/agent.toml
Type `help` for commands.
```

### Console commands

The host provides:

| Command | Effect |
|---------|--------|
| `components` | List running components. |
| `config` | Print the complete configuration currently in the store, including sections of disabled components. |
| `help` | List host commands and running components. |
| `quit` | Stop workers and save the final configuration. Ctrl+C and end of input also stop the application. |

Component commands use the component name as a prefix:

| Command | Effect |
|---------|--------|
| `heartbeat status` | Show whether the heartbeat is enabled, its interval, message and number of messages sent. |
| `heartbeat interval <seconds>` | Change and save the interval; it must be positive. |
| `heartbeat message <text>` | Change and save the message; it must not be empty. |
| `heartbeat pause` / `heartbeat resume` | Disable or enable heartbeat messages and save the setting. |
| `reminders list` | List reminders and their remaining time. |
| `reminders add <name> <seconds> <message>` | Add a uniquely named reminder and save it. Its timer starts now. |
| `reminders remove <name>` | Remove and save a reminder. |
| `journal show` | Show recent events held in memory. |
| `journal file <path>` | Append future events to a file. A relative path is resolved from the configuration file's directory; the path must not resolve to the configuration file. |
| `journal file off` | Stop writing events to a file; does not delete an existing file. |
| `heartbeat help`, `reminders help`, `journal help` | List the component's commands. |

Commands and arguments are separated by any whitespace; spaces inside a message or journal path are preserved.

### A walk through

These steps use the default configuration path. Start with no `agent.toml` and no `journal.log` in `examples/modular-agent/`; if either already exists, preserve it and use `--config` with a different path instead.

1. Start the application with `cargo run -p modular-agent`. Type `components` to see the three running components and `config` to see the defaults they added.
2. Type `heartbeat interval 3`. The message `heartbeat: still here` now appears every three seconds.
3. Type `reminders add tea 10 Tea is ready`. After ten seconds, `reminder tea: Tea is ready` appears and `reminders list` shows the reminder as done.
4. Type `journal file journal.log` and then `journal show`: it lists the recent heartbeat and reminder events. New events are also appended to `examples/modular-agent/journal.log`.
5. Type `quit` and start again with `cargo run -p modular-agent -- --without journal`. The journal is disabled, but `config` still shows `[Journal]` and its file setting. The `tea` reminder fires again ten seconds after the start, because reminders count from application start.
6. Type `quit` and start without `--without`: the journal appends to `journal.log` again. To start over, remove `agent.toml` and `journal.log` when you no longer need them; both are ignored by Git.

## Configuration

The first run writes defaults for all enabled components:

```toml
[Heartbeat]
enabled = true
interval_secs = 10
message = "still here"

[Journal]
keep = 100

[Reminders]
items = []
```

Component commands can add reminder tables or `[Journal.file]`. After the walk through, the file looks like this:

```toml
[Heartbeat]
enabled = true
interval_secs = 3
message = "still here"

[Journal]
keep = 100

[Journal.file]
path = "journal.log"

[[Reminders.items]]
after_secs = 10
message = "Tea is ready"
name = "tea"
```

The store writes sections and keys in alphabetical order, not in the order of the fields or of a hand-edited file.

`after_secs` is measured from application start for reminders loaded from the file, and from the `add` command for new reminders. It is not a wall-clock schedule. A fired reminder stays in the configuration, so it fires again after every start until it is removed with `reminders remove`.

| Section | Key | Default | Meaning |
|---------|-----|---------|---------|
| `Heartbeat` | `enabled` | `true` | Whether the periodic message is printed and published. |
| `Heartbeat` | `interval_secs` | `10` | Seconds between messages; must be greater than zero. |
| `Heartbeat` | `message` | `"still here"` | Text printed and published on each tick; must not be empty. |
| `Reminders` | `items` | `[]` | Array of tables with a unique, nonempty `name`, positive `after_secs` and nonempty `message`. |
| `Journal` | `keep` | `100` | Maximum number of recent events retained in memory; must be greater than zero. |
| `Journal.file` | `path` | absent | Optional append-only event output path; a relative path is resolved from the configuration file's directory. |

`Reminders` and its item tables, `Journal` and `Journal.file` reject unknown keys. `Heartbeat` is lenient when loading, but saving it replaces the whole section, so unrecognized keys in `[Heartbeat]` are removed by a later heartbeat command.

## Architecture

```text
agent.toml ──► ConfigStore ──┬──► [Heartbeat] ──► heartbeat worker  + ticks
                             ├──► [Reminders] ──► reminders worker  + timers, fired
                             └──► [Journal] ────► journal worker    + recent events

heartbeat and reminders ── events ──► bounded broadcast channel ──► journal

component command ──► save its scope to ConfigStore ──► persistence request
                                                     └──► single persister ──► agent.toml
```

The `+` fields are runtime state, not configuration.

| File | Responsibility |
|------|----------------|
| [`src/main.rs`](src/main.rs) | Registry, startup, console dispatch, shutdown and tests for disabled or invalid components. |
| [`src/component.rs`](src/component.rs) | Component interfaces and shared context for configuration, events, persistence and shutdown. |
| [`src/persist.rs`](src/persist.rs) | Single task that serializes requests to write the complete store. |
| [`src/events.rs`](src/events.rs) | Event type shared by publishers and subscribers. |
| [`src/components/heartbeat.rs`](src/components/heartbeat.rs) | Heartbeat settings, commands and periodic worker. |
| [`src/components/reminders.rs`](src/components/reminders.rs) | Reminder tables, commands, validation and timer worker. |
| [`src/components/journal.rs`](src/components/journal.rs) | Event subscriber, in-memory history and optional file output. |

### Startup

Each component's `start` method creates its typed scope from the shared store, validates its own rules and starts its worker. When its section is missing, it saves its defaults and requests persistence. The host dispatches commands by component name and does not inspect the component's settings.

Disabling a component skips its `start` method. A component that fails to load or validate prints an error such as `component journal: Journal.keep must be greater than zero` and never saves its scope. In both cases its existing section stays in the store and is written unchanged when another component saves or the application exits. Sections not owned by any registered component are also preserved; the host warns about them at startup, for example `warning: unowned section [Notes] is preserved`.

### Saving

The persister serializes complete-store writes. A command reports success once its scope is saved to the store and a write is requested; the file is written afterwards. If the request fails because the persister has stopped, the command restores its previous scope and store section. A later file-write failure is printed and the in-memory store remains available; another save request or the final shutdown flush retries it. If the final flush fails, the application prints the error and exits with a nonzero status. Runtime state, including heartbeat ticks, reminder timers and recent journal events, is not written to TOML.

### Shutdown

On shutdown the host first signals all components and waits for event producers to stop. Components that publish from workers must join them in `quiesce`. The host then calls `shutdown` on every component; the journal drains the events still available in its receiver before it exits. This two-phase lifecycle does not depend on the order in which components are registered.

## Limitations

- **Simple command parsing.** The first whitespace separates a command from its arguments, and quotes are not interpreted. A reminder name cannot contain whitespace.
- **Limited event history.** The journal keeps only the configured number of recent events in memory. Its file output appends every received event and does not rotate or truncate the file.
- **Bounded event channel.** If the journal cannot keep up, it reports how many events were missed.
- **Journal file errors are not fatal.** If writing or flushing the journal file fails, the error is printed and the event remains in the in-memory history. Parent directories of the journal file are not created.
- **Reminders are not a schedule.** They count from application start or from `add`, and a fired reminder fires again after a restart.
- **The file is rewritten on every save.** Every save, including the final flush on exit, rewrites the whole configuration file from the store. Comments and formatting in a hand-edited `agent.toml` are not preserved.
- **This is not a production service.** It is an illustrative agent, not a production scheduler or logging service.
