Common compile-time and load-time error messages and how to fix them.

- "unsupported field type" → use one of the [supported](crate::guide::derive#supported-field-types) scalar/container combinations; for a struct, derive `ConfigTable` on it and add `#[confer(table)]` to the field.
- "cannot be used as a nested configuration table" → the type of a `#[confer(table)]` field must implement `ConfigTable`; derive it, or remove `#[confer(table)]` from a scalar field.
- "duplicate TOML key" → two loaded fields map to the same key; rename one via `#[confer(rename = ...)]`.
- "`ConferApp` needs an explicit section" → choose `#[confer(section = "App")]` to keep the old section, or `#[confer(section = "ConferApp")]` and update the TOML section to use the full name. The compiler cannot check that your choice matches an existing file.
- "`#[confer(init)]` was removed" → for an ignored field write `#[confer(ignore = <expr>)]` (no quotes); on a loaded field delete `init`, and add `#[confer(default = ...)]` only if a missing key should fall back to a value.
- "`unknown key App.prot`" → the containing struct has `#[confer(deny_unknown_keys)]`; fix the key, rename a loaded field to it, or remove strictness if extra keys are intentional. Nested types need the attribute themselves; it is not inherited.
- "cannot be combined with `#[confer(ignore)]`" → an ignored field has no TOML key; use `#[confer(ignore = <expr>)]` instead of `default`, and drop `rename`.
- A default expression must have the field's type: for a `String`, write `"text".into()` rather than `"text"`; for a `Vec`, use `vec![...]` rather than an array literal.
- "`#[confer(default)] cannot be used on an Option field`" → remove the attribute to keep `None` on a missing key, or use the inner type with a default when a value is required.
- "`missing key App.server`" for a formerly implicit table → make it required, add `#[confer(table, field_defaults)]` to decode an empty table, or choose an explicit Rust `default`. Migrate nested table fallbacks too.
