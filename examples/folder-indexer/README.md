# Folder indexer

A small console application configured with [conferencier](../../README.md). It scans configured folders, prints a summary of their files and can write a JSON catalog. Unlike the [uptime monitor](../uptime-monitor/README.md), it owns its configuration file: console commands update the configuration and save it immediately.

The example shows:

- **One store, one scope per component.** The scanner owns `[Indexer]` and the report owns `[Report]`; both scopes are loaded from one `ConfigStore`.
- **Configuration from defaults.** On the first start, an empty store and the scopes' defaults produce a usable TOML file.
- **Tables and optional values.** Sources use `Vec<Source>` tables; `[Report.summary]` uses its fields' defaults when absent. An absent extension filter or catalog is represented by `None` and removed from TOML on save.
- **Configuration and runtime state.** Scan results and counts live in ignored fields of the scanner scope, not in the configuration file.
- **Application-managed settings.** Console commands modify the shared scopes and explicitly save them to the store and then to the file.
- **Strict and lenient loading.** `[Indexer]` and its sources reject unknown keys. `[Report]` is lenient, while `[Report.catalog]` is strict. The application also warns about unexpected top-level table sections.

The package is unpublished; `serde_json` is used for the optional catalog, not for configuration. Neither its dependencies nor those of the tests become dependencies of the library.

## Running

From anywhere in the repository:

```text
cargo run -p folder-indexer
```

By default, the application opens `examples/folder-indexer/indexer.toml`, regardless of the working directory. If the file does not exist, it creates it with default settings and no sources. The file is ignored by Git.

To use a different file, pass its path as an argument:

```text
cargo run -p folder-indexer -- --config path/to/indexer.toml
```

The parent directory must exist. A relative `--config` path is resolved from the working directory; relative source and catalog paths are resolved from the configuration file's directory. Quote a `--config` argument containing spaces according to your shell's rules.

The application prints whether it created or loaded the file and waits for console commands:

```text
Created .../examples/folder-indexer/indexer.toml with the default configuration
Type `help` for commands.
No scan results yet. Type `scan` to index the configured folders.
```

### Console commands

| Command | Effect |
|---------|--------|
| `config` | Print the effective configuration, including defaults; do not write the file. |
| `add-source <name> <path>` | Add a uniquely named folder and save the configuration. Paths can be relative to the configuration file or absolute. |
| `remove-source <name>` | Remove a source and save the configuration. |
| `extensions <all\|md,txt>` | Filter by a comma-separated list of extensions without dots, or use `all` to remove the filter. Matching is ASCII case-insensitive. |
| `largest <count>` | Set the number of largest files printed in the report; `0` disables the list. |
| `catalog <path>` | Configure a JSON catalog and save the setting. The next `scan` writes it to a **new** file at this path, creating missing parent folders. A `pretty` setting of an already configured catalog is kept; a new catalog table starts with `pretty = true`. |
| `catalog off` | Disable the catalog and remove its table from the configuration; existing catalog files are not deleted. |
| `scan` | Scan the configured folders concurrently, print a report and write the optional catalog. |
| `report` | Print the most recent scan's report without scanning again or writing a catalog. |
| `help` | List commands. |
| `quit` | Exit. Ctrl+C and end of input also exit; an in-progress command finishes first. |

