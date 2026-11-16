# conferencier

Conferencier is a small TOML configuration library for Tokio applications. It maps typed Rust structs to TOML sections, nested tables and arrays of tables, and shares each component's configuration and runtime state through an explicit [`SharedScope`](crate::SharedScope) handle.

The [guide](crate::guide) explains how the store, scopes and tables work together, how to use derive attributes, and how to migrate between releases. The [API reference](crate) documents each public type and method.

## Start here

- [Concepts](crate::guide::concepts): the store, scopes, loading, saving and shared runtime state.
- [Derive attributes](crate::guide::derive): configure how Rust fields map to TOML.
- [Nested tables](crate::guide::tables): represent nested TOML tables with plain structs.
- [Saving to files](crate::guide::files): understand serialization, file targets and permissions.
- [Troubleshooting](crate::guide::troubleshooting): find common errors and their solutions.
- [Migration](crate::guide::migration): upgrade code and configuration from earlier releases.

The default `derive` feature re-exports `ConfigScope` and `ConfigTable` alongside their traits. Store access and manual trait implementations are available without that feature; see the [API reference](crate) and the [concepts](crate::guide::concepts).
