# RFC-0011: Public API polish before 0.1.0

Date: 2026-11-17
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.1.0-beta.1
Amends: [RFC-0001](../devel-0.0.x/RFC-0001_baseline_architecture.md)
Related: [AUDIT-001](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), [RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), [RFC-0007](../devel-0.0.x/RFC-0007_default_expressions.md)

## Document Scope

This RFC specifies the public behavior changes that resolve findings A1–A7 of the
[quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), as planned in
the [backlog](../BACKLOG.md#planned-for-010-beta1). It covers error messages,
sources and extensibility, file paths in parse errors, the Tokio features enabled
by the library, non-finite `f32` values, the signatures of the typed setters and
the `Debug` output of `ConfigStore`. It also records the decision about strict
unknown-key handling by default, which
[RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md) Q6 postponed until
before 0.1.0.

Internal fixes from the audit (A8, B1–B5) are not specified here; they change no
public behavior beyond what the changelog records.

## Synopsis

- `Display` of a wrapping error does not repeat the cause returned by `source()`
  (E1–E3).
- `Error::Io` names its file without debug formatting, and `Error::Parse` becomes
  a struct variant with an optional path, filled in by file methods or attached
  by callers that parse text themselves (E2, E4).
- `Io` and `Parse` are non-exhaustive variants with public constructors (E5).
  `Io` can also identify the operation, and `SaveFileCleanup` retains the target
  path (E2–E6).
- The library enables only the Tokio features `sync`, `rt` and `fs` (T1–T2).
- `f32` fields accept infinity and NaN like `f64` fields (F1–F2).
- `set_string` accepts `impl Into<String>`, the vector setters accept any
  `IntoIterator` (S1–S3).
- `Debug` of `ConfigStore` lists the top-level keys, not the values (D1–D2).
  Error diagnostics are not redacted and may contain sensitive input (E7).
- Unknown keys stay accepted by default; strictness remains opt-in (K1).

## Table of Contents

- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [Error messages and sources](#error-messages-and-sources)
- [Tokio features](#tokio-features)
- [Non-finite `f32` values](#non-finite-f32-values)
- [Setter signatures](#setter-signatures)
- [`Debug` of `ConfigStore`](#debug-of-configstore)
- [Unknown keys by default](#unknown-keys-by-default)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope](#out-of-scope)
- [Testing](#testing)
- [Documentation deliverables](#documentation-deliverables)
- [Resolved questions](#resolved-questions)
- [Open questions](#open-questions)
- [References](#references)

## Motivation

The 0.1.0 line is meant to be the first API that users can depend on without
following every release. The audit found no defects in the core behavior, but it
found several places where the public surface is inconvenient or surprising:

- error reporters that walk the source chain print the cause twice (A1), and
  `Error::Io` shows `path: Some("config.toml")` (A2);
- an application loading several files cannot tell which one failed to parse (A3);
- every user compiles Tokio's multi-threaded runtime and macros, even when
  the application does not use them (A4);
- `f32` and `f64` fields treat infinity and NaN differently without a reason (A5);
- the setters force callers to create owned values explicitly (A6);
- `{:?}` of a store prints secrets into logs (A7).

All of these change observable behavior and some of them break existing code.
They are cheapest to change now, before the first beta of the 0.1.x line.

## Goals and non-goals

Goals:

- Follow the common Rust conventions for error display and sources, so that
  `anyhow`, `eyre`, `tracing` and similar reporters produce clean messages.
- Let a caller identify the file behind every file-related error.
- Keep the Tokio feature set of the library minimal and documented.
- Make the derive's float handling symmetric.
- Make the setters convenient without changing what they store.
- Avoid printing configuration values through `ConfigStore`'s `Debug`.

Non-goals:

- New configuration features, new field types or new derive attributes.
- Changing the messages of the key-related errors (`MissingKey`, `TypeMismatch`,
  `ValueParse`, `UnknownKey`); they wrap no source and stay as specified by RFC-0006.

## Error messages and sources

- **E1 — One place for each cause.** An error variant that wraps another error
  returns it from `std::error::Error::source` and does not include its message in
  `Display`. This applies to `Io`, `Parse`, `Serialize` and `SaveFileCleanup`.
  The wrapped errors remain public fields, so callers can still match on them.

  Applications that want the full chain can opt in to formatting it. The
  troubleshooting guide includes a small helper with no additional dependency:

  ```rust,ignore
  fn display_error_chain(error: &dyn std::error::Error) -> String {
      let mut messages = vec![error.to_string()];
      let mut source = error.source();
      while let Some(cause) = source {
          messages.push(cause.to_string());
          source = cause.source();
      }
      messages.join(": ")
  }
  ```

  Example applications use the helper where the underlying cause is useful.
  Formatting a complete chain is an explicit application choice: as noted in E7,
  a source may contain sensitive input. Conferencier does not add a library-level
  error-chain formatting helper.

- **E2 — Path rendering and I/O operation.** Where a message names a file, the
  path is rendered with `Path::display` and enclosed in double quotes, without
  escaping. The exact path is available in the variant's `path` field; the
  message is for humans only.

  `Error::Io` gains `operation: Option<IoOperation>`. `IoOperation` is a public,
  `#[non_exhaustive]` enum re-exported from the crate root, with stable,
  user-facing operations:

  ```rust,ignore
  pub enum IoOperation {
      Read,
      InspectTarget,
      CreateTemporary,
      PrepareTemporary,
      WriteTemporary,
      ReplaceTarget,
  }
  ```

  `None` means that the caller or the library operation did not supply an
  operation. Conferencier fills the operation for every file operation performed
  by `ConfigStore::from_file`, `load_file` and `save_file`. The exceptional
  `spawn_blocking` join failure is not itself a file operation and keeps
  `operation: None`. For `save_file`, `path` continues to name the file directly
  involved in the failed system call: the target for `InspectTarget` and
  `ReplaceTarget`, and the temporary file for the other operations.

- **E3 — Messages.** `Display` produces exactly these messages (`<p>` is a
  rendered path, `<t>` a rendered temporary path and `<target>` the save
  destination):

  | Variant | Message |
  | --- | --- |
  | `Io { operation: Some(Read), path: Some(_), .. }` | `failed to read file "<p>"` |
  | `Io { operation: Some(InspectTarget), path: Some(_), .. }` | `failed to inspect save target "<p>"` |
  | `Io { operation: Some(CreateTemporary), path: Some(_), .. }` | `failed to create temporary file "<p>"` |
  | `Io { operation: Some(PrepareTemporary), path: Some(_), .. }` | `failed to prepare temporary file "<p>"` |
  | `Io { operation: Some(WriteTemporary), path: Some(_), .. }` | `failed to write temporary file "<p>"` |
  | `Io { operation: Some(ReplaceTarget), path: Some(_), .. }` | `failed to replace target file "<p>"` |
  | `Io { operation: Some(op), path: None, .. }` | `failed to <op>` |
  | `Io { operation: None, path: Some(_), .. }` | `I/O error on "<p>"` |
  | `Io { operation: None, path: None, .. }` | `I/O error` |
  | `Parse { path: Some(_), .. }` | `failed to parse TOML file "<p>"` |
  | `Parse { path: None, .. }` | `failed to parse TOML` |
  | `Serialize(_)` | `failed to serialize TOML` |
  | `SaveFileCleanup { .. }` | `failed to save "<target>"; removing temporary file "<t>" also failed (<cleanup_error>)` |

  `<op>` uses the operation names above without a path, for example
  `failed to write temporary file`. `SaveFileCleanup` keeps `save_error` as its
  source. `cleanup_error` is not part of the source chain, so its message stays
  in `Display`. A report such as `anyhow`'s `{:#}` then reads, for example:

  ```text
  failed to save "config.toml"; removing temporary file ".conferencier.1a2b.tmp" also failed (Access is denied. (os error 5)): failed to write temporary file ".conferencier.1a2b.tmp": There is not enough space on the disk. (os error 112)
  ```

- **E4 — Path of parse errors.** `Error::Parse` becomes a struct variant:

  ```rust,ignore
  #[non_exhaustive]
  Parse {
      /// The file that was parsed, if known.
      path: Option<PathBuf>,
      /// The parser's diagnostic, with its location in the document.
      source: toml::de::Error,
  },
  ```

  `ConfigStore::from_file` and `ConfigStore::load_file` set `path` to the path they
  read. `ConfigStore::from_str`, `FromStr` and `From<toml::de::Error>` set it to
  `None`. A caller that reads a file synchronously and then calls `from_str` can
  attach the path to a parse error by changing the public `path` field:

  ```rust,ignore
  let store = ConfigStore::from_str(&contents).map_err(|mut error| {
      if let Error::Parse { path: error_path, .. } = &mut error {
          *error_path = Some(path.to_path_buf());
      }
      error
  })?;
  ```

  This does not add a blocking file API or a general error-context helper.

- **E5 — Extensible variants and constructors.** `Error::Io`, `Error::Parse` and
  `Error::SaveFileCleanup` are individually marked `#[non_exhaustive]`, in
  addition to the existing attribute on `Error` itself. Their public fields
  remain public. Outside the crate, patterns must include `..` and callers cannot
  construct these variants directly. Callers use these public constructors:

  - `Error::io_error(path: Option<PathBuf>, source: std::io::Error) -> Error`;
  - `Error::io_error_with_operation(path: Option<PathBuf>, operation: IoOperation, source: std::io::Error) -> Error`;
  - `Error::parse_error(path: Option<PathBuf>, source: toml::de::Error) -> Error`.

  `io_error` stores `operation: None`; `io_error_with_operation` stores
  `operation: Some(operation)`. The existing
  `From<std::io::Error>` and `From<toml::de::Error>` conversions remain available
  and set the path and operation to `None`. `SaveFileCleanup` is normally created
  inside conferencier; because it is non-exhaustive, a public
  `Error::save_file_cleanup_error(target_path, temporary_path, save_error,
  cleanup_error)` constructor is provided for tests and manual construction.
  Additional fields can later be added without breaking construction or
  patterns if the existing constructors can supply suitable defaults for them.

- **E6 — Save target and temporary file.** `Error::SaveFileCleanup` gains
  `target_path: PathBuf`, in addition to `save_error`, `temporary_path` and
  `cleanup_error`. `target_path` is the path passed to `save_file`; it is not
  inferred from the temporary sibling or the nested I/O error. The primary
  failure remains available in full through `save_error`, including an
  `Io.operation`, the directly affected `Io.path`, the I/O kind and the raw OS
  code. The target path is therefore retained even when the primary failure
  involved only the temporary file.

- **E7 — Sensitive diagnostics.** Error messages, `Debug` output and source
  chains are not redacted. A TOML parse error's source can include the offending
  line of input; other diagnostics can include values, key names or file paths.
  For example, an out-of-range `u64` saved by a derive currently appears in the
  `ValueParse` message. Applications should decide how to handle these diagnostics
  before displaying or logging them. The `ConfigStore` `Debug` policy (D1) does
  not sanitize errors or their sources.

## Tokio features

- **T1 — Minimal features.** The `conferencier` crate enables only the Tokio
  features `sync`, `rt` and `fs`. `macros` and `rt-multi-thread` move to the
  dev-dependencies, where tests and doctests use them.
- **T2 — Documented promise.** The crate documentation states that the library
  does not start a runtime and does not choose a runtime flavor; the application
  enables the Tokio features it needs, such as `macros` and `rt-multi-thread`. The
  async methods work on any Tokio runtime, including a current-thread runtime.
  Adding a Tokio feature to the library later needs a changelog entry.

## Non-finite `f32` values

- **F1 — Accepted like `f64`.** Loading an `f32` or `Vec<f32>` field accepts the
  TOML values `inf`, `+inf`, `-inf` and `nan` and stores the corresponding `f32`
  value. The `non-finite float` error is removed. NaN remains NaN when saved and
  loaded again, but its sign and payload are not preserved as part of this
  contract.
- **F2 — Range unchanged.** A finite `f64` value is rejected if it is less than
  `f32::MIN as f64` or greater than `f32::MAX as f64`. Both endpoints are
  accepted. This deliberately rejects any finite value outside the mathematical
  `f32` range, even when Rust's `as f32` conversion would round a value just
  beyond an endpoint back to a finite endpoint. The error is `ValueParse` with
  the message `value out of range for f32`, followed by ` (at index N)` for
  vector elements. Finite values inside the range are rounded to the nearest
  `f32` as before. Finite `f32` values and infinities saved by conferencier load
  back to the same value; NaN loads as NaN and is checked with `is_nan()`, not
  equality.

## Setter signatures

- **S1 — Strings.** `ConfigStore::set_string` takes `value: impl Into<String>`.
- **S2 — String arrays.** `ConfigStore::set_string_vec` takes
  `values: impl IntoIterator<Item = impl Into<String>>`, so arrays of `&str`,
  vectors of `String` and iterators are accepted.
- **S3 — Other arrays.** `set_integer_vec`, `set_float_vec`, `set_boolean_vec` and
  `set_datetime_vec` take `values: impl IntoIterator<Item = T>` with their current
  element type `T` (`i64`, `f64`, `bool`, `Datetime`). The scalar setters
  `set_integer`, `set_float`, `set_boolean` and `set_datetime` are unchanged; widening
  conversions such as `impl Into<i64>` are not added.

What the setters store, their section validation and their errors are unchanged.

## `Debug` of `ConfigStore`

- **D1 — Keys only.** `ConfigStore` implements `Debug` manually. The output lists
  the top-level keys of the document, in document order, and no values, for example
  `ConfigStore { keys: ["App", "Database"] }`. The alternate form `{:#?}` prints the
  same information, pretty-printed.
- **D2 — Locking.** `Debug` takes the read lock like any other read and follows the
  store's locking rules, including the handling of a poisoned lock. The key names
  are copied under the lock and formatted after it is released, so user code
  behind the formatter never runs while the lock is held.

Applications that need the content use the existing read methods.
Top-level key names may themselves be sensitive. The application controls the
`Debug` implementation of its own scope types and can redact selected fields,
for example:

```rust
use conferencier::ConfigScope;
use std::fmt;

#[derive(ConfigScope)]
#[confer(section = "Database")]
struct DatabaseConfig {
    host: String,
    password: String,
}

impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("host", &self.host)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

let config = DatabaseConfig {
    host: "localhost".into(),
    password: "example-secret".into(),
};
assert!(!format!("{config:?}").contains("example-secret"));
```

This affects only formatting that scope. It does not redact raw TOML, errors or
their sources, including parse errors produced before the scope is created.

## Unknown keys by default

- **K1 — Lenient by default.** RFC-0006 Q6 left "strict by default" as a decision
  before 0.1.0. This RFC keeps the current default: derived types accept unknown
  keys unless they opt in with `deny_unknown_keys`. No `allow_unknown_keys`
  attribute is added. Reasons: a configuration file shared by several scopes or
  written by a newer application version must keep loading; strictness is a
  per-type choice that the derive already supports; and making strictness the
  default after 0.1.0 would be a breaking change, so the decision has to be made
  now rather than deferred again.

## Compatibility

This RFC changes behavior that users can observe. It is released in
0.1.0-beta.1, the first release of the 0.1.x line.

- **Breaking:** `Display` of `Io`, `Parse`, `Serialize` and `SaveFileCleanup` no
  longer contains the cause (E1, E3). Code that prints an error with `{}` and relies
  on the cause must print the source chain, for example with `anyhow`'s `{:#}` or by
  walking `source()`. Code that compares message strings must be updated.
- **Breaking:** `Error::Io` is non-exhaustive and has an optional `operation`
  field (E2, E5). Replace direct construction with
  `Error::io_error(path, source)` or
  `Error::io_error_with_operation(path, operation, source)`, and add `..` to
  patterns such as `Error::Io { path, operation, source, .. }`.
- **Breaking:** `Error::Parse` is a non-exhaustive struct variant (E4–E5).
  Patterns `Error::Parse(_)` become `Error::Parse { .. }` or
  `Error::Parse { source, .. }`. Replace direct construction with
  `Error::parse_error(None, source)` or the existing `Error::from(source)`.
- **Breaking:** `Error::SaveFileCleanup` is non-exhaustive and gains
  `target_path` (E3, E5–E6). Replace direct construction with
  `Error::save_file_cleanup_error(target_path, temporary_path, save_error,
  cleanup_error)` and add `..` to field patterns.
- **Breaking:** the library no longer enables Tokio's `macros` and `rt-multi-thread`
  features (T1). An application that used `#[tokio::main]`, `#[tokio::test]` or the
  multi-threaded runtime without enabling these features itself fails to compile
  until it adds them to its own `tokio` dependency.
- **Breaking:** calls such as `set_string("App", "name", "demo".into())` or
  `set_string_vec("App", "langs", vec!["en".into()])` no longer infer the target
  type (S1, S2). Pass the value directly: `set_string("App", "name", "demo")`.
  Calls that pass a `String` or `Vec<String>` keep compiling.
- **Breaking:** the `Debug` output of `ConfigStore` no longer contains values (D1).
- `f32` fields that previously failed with `non-finite float` now load (F1). An
  application that relied on this rejection must check the value itself.
- K1 changes nothing.

`Error` is already `#[non_exhaustive]`; no variant is added or removed.

## Alternatives considered

1. **Keep the cause in `Display` and return `None` from `source()`** (A1). Printing
   with `{}` would stay informative, which helps applications that print errors
   without a reporter, such as the example applications. It hides the cause from
   reporters, `tracing` and `anyhow::Error::chain`, and it is the less common
   convention for wrapping errors. Rejected in favor of E1; see
   [resolved Q1](#resolved-questions).
2. **Keep only the path on `Error::Io`** (A2). The path and `io::ErrorKind`
   identify the file and low-level error, but the path of a temporary file alone
   does not tell the caller whether creation, preparation or writing failed.
   Rejected in favor of the optional `IoOperation` in E2.
3. **A new variant `ParseFile { path, source }`** (A3). It would keep
   `Parse(toml::de::Error)` source-compatible, but matching every parse error would
   need two patterns, and `Io` already uses an optional path. Rejected.
4. **Wrapping a file's parse error in `Io` with `ErrorKind::InvalidData`.** It would
   mix syntax errors with I/O failures and lose the `toml::de::Error` type. Rejected.
5. **Documenting the `f32` asymmetry instead of removing it** (A5). The asymmetry
   has no technical reason, and it breaks the round trip of saved values (RFC-0007
   S2). Rejected.
6. **`impl Into<…>` element types for every vector setter** (S3). Integer and float
   conversions would allow `[1u8, 2]`, but they are inconsistent with the unchanged
   scalar setters and make some literal arrays ambiguous. Rejected.
7. **Full document with the alternate flag `{:#?}`** (D1). `{:#?}` is commonly used
   for pretty logs, so it would leak secrets just as easily. Rejected.
8. **Removing `Debug` from `ConfigStore`.** It would break `#[derive(Debug)]` on
   application types that contain a store. Rejected.
9. **Strict by default with an `allow_unknown_keys` opt-out** (K1). Catches typos
   earlier, but breaks shared and forward-compatible files by default. Rejected.
10. **Leave `Io` and `Parse` directly constructible.** This avoids the migration
    in E5, but adding a field would break construction and patterns that list all
    fields. Rejected in favor of non-exhaustive variants and constructors.
11. **Infer the target path from `save_error` or `temporary_path`.** It would avoid
     one field in `SaveFileCleanup`, but a primary failure may carry only a
     temporary path, and temporary paths are not a public naming contract from
     which the target can be recovered reliably. Rejected in favor of E6.

## Out of scope

- The internal findings A8 and B1–B5 of the audit; they follow the plan in the
  [backlog](../BACKLOG.md#planned-for-010-beta1).
- A library-level redaction mechanism for values, fields or error diagnostics.
- A blocking variant of `from_file`.
- Changes to the key-related error messages.

## Testing

- Unit tests in `error.rs` assert every message of E3, with and without a path,
  and assert that `source()` returns the wrapped error for each wrapping variant.
- A test asserts that no wrapping variant's `Display` contains the message of its
  source.
- A doctest for `display_error_chain` verifies that it includes the outer error
  and every source in order.
- Tests in `store.rs` assert the operation and the directly affected path for
  target inspection, temporary-file creation, preparation, writing and
  replacement. Each public `save_file` I/O path has an explicit operation.
- A test of simultaneous save and cleanup failure asserts `target_path`,
  `temporary_path`, the nested operation and the updated `Display` and source
  chain. It also verifies that the cleanup cause is not repeated by `source()`.
- Tests of `from_file` and `load_file` with invalid TOML assert
  `Error::Parse { path: Some(p), .. }` with the read path; a `from_str` test asserts
  `path: None`.
- A doctest for synchronous file loading verifies that a parse error from
  `from_str` can be given the source file path without losing its parser source.
- Tests outside the library crate construct `Io`, `Parse` and
  `SaveFileCleanup` with the public constructors and inspect their fields using
  patterns with `..`. Compile-fail tests outside the crate verify that direct
  construction and patterns without `..` are rejected for all three variants.
- `cargo test -p conferencier --no-default-features --locked` and the workspace tests
  pass with the reduced Tokio features; a test or doctest runs an async method on
  a current-thread runtime.
- Derive tests load `inf`, `+inf`, `-inf` and `nan` into `f32` and `Vec<f32>`
  fields and verify infinities with equality and NaN with `is_nan()`, without
  asserting its sign or payload. Tests accept both finite endpoints and reject
  finite values just outside `f32::MIN` and `f32::MAX`, including a value above
  `f32::MAX` that would cast to a finite `f32`. Vector range errors include the
  element index. A round trip of a non-finite `f32` default through save and load
  succeeds and checks the reloaded NaN with `is_nan()`.
- Setter tests pass `&str`, `String`, arrays of `&str` and iterators.
- A test asserts that the `Debug` output contains the top-level keys and no value
  of the document.
- The guide's custom scope `Debug` example is tested as a doctest.

## Documentation deliverables

- Rustdoc of `Error` and `IoOperation` (variants, operations, constructors, the
  I/O example and the diagnostic warning in E7), `ConfigStore` (`Debug`, setters,
  `from_file`, `load_file`) and the crate-level note about Tokio features (T2).
- Guide: printing errors with their source chain in
  [troubleshooting](../../conferencier/guide/troubleshooting.md), including the
  opt-in helper from E1 and the warning in E7; the custom scope `Debug` example
  and synchronous file-loading example with path context in
  [concepts](../../conferencier/guide/concepts.md); the I/O error construction
  example in
  [concepts](../../conferencier/guide/concepts.md#reading-a-file-before-the-runtime-starts);
  the `f32` rules in [derive](../../conferencier/guide/derive.md#value-conversion) and
  the round-trip note in [files](../../conferencier/guide/files.md).
- The example applications print errors with their source chain.
- [Migration guide](../../conferencier/guide/migration.md): a 0.0.8 → 0.1.0-beta.1
  section covering every **Breaking** item of [Compatibility](#compatibility).
- [Changelog](../../CHANGELOG.md): the **Breaking** entries and the `f32` change
  under [Unreleased].
- `Amended by: RFC-0011` in the header of RFC-0001, once this RFC is approved.

## Resolved questions

- **Q1 — Cause in `Display` or in `source()`?** Keep E1: wrapped errors expose
  their cause through `source()` without duplicating it in `Display`. The
  troubleshooting guide documents a small opt-in helper for formatting the full
  chain, and the example applications use it where the cause is useful. E7 warns
  that printing a full chain may disclose sensitive input.
- **Q2 — Operation in `Error::Io`.** The path alone is ambiguous for save failures
  involving a temporary file. E2 adds an optional, stable `IoOperation`; errors
  from callers can leave it as `None`. E6 separately retains the save target in
  `SaveFileCleanup`.
- **Q5 — Extensibility of wrapping variants.** Review noted that the
  `#[non_exhaustive]` attribute on `Error` protects against new variants, not new
  fields in existing variants. E5 makes `Io`, `Parse` and `SaveFileCleanup`
  non-exhaustive and provides constructors, allowing fields to be added when
  suitable defaults exist.
- **Q6 — Sensitive values in diagnostics.** Conferencier cannot determine which
  values an application considers sensitive. E7 documents the risk instead of
  adding redaction; the application can customize its scope's `Debug`, but that
  does not sanitize errors or their sources.
- **Q7 — Parse path before the runtime starts.** A caller that reads text
  synchronously can attach the known path to `Error::Parse` by updating its
  public `path` field, or by rebuilding it with `Error::parse_error`. This is
  sufficient for the documented use case, so no additional `with_path` helper
  is added.

## Open questions

- **Q3 — Debug content.** Are top-level keys enough, or should `Debug` also list
  nested table names, or print only the number of sections?
- **Q4 — Strict by default.** Is the decision of K1 (keep lenient) acceptable for
  0.1.0, or should it be reconsidered before the stable release?

## References

- [AUDIT-001: Quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), findings A1–A7.
- [RFC-0001: Baseline architecture](../devel-0.0.x/RFC-0001_baseline_architecture.md): the original `Error`, Tokio features and `f32` rule.
- [RFC-0006: Strict unknown keys](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), Q6.
- [RFC-0007: Default expressions](../devel-0.0.x/RFC-0007_default_expressions.md), S2.
- Rust error-handling project group, [guidelines for `source` and `Display`](https://blog.rust-lang.org/inside-rust/2021/07/01/What-the-error-handling-project-group-is-working-towards/#guidelines-for-implementing-display-fmt-and-error-source).
