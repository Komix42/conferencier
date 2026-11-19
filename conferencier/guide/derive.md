The `#[derive(ConfigScope)]` macro understands a small set of field/struct attributes that control how data is mapped to TOML. `#[derive(ConfigTable)]` accepts the same field attributes (see [Nested tables](crate::guide::tables)).

`#[confer(...)]` is the crate-specific derive helper attribute: `confer` abbreviates *conferencier*, like `serde` in `#[serde(...)]`. It is not a required prefix for struct names, and the attribute is not renamed with the public types.

- `#[confer(section = "Name")]` on a `ConfigScope` struct sets its TOML section name. If omitted, the section is the full struct name: `AppConfig` owns `[AppConfig]` (a raw identifier such as `r#AppConfig` too). A temporary migration check requires an explicit section for names such as `ConferApp`; see the [migration guide](crate::guide::migration#from-004-to-005).
- `#[confer(deny_unknown_keys)]` on a struct rejects the smallest unknown key in that struct's table when loading. Loaded fields define the known keys after `rename`; ignored fields do not. Strictness is per type, not inherited: if `App` has a lenient nested `Server`, `typo = 1` in `[App.server]` is still accepted; put `deny_unknown_keys` on `Server` too to reject it.

Strictness checks only the section or table being loaded. For example, a strict scope for `[App]` does not reject a misspelled top-level `[Ap]` section; a missing `[App]` is treated as empty and may load if its fields have defaults. Strictness is checked on load, not save; saving still replaces the scope's section and removes keys that do not map to loaded fields.

A load stops at the first error. A strict scope first reports a section that is not a table, then checks the section for unknown keys, then converts fields in declaration order. Nested tables are checked and converted depth-first when their field is reached. A misspelled required key is therefore reported as `unknown key App.prot` rather than `missing key App.port`, but an error in an earlier nested table can precede an error in a later outer field. If several keys are unknown, the smallest in byte-wise order is reported (alphabetical for ASCII), regardless of their order in the TOML document.

To detect a misspelled section name, an application can compare the table-backed names returned by [`ConfigStore::list_sections`](crate::ConfigStore::list_sections) with the section names it expects. This does not report top-level scalar or array entries.

Every field is either loaded from TOML or ignored by the configuration file.

**Loaded fields** (the default) are read from and written to a TOML key:

- `#[confer(rename = "key")]` on a field overrides the TOML key name. Two loaded fields of one struct must not map to the same key; the derive reports a duplicate key at compile time.

- `#[confer(default)]` uses the field type's `Default::default()` when its key is missing.

- `#[confer(default = <expr>)]` uses a Rust expression of exactly the field's type when its key is missing. For example, use `"localhost".into()` for `String`, `1.0` for `f64`, `vec!["a".into(), "b".into()]` for `Vec<String>`, and `"07:30".parse().unwrap()` for `Datetime`. An invalid datetime string then panics when the missing-key fallback is evaluated.
    - Defaults run in field declaration order, only when their key is missing; a present but invalid value returns an error instead. A failed load leaves the scope unchanged, but earlier expression side effects are not undone.
    - A syntactically recognized `Option<T>` or `Option<Vec<T>>` cannot have a `default`. Type aliases are opaque to the derive.

- `#[confer(table)]` maps the field to a nested table or an array of tables; see [Nested tables](crate::guide::tables).

- `#[confer(table, field_defaults)]` loads a missing single table from an empty table, applying its fields' defaults and reporting any required inner key. The table type need not implement `Default`. It may be combined with `rename`, but not with `default`, `ignore`, `Option<T>` or `Vec<T>`.

For every loaded field, a missing key uses its explicit `default` or `field_defaults`, otherwise `None` for a syntactically recognized `Option`, or else `Error::MissingKey`.

On save, an `Option<T>` or `Option<Vec<T>>` set to `None` is omitted from the new table; `Some(value)` is written. In particular, `Some(vec![])` writes an empty array, not an absent key.

**Ignored fields** are never read from or written to TOML. They have no key, so they are not checked for key collisions. Use them for runtime-only state (see [Configuration and runtime state](crate::guide::concepts#configuration-and-runtime-state)) or for types the derive does not support (e.g., maps):

- `#[confer(ignore)]` initializes the field with `Default::default()`.

- `#[confer(ignore = <expr>)]` initializes the field with a Rust expression, e.g. `#[confer(ignore = Vec::with_capacity(16))]`.
    - An ignored field of a scope is initialized once, in `from_store`, after all loaded fields have converted; scope `load` and `save` never touch it. An ignored field of a table is initialized whenever the table is constructed.
    - The field type does not need to implement `Default`.

Both expression forms use a synchronous closure returning the field's type. They may use constants and functions in scope, but not generated parameters, other fields or `.await`. `return` exits the closure, and `?` uses its return type rather than propagating through the generated method.

`ignore` cannot be combined with `rename` or `default`, because an ignored field has no key.

## Supported field types

- Scalars: `String`, `bool`, signed/unsigned integers (`i8`..`i64`, `isize`, `u8`..`u64`, `usize`), floats (`f32`, `f64`), and `Datetime` (`conferencier::Datetime`, a re-export of `toml::value::Datetime`).
- Containers: plain `T`, `Vec<T>`, `Option<T>`, `Option<Vec<T>>`.
- Nested tables: `T`, `Option<T>` and `Vec<T>` where `T` implements `ConfigTable`, with `#[confer(table)]`.

The derive classifies field types by their final path segment; it cannot resolve types. Fully qualified paths to supported types work, but a custom type named `String` or `Datetime` is treated as the corresponding built-in type. Type aliases are opaque: scalar aliases are unsupported, and table aliases are treated as single table types that must implement `ConfigTable`, even if they name an `Option`. References and nested vectors such as `Vec<Vec<T>>` are unsupported. For nested TOML tables, use `#[confer(table)]` with `T`, `Option<T>` or `Vec<T>`, where `T` implements `ConfigTable`.

## Value conversion

These rules apply to loaded scalar fields and their vector elements, both in scopes and in derived tables:

- Integer fields accept TOML integers, which are signed 64-bit values. Loading checks the range of the Rust field type; saving a `u64` or `usize` greater than `i64::MAX` returns an error.
- Float fields accept TOML floats and integers, but integer-to-float conversion and conversion to `f32` can lose precision. Loading `f32` accepts infinities and NaNs, and rejects finite values outside its range; `f64` accepts non-finite values.
- `Datetime` accepts a TOML date, time or datetime, or a string parsed as a TOML datetime. Other strings are not coerced into numbers or booleans.
- A vector requires an array with compatible elements. A conversion error identifies the zero-based index of the failing element.

Non-finite `f32` values can be saved and loaded back; NaN sign and payload are not preserved.

## TOML compatibility

Conferencier uses `toml` 1.2 with TOML 1.1 support. Prefer `conferencier::toml::{Table, Value}` and `conferencier::Datetime` for types shared with the store; types from `toml` 0.9 are not interchangeable with them.

TOML 1.1 times may omit seconds (for example `07:30`, or `"07:30".parse().unwrap()` as a datetime default). In the new datetime API, `Time::second` and `Time::nanosecond` are optional: `None` means the component was omitted, not explicitly set to zero.
