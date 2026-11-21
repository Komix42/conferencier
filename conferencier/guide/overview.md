# conferencier

Conferencier is a minimal, easy-to-use TOML configuration library for Tokio applications. It maps typed Rust structs to TOML sections, nested tables and arrays of tables, and shares each component's configuration and runtime state through an explicit [`SharedScope`] handle.

The [guide] explains how the store, scopes and tables work together, how to use derive attributes, and how to migrate between releases. The [API reference](crate) documents each public type and method.

Conferencier does not layer configuration sources, watch files or validate application-specific rules. Saving a scope replaces its section; serializing the store does not preserve comments or formatting. See [saving to files](crate::guide::files) for details.

The library enables only Tokio's `sync`, `rt` and `fs` features; applications
enable runtime features such as `macros` and `rt-multi-thread` themselves.
Invalid input handled by the library returns an error rather than panicking,
with documented exceptions for caller-supplied code, file operations without a
Tokio runtime, and stack overflows from deeply nested values built in code or
parsed with `toml/unbounded`. See
[runtime requirements](crate::guide::concepts#reading-a-file-before-the-runtime-starts)
and [nesting limits](crate::guide::concepts#nesting-limits).

## Start here

- [Concepts](crate::guide::concepts): the store, scopes, loading, saving, shared runtime state and locking.
- [Derive attributes](crate::guide::derive): configure how Rust fields map to TOML.
- [Nested tables](crate::guide::tables): represent nested TOML tables with plain structs.
- [Saving to files](crate::guide::files): understand serialization, file replacement, targets and permissions.
- [Troubleshooting](crate::guide::troubleshooting): find common errors and their solutions.
- [Migration](crate::guide::migration): upgrade code and configuration from earlier releases.

The default `derive` feature re-exports `ConfigScope` and `ConfigTable` alongside their traits. Store access and manual trait implementations are available without that feature; see the [API reference](crate) and the [concepts](crate::guide::concepts).
