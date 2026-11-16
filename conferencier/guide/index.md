The guide explains how conferencier works as a whole. The [crate documentation](crate) gives a quick overview, and the documentation of each item, such as [`ConfigStore`](crate::ConfigStore), describes its exact behavior.

- [Concepts](self::concepts): the store, scopes and tables, how data moves between them, reloading and runtime state.
- [Derive attributes](self::derive): the `#[confer(...)]` attributes and supported field types.
- [Nested tables](self::tables): sub-tables and arrays of tables mapped to plain structs.
- [Saving to files](self::files): what saving does to a configuration file, file targets and permissions.
- [Troubleshooting](self::troubleshooting): common error messages and how to fix them.
- [Migration](self::migration): upgrading from earlier releases.
