Common compile-time and load-time error messages and how to fix them.

## Showing error causes

`Error`'s `Display` describes the failed operation without repeating its source.
To include the cause chain when reporting an error, format each source explicitly:

```rust
fn display_error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str("\nCaused by: ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}
# #[derive(Debug)]
# struct ExampleError {
#     text: &'static str,
#     source: Option<Box<ExampleError>>,
# }
# impl std::fmt::Display for ExampleError {
#     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
#         f.write_str(self.text)
#     }
# }
# impl std::error::Error for ExampleError {
#     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
#         match &self.source {
#             Some(source) => Some(source.as_ref()),
#             None => None,
#         }
#     }
# }
# let error = ExampleError {
#     text: "outer",
#     source: Some(Box::new(ExampleError {
#         text: "middle line 1\nmiddle line 2",
#         source: Some(Box::new(ExampleError { text: "inner", source: None })),
#     })),
# };
# assert_eq!(
#     display_error_chain(&error),
#     "outer\nCaused by: middle line 1\nmiddle line 2\nCaused by: inner",
# );
```

Error sources are not generally redacted: for example, a TOML parser diagnostic
may include the offending line of input. Decide whether to display or log a
complete chain based on the data your application handles.

## Common error messages

- "unsupported field type" → use one of the [supported](crate::guide::derive#supported-field-types) scalar/container combinations; for a struct, derive `ConfigTable` on it and add `#[confer(table)]` to the field.
- "cannot be used as a nested configuration table" → the type of a `#[confer(table)]` field must implement `ConfigTable`; derive it, or remove `#[confer(table)]` from a scalar field.
- "duplicate TOML key" → two loaded fields map to the same key; rename one via `#[confer(rename = ...)]`.
- "`ConferApp` needs an explicit section" → choose `#[confer(section = "App")]` to keep the old section, or `#[confer(section = "ConferApp")]` and update the TOML section to use the full name. The compiler cannot check that your choice matches an existing file. See [migration from 0.0.4](crate::guide::migration#from-004-to-005).
- "`#[confer(init)]` was removed" → for an ignored field write `#[confer(ignore = <expr>)]` (no quotes); on a loaded field delete `init`, and add `#[confer(default = ...)]` only if a missing key should fall back to a value. See [migration from 0.0.3](crate::guide::migration#from-003-to-004).
- "`unknown key App.prot`" → the containing struct has `#[confer(deny_unknown_keys)]`; fix the key, rename a loaded field to it, or remove strictness if extra keys are intentional. Nested types need the attribute themselves; it is not inherited. See [derive attributes](crate::guide::derive).
- "cannot be combined with `#[confer(ignore)]`" → an ignored field has no TOML key; use `#[confer(ignore = <expr>)]` instead of `default`, and drop `rename`.
- A default expression must have the field's type: for a `String`, write `"text".into()` rather than `"text"`; for a `Vec`, use `vec![...]` rather than an array literal.
- "`#[confer(default)] cannot be used on an Option field`" → remove the attribute to keep `None` on a missing key, or use the inner type with a default when a value is required.
- "`missing key App.server`" for a table → add the required table to TOML, make the field an `Option`, add `#[confer(table, field_defaults)]` to decode an empty table, or choose an explicit Rust `default`. See [nested tables](crate::guide::tables) and, when upgrading, [migration from 0.0.4](crate::guide::migration#from-004-to-005).