Configuration-changing commands save immediately. Invalid commands and validation errors leave the configuration unchanged. If saving fails, the application reports the error and restores its previous in-memory settings; see the [file-saving guide](../../conferencier/guide/files.md#replacing-the-file) for filesystem limitations.

Ctrl+C received during a command is remembered. The current command, including a scan, catalog write or configuration save, finishes before the application exits; queued commands are not run.

The console splits commands on whitespace and does not interpret quotes. Source names and paths entered there cannot contain spaces; use a TOML file instead when they do. `--config` is parsed separately by the shell and can contain spaces.

### A walk through

These steps use the bundled [`sample/`](sample/) folders and the default configuration path. Start with no `indexer.toml` and no `catalog.json` in `examples/folder-indexer/`; if either already exists, preserve it and use different paths instead.

1. Start the application with `cargo run -p folder-indexer`. Type `config` to see the defaults: recursion is on, hidden files are excluded, all extensions are included, and there are no sources.
2. Type `add-source notes sample/notes` and `add-source manuals sample/manuals`. Type `scan`: the report lists five files across the two sources. The hidden note is excluded, while files in subfolders are included.
3. Type `extensions md` and `scan`. Only the three Markdown files are included. The matching filter is saved in `indexer.toml`.
4. Type `catalog catalog.json` and `scan`. The report is printed and `examples/folder-indexer/catalog.json` is created. Open it to see each file's source, relative path, extension, size and modification time.
5. Type `extensions all`, then `scan` again. The five-file report is updated, but writing the catalog fails because `catalog.json` already exists. The existing file is **not overwritten**. To save another catalog, choose a new path with `catalog <path>` or remove the old catalog yourself.
6. Type `catalog off` and `config`: neither `extensions` nor `[Report.catalog]` is present in the effective configuration. Type `quit`, then start again: the two sources are still configured, while the last scan's results are gone.

The generated default `indexer.toml` and `catalog.json` are ignored by Git. To start over, remove those files when you no longer need them.

### JSON catalog

The catalog contains an entry per file and totals across all sources. For example, a scan containing one file could produce:

```json
{
  "files": [
    {
      "extension": "md",
      "modified_unix_secs": 1730000000,
      "path": "meeting.md",
      "size_bytes": 78,
      "source": "notes"
    }
  ],
  "total_bytes": 78,
  "total_files": 1
}
```

Paths are relative to their source, not to the catalog, and use `/` between path components. On Unix, a backslash in a file name remains a backslash, escaped as `\\` in JSON. `extension` is `null` for a file without one. Size and modification time depend on the files and filesystem.

## Configuration

The first run writes a file like this:

```toml
[Indexer]
include-hidden = false
recursive = true
sources = []

[Report.summary]
by-extension = true
largest = 5
```

An application command can add `[[Indexer.sources]]` entries or a `[Report.catalog]` table. Alternatively, stop the application and edit the file before restarting it:

```toml
[Indexer]
recursive = true
include-hidden = false
extensions = ["md", "txt"]

[[Indexer.sources]]
name = "notes"
path = "sample/notes"
exclude = ["drafts"]

[Report.summary]
largest = 5
by-extension = true

[Report.catalog]
path = "catalog.json"
pretty = true
```

When adding `[[Indexer.sources]]` by hand, first remove `sources = []`: TOML does not allow an array of tables after an array defined under the same key.

| Key | Default | Meaning |
|-----|---------|---------|
| `Indexer.recursive` | `true` | Visit subdirectories. |
| `Indexer.include-hidden` | `false` | Include names beginning with `.`; this does not inspect the Windows hidden attribute. |
| `Indexer.extensions` | absent | Absent means all files; an empty array means no files. Present entries must be nonempty and must not start with a dot. |
| `Indexer.sources` | `[]` | Array of source tables. Each requires a unique, nonempty `name` and a nonempty directory `path`. |
| `Indexer.sources[].exclude` | absent | Exact file or directory names to skip at any depth, for example `["drafts"]`; not glob patterns or paths. |
| `Report.summary.largest` | `5` | Maximum number of largest files shown. |
| `Report.summary.by-extension` | `true` | Show counts grouped by extension. |
| `Report.catalog` | absent | Optional table; absent means no catalog is written. |
| `Report.catalog.path` | required when the table is present | Path of the new JSON file, relative to the configuration directory or absolute. Missing parent folders are created. |
| `Report.catalog.pretty` | `true` | Indent the JSON output. |

`[Indexer]`, each source and `[Report.catalog]` reject unknown keys. `[Report]` and `[Report.summary]` ignore them. Because **both scopes are saved on every successful configuration edit**, unrecognized keys inside their sections are removed on the next save, even in the lenient report. Other top-level sections remain in the store and file, but are omitted from `config`'s effective snapshot. At startup the application uses `list_sections` to warn about unexpected top-level *table* sections; top-level scalars and arrays are not reported.

## Architecture

```text
                  load / save_file
   indexer.toml ◄──────────────────► ConfigStore ── from_store ──┬──► SharedScope<IndexerConfig>  + scan results
                                          ▲                      └──► SharedScope<ReportConfig>
                                          │ scope save
                                      console edits, first-run defaults

   scan: copy scanner settings, scan sources concurrently, store results
   report: read results and report settings, print summary, optionally write JSON
```

The `+` fields are runtime state, not configuration.

| File | Responsibility |
|------|----------------|
| [`src/config.rs`](src/config.rs) | Scopes, nested tables, defaults, validation, loading and saving. |
| [`src/main.rs`](src/main.rs) | Startup, command parsing and configuration edits. |
| [`src/scan.rs`](src/scan.rs) | Concurrent source scans, filters and runtime results. |
| [`src/report.rs`](src/report.rs) | Console summary and optional JSON catalog. |

### Startup and saving

`Settings::open` loads a file into a `ConfigStore`, or creates an empty store if the file is absent. `IndexerConfig::from_store` and `ReportConfig::from_store` create independent `SharedScope`s from their sections; missing sections behave like empty tables, so defaults apply. The application checks its own rules, such as unique source names. On first start, it calls each scope's `save` and then the store's `save_file` to write the defaults.

A console edit changes a scope under its write lock. After the lock is released, `Settings::save` copies both scopes to the store and writes the file. If a save reports an error, the application restores the previous store snapshot and reloads the scopes; ignored runtime fields are not changed by `load`. Conferencier itself does not automatically synchronize a scope, store and file.

`config` uses a separate temporary store: it saves the scopes there and calls `save_str`, without changing the application store or file. The application's configuration file is rewritten on each successful edit, so comments and formatting are not preserved.

### Scanning and reporting

The scanner copies settings from `SharedScope<IndexerConfig>` under a read lock and releases the lock before filesystem I/O. It starts one Tokio task per source, collects their results, and takes the write lock to publish them only after all sources succeed. If a source cannot be read, the previous results remain available through `report`. The report copies settings and results under short read locks, then formats them without holding locks.

If catalog creation fails after the scan succeeds, the new results still appear in the report and remain available in memory. Catalog creation never overwrites an existing file, including an earlier catalog. If writing a newly created file fails, an incomplete file can remain; inspect or remove it before retrying. The catalog is JSON generated by the application, not a conferencier configuration file.

## Limitations

- **No automatic reload or watching.** The file is loaded once. Stop the application before editing it by hand and restart it to use the changes.
- **A report is a snapshot.** Results are lost on exit and are not refreshed when settings or files change. Run `scan` again; until then, `report` shows the previous results.
- **Only some settings have console commands.** For example, `recursive`, `include-hidden`, `exclude`, `by-extension` and catalog `pretty` can be edited in the file while the application is stopped.
- **Filesystem traversal is deliberately simple.** The scanner reads metadata, not file contents; it skips symbolic links found within sources and non-regular files. It does not interpret glob patterns. A configured source path itself may resolve through a link.
- **Catalog paths are create-only.** Choose a new path or remove the old catalog before writing another one. If the catalog is inside a source, a later scan can index it.
- **The JSON is a snapshot too.** Modification times are Unix seconds or `null` when unavailable or before the Unix epoch; file names not representable as UTF-8 are displayed lossily. Files can change during a scan.
- **This is not a production indexer.** There is no incremental scan, watcher, content search or persistent database.
