The `#[derive(ConfigScope)]` macro understands a small set of field/struct attributes that control how data is mapped to TOML. `#[derive(ConfigTable)]` accepts the same field attributes (see [Nested tables](crate::guide::tables)).

`#[confer(...)]` is the crate-specific derive helper attribute: `confer` abbreviates *conferencier*, like `serde` in `#[serde(...)]`. It is not a required prefix for struct names, and the attribute is not renamed with the public types.

- `#[confer(section = "Name")]` on the struct sets the TOML section name. If omitted, the section is the full struct name: `AppConfig` owns `[AppConfig]` (a raw identifier such as `r#AppConfig` too).
    - Until version 0.0.4, a leading `Confer` word was stripped from the default (`ConferApp` → `[App]`). To keep existing configuration files from silently moving to another section, a `ConfigScope` named `Confer` followed by an uppercase letter, such as `ConferApp`, does not compile without an explicit section: write `#[confer(section = "App")]` to keep the old section or `#[confer(section = "ConferApp")]` to use the full name. Names such as `ConferenceSettings` are not affected.
    - This migration check stays through the 0.1.x releases and is removed in 0.2.0; then `ConferApp` without `section` simply uses `[ConferApp]`.
- `#[confer(deny_unknown_keys)]` on a struct rejects the smallest unknown key in that struct's table when loading. Loaded fields define the known keys after `rename`; ignored fields do not. Strictness is per type, not inherited: if `App` has a lenient nested `Server`, `typo = 1` in `[App.server]` is still accepted; put `deny_unknown_keys` on `Server` too to reject it.

Strictness checks only the section or table being loaded. For example, a strict scope for `[App]` does not reject a misspelled top-level `[Ap]` section; a missing `[App]` is treated as empty and may load if its fields have defaults. Strictness is checked on load, not save; saving still replaces the scope's section and removes keys that do not map to loaded fields.

Every field is either loaded from TOML or ignored by the configuration file.

**Loaded fields** (the default) are read from and written to a TOML key:

- `#[confer(rename = "key")]` on a field overrides the TOML key name.

- `#[confer(default)]` uses the field type's `Default::default()` when its key is missing.

- `#[confer(default = <expr>)]` uses a Rust expression of exactly the field's type when its key is missing. For example, use `"localhost".into()` for `String`, `1.0` for `f64`, `vec!["a".into(), "b".into()]` for `Vec<String>`, and `"07:30".parse().unwrap()` for `Datetime`. An invalid datetime string then panics when the missing-key fallback is evaluated.
    - Defaults run in field declaration order, only when their key is missing; a present but invalid value returns an error instead. A failed load leaves the scope unchanged, but earlier expression side effects are not undone.
    - A syntactically recognized `Option<T>` or `Option<Vec<T>>` cannot have a `default`. Type aliases are opaque to the derive.

- `#[confer(table)]` maps the field to a nested table or an array of tables; see [Nested tables](crate::guide::tables).

- `#[confer(table, field_defaults)]` loads a missing single table from an empty table, applying its fields' defaults and reporting any required inner key. The table type need not implement `Default`. It may be combined with `rename`, but not with `default`, `ignore`, `Option<T>` or `Vec<T>`.

For every loaded field, a missing key uses its explicit `default` or `field_defaults`, otherwise `None` for a syntactically recognized `Option`, or else `Error::MissingKey`.

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

If a type falls outside this set, the derive emits a compile error with a friendly message.
Type aliases are opaque: scalar aliases are unsupported, and table aliases are treated as single table types that must implement `ConfigTable`, even if they name an `Option`.

## TOML compatibility

Conferencier uses `toml` 1.2 with TOML 1.1 support. Prefer `conferencier::toml::{Table, Value}` and `conferencier::Datetime` for types shared with the store; types from `toml` 0.9 are not interchangeable with them.

TOML 1.1 times may omit seconds (for example `07:30`, or `"07:30".parse().unwrap()` as a datetime default). In the new datetime API, `Time::second` and `Time::nanosecond` are optional: `None` means the component was omitted, not explicitly set to zero.
