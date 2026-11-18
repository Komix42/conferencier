# RFC-0011: Public API polish before 0.1.0

Date: 2026-11-17
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.1.0-beta.1
Amends: [RFC-0001](../devel-0.0.x/RFC-0001_baseline_architecture.md), [RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), [RFC-0007](../devel-0.0.x/RFC-0007_default_expressions.md)
Related: [AUDIT-001](../devel-0.0.x/AUDIT-001_quality_0.0.8.md)

## Document Scope

This RFC specifies the public behavior changes that resolve findings A1–A7 of the
[quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), as planned in
the [backlog](../BACKLOG.md#planned-for-010-beta1). It covers error messages,
sources and extensibility, file paths in parse errors, the Tokio features enabled
by the library, non-finite `f32` values, the signatures of the typed setters and
the `Debug` output of `ConfigStore`. It also records the decision about strict
unknown-key handling by default, which
[RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md) Q6 postponed until
before 0.1.0. It also establishes the 0.x compatibility policy, the Rust 1.85
MSRV commitment for 0.1.x, and the API-compatibility check process for beta.1
and later 0.1.x releases.

Internal fixes from the audit (A8, B1–B5) are not specified here; they change no
public behavior beyond what the changelog records.

## Synopsis

- `Display` of a wrapping error does not repeat the cause returned by `source()`
  (E1–E3).
- `Error::Io` names its file without debug formatting, and `Error::Parse` becomes
  a struct variant with an optional path, filled in by file methods or attached
  by callers that parse text themselves (E2, E4).
- All nine data-bearing `Error` variants are non-exhaustive and have public
  constructors with a uniform `_error` suffix (E5). `Io` can identify the
  operation, task join failures have
  their own variant, and every `save_file` error names the save target (E2–E6).
- The library enables only the Tokio features `sync`, `rt` and `fs` (T1–T2).
- `0.1.x` compatibility starts with `0.1.0-beta.1`; breaking changes defer to
  `0.2.0`.
  Rust 1.85 remains supported throughout 0.1.x, and the post-1.0 MSRV policy
  will be set before `1.0.0` (Versioning, Q9, Q18).
- API compatibility is audited for both crates against `0.0.8` before beta.1,
  then against the preceding published 0.1.x release; `conferencier` is checked
  with and without default features (Q10). The crates are released in lockstep
  with an exact matching dependency; derive syntax and generated behavior are
  covered by the compatibility policy, while `__private` is not a stable API
  (Versioning, Q9–Q10).
- `f32` fields accept infinity and NaN like `f64` fields (F1–F2).
- `set_string` accepts `impl Into<String>`, the vector setters accept compatible
  iterators (S1–S3).
- `Debug` of `ConfigStore` lists the top-level keys when available, not the values;
  its text format is not stable (D1–D2). Error diagnostics are not redacted and
  may contain sensitive input (E7).
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
- [Versioning and public dependencies](#versioning-and-public-dependencies)
- [Public API checks](#public-api-checks)
- [Trait evolution](#trait-evolution)
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

The beta also needs clear expectations for users who adopt it: a compatibility
boundary for 0.x releases, a 0.1.x MSRV commitment, and checks that cover the
public API of both crates. Making the error variants extensible now avoids
unnecessary breaks as the error API evolves within that line.

## Goals and non-goals

Goals:

- Follow the common Rust conventions for error display and sources, so that
  `anyhow`, `eyre`, `tracing` and similar reporters produce clean messages.
- Let a caller identify the file behind every file-related error.
- Keep the Tokio feature set of the library minimal and documented.
- Make the derive's float handling symmetric.
- Make the setters convenient without changing what they store.
- Avoid printing configuration values through `ConfigStore`'s `Debug`.
- Establish the 0.x compatibility policy, Rust 1.85 support throughout 0.1.x,
  and the API-compatibility checks for both crates.
- Make all error variants extensible without requiring downstream code to
  change for future compatible field additions.

Non-goals:

- New configuration features, new field types or new derive attributes.
- Changing the messages of the key-related errors (`MissingKey`, `TypeMismatch`,
  `ValueParse`, `UnknownKey`); they wrap no source and stay as specified by RFC-0006.

## Error messages and sources

- **E1 — One place for each cause.** An error variant that wraps another error
  returns it from `std::error::Error::source` and does not include its message in
  `Display`. This applies to `Io`, `Parse`, `Serialize`, `SaveFileCleanup` and
  `SaveFileTaskJoin`. The wrapped errors remain public fields, so callers can
  still match on them.

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
  escaping. The exact path is available in the corresponding variant field
  (`path`, `target_path` or `temporary_path`); the message is for humans only.

  `Error::Io` gains `operation: Option<IoOperation>`. `IoOperation` is a public,
  `#[non_exhaustive]` enum re-exported from the crate root, with stable,
  user-facing operations:

  ```rust,ignore
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  #[non_exhaustive]
  pub enum IoOperation {
      /// Read a configuration file.
      Read,
      /// Inspect and validate the save destination before writing.
      InspectTarget,
      /// Create a new temporary file for the save.
      CreateTemporary,
      /// Prepare the temporary file before writing, if the platform requires it.
      PrepareTemporary,
      /// Write the serialized configuration to the temporary file.
      WriteTemporary,
      /// Replace the save destination with the completed temporary file.
      ReplaceTarget,
  }
  ```

  These names and their high-level meanings are part of the stable public API.
  They identify phases of a file operation, not individual system calls or
  guarantees that a phase performs a system call on every platform. For example,
  `PrepareTemporary` includes establishing the temporary file's permissions
  before writing where required; it may do nothing on another platform. The
  non-exhaustive enum may gain variants, but existing variants will not be
  renamed or repurposed in 0.1.x.

  `None` means that the caller or the library operation did not supply an
  operation. Conferencier fills the operation for every file operation performed
  by `ConfigStore::from_file`, `load_file` and `save_file`. A `spawn_blocking`
  join failure is not itself a file-operation error and is returned as
  `Error::SaveFileTaskJoin`; it has no `IoOperation`. For `save_file`, `path`
  continues to name the file directly involved in the failed operation: the
  target for `InspectTarget` and `ReplaceTarget`, and the temporary file for the
  other operations.

  `Error::Io` also gains `target_path: Option<PathBuf>`, the destination of the
  save that failed. `save_file` sets it to the path passed to it on every `Io`
  error it returns; all other operations and the `Io` constructors in E5 leave
  it `None`. For `InspectTarget` and `ReplaceTarget`, `path` and `target_path`
  name the same file.

  A task join failure uses this separate variant:

  ```rust,ignore
  #[non_exhaustive]
  SaveFileTaskJoin {
      /// The destination path of the save.
      target_path: PathBuf,
      /// The failure to join the Tokio blocking task.
      source: tokio::task::JoinError,
  }
  ```

  `Error::source()` returns the `JoinError` directly. This variant is not an
  `Io` error and has no `IoOperation`.

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
  | `Io { operation: Some(InspectTarget), path: None, target_path: Some(_), .. }` | `failed to inspect save target "<target>"` |
  | `Io { operation: Some(ReplaceTarget), path: None, target_path: Some(_), .. }` | `failed to replace target file "<target>"` |
  | `Io { operation: Some(op), path: None, .. }` | `failed to <op>` |
  | `Io { operation: None, target_path: Some(_), .. }` | `failed to save "<target>"` |
  | `Io { operation: None, path: Some(_), .. }` | `I/O error on "<p>"` |
  | `Io { operation: None, path: None, .. }` | `I/O error` |
  | `SaveFileTaskJoin { .. }` | `failed to save "<target>"` |
  | `Parse { path: Some(_), .. }` | `failed to parse TOML file "<p>"` |
  | `Parse { path: None, .. }` | `failed to parse TOML` |
  | `Serialize { path: Some(_), .. }` | `failed to serialize TOML for "<p>"` |
  | `Serialize { path: None, .. }` | `failed to serialize TOML` |
  | `SaveFileCleanup { .. }` | `failed to save "<target>"; removing temporary file "<t>" also failed (<cleanup_error>)` |

  The message templates in this table, including their wording and punctuation,
  are part of the 0.1.x compatibility contract. Changing a template is a
  breaking change and is deferred to 0.2.0. Substituted paths and underlying
  error messages are rendered from platform- and environment-dependent values;
  the contract does not make those values byte-for-byte identical across
  platforms.

  The `operation: None, target_path: Some(_)` row applies before the
  `operation: None, path: Some(_)` row, even if both paths are present. This is
  deterministic; library-generated `save_file` I/O errors always have an
  operation, and the public I/O constructors leave `target_path` unset.

  The first matching row applies. `<op>` uses the operation names above without
  a path, for example `failed to write temporary file`. For `CreateTemporary`,
  `PrepareTemporary` and `WriteTemporary`, a `target_path` of `Some` appends
  ` for "<target>"` to the message, with or without `<p>`, for example
  `failed to write temporary file "<t>" for "<target>"`. `InspectTarget` and
  `ReplaceTarget` do not repeat `target_path` when `<p>` is present, because
  `<p>` already names the target. When `<p>` is absent, they use `target_path`
  as the displayed path. `SaveFileCleanup` keeps `save_error` as its source.
  `cleanup_error` is not part of the source chain, so its message stays in
  `Display`. A report such as `anyhow`'s `{:#}` then reads, for example:

  ```text
  failed to save "config.toml"; removing temporary file ".conferencier.1a2b.tmp" also failed (Access is denied. (os error 5)): failed to write temporary file ".conferencier.1a2b.tmp" for "config.toml": There is not enough space on the disk. (os error 112)
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
  read. `ConfigStore::from_str`, `ConfigStore::load_str`, `FromStr` and
  `From<toml::de::Error>` set it to `None`. A caller that reads a file
  synchronously and then calls `from_str` can attach the path to a parse error by
  changing the public `path` field:

  ```rust,ignore
  let store = ConfigStore::from_str(&contents).map_err(|mut error| {
      if let Error::Parse { path: error_path, .. } = &mut error {
          *error_path = Some(path.to_path_buf());
      }
      error
  })?;
  ```

  This does not add a blocking file API or a general error-context helper.

- **E5 — Extensible variants and constructors.** All nine variants of `Error`
  (`Io`, `SaveFileTaskJoin`, `SaveFileCleanup`, `Parse`, `Serialize`, `MissingKey`,
  `TypeMismatch`, `ValueParse` and `UnknownKey`) are individually marked
  `#[non_exhaustive]`, in addition to the existing attribute on `Error` itself.
  Their public fields remain public. Outside the crate, patterns must include
  `..` and callers cannot construct these variants directly. Callers use these
  public constructors:

  - `Error::io_error(path: Option<PathBuf>, source: std::io::Error) -> Error`;
  - `Error::io_error_with_operation(path: Option<PathBuf>, operation: IoOperation, source: std::io::Error) -> Error`;
  - `Error::parse_error(path: Option<PathBuf>, source: toml::de::Error) -> Error`;
  - `Error::serialize_error(path: Option<PathBuf>, source: toml::ser::Error) -> Error`;
  - `Error::missing_key_error(path: impl Into<String>, key: impl Into<String>) -> Error`;
  - `Error::type_mismatch_error(path: impl Into<String>, key: impl Into<String>, expected: &'static str, found: &'static str) -> Error`;
  - `Error::value_parse_error(path: impl Into<String>, key: impl Into<String>, message: impl Into<String>) -> Error`;
  - `Error::unknown_key_error(path: impl Into<String>, key: impl Into<String>) -> Error`;
  - `Error::save_file_task_join_error(target_path: impl Into<PathBuf>, source: tokio::task::JoinError) -> Error`;
  - `Error::save_file_cleanup_error(target_path: impl Into<PathBuf>, temporary_path: impl Into<PathBuf>, save_error: Error, cleanup_error: std::io::Error) -> Error`.

  The four key-error constructors already exist as `missing_key`,
  `type_mismatch`, `value_parse` and `unknown_key`. They are renamed with the
  `_error` suffix so that every constructor follows one naming pattern; their
  parameters are unchanged. The old names are removed without deprecated
  aliases. Code generated by `conferencier-derive` uses the new names, which is
  sufficient because both crates are released in lockstep (see
  [Versioning and public dependencies](#versioning-and-public-dependencies)).

  The I/O constructors use the concrete `Option<PathBuf>` rather than
  `Option<impl Into<PathBuf>>`, so callers can pass `None` without a type
  annotation. When a path is known, callers can pass it as `Some(path.into())`.

  `io_error` stores `operation: None`; `io_error_with_operation` stores
  `operation: Some(operation)`; both store `target_path: None`. A caller that
  needs an `Io.target_path` sets the public field on the constructed error, as
  shown for `Parse.path` in E4. The task-join constructor takes its target path
  and typed source directly. The existing `From<std::io::Error>`,
  `From<toml::de::Error>` and `From<toml::ser::Error>` conversions remain
  available and set every path and the operation to `None`. `SaveFileCleanup` is
  normally created inside conferencier. Its public constructor accepts the
  unboxed `save_error` and boxes it internally. Additional fields can later be
  added without breaking construction or patterns if the existing constructors
  can supply suitable defaults for them.

- **E6 — Save target and temporary file.** Every error returned by `save_file`
  identifies the path passed to it, without inferring it from the temporary
  sibling:

  - File-operation `Io` errors carry it in `target_path` (E2).
  - `SaveFileTaskJoin` carries it in its own `target_path` field (E2).
  - `Error::Serialize` becomes a struct variant:

    ```rust,ignore
    #[non_exhaustive]
    Serialize {
        /// The file that was to be written, if any.
        path: Option<PathBuf>,
        /// The serializer's error.
        source: toml::ser::Error,
    },
    ```

    `save_file` sets `path` to its target; `ConfigStore::save_str` sets it to
    `None`. Serialization still happens before any file operation, so the
    target is left untouched.
  - `Error::SaveFileCleanup` gains `target_path: PathBuf`, in addition to
    `save_error`, `temporary_path` and `cleanup_error`. The primary failure
    remains available in full through `save_error`, including an
    `Io.operation`, the directly affected `Io.path`, `Io.target_path`, the I/O
    kind and the raw OS code. The target path is therefore retained even when
    the primary failure involved only the temporary file.

- **E7 — Sensitive diagnostics.** Error messages, `Debug` output and source
  chains are not redacted. A TOML parse error's source can include the offending
  line of input; other diagnostics can include values, key names or file paths.
  For example, an out-of-range `u64` saved by a derive currently appears in the
  `ValueParse` message. Applications should decide how to handle these diagnostics
  before displaying or logging them. The `ConfigStore` `Debug` policy (D1) does
  not sanitize errors or their sources.

## Tokio features

- **T1 — Minimal features.** The `conferencier` crate's normal Tokio dependency
  requests only `sync`, `rt` and `fs`. `macros` and `rt-multi-thread` move to the
  dev-dependencies, where tests and doctests use them.
- **T2 — Documented promise.** The crate documentation states that the library
  does not start a runtime and does not choose a runtime flavor; the application
  enables the Tokio features it needs, such as `macros` and `rt-multi-thread`. The
  async methods work on any Tokio runtime, including a current-thread runtime.
  Adding a Tokio feature to the library later needs a changelog entry. Removing
  one is a breaking change and is recorded as such in the changelog: applications
  may depend on the feature being enabled transitively.

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
  vectors of `String` and iterators are accepted. A borrowed `Vec<String>` is
  accepted directly; a borrowed slice of `&str` needs `.iter().copied()` to
  yield `&str` rather than `&&str`.
- **S3 — Other arrays.** `set_integer_vec`, `set_float_vec`, `set_boolean_vec` and
  `set_datetime_vec` take `values: impl IntoIterator<Item = T>` with their current
  element type `T` (`i64`, `f64`, `bool`, `Datetime`). Borrowed slices and
  vectors can be passed as `.iter().copied()`, without constructing an
  intermediate vector. A bare `vec![]` still infers the setter's element type.
  The scalar setters `set_integer`, `set_float`, `set_boolean` and `set_datetime`
  are unchanged; widening conversions such as `impl Into<i64>` are not added.

What the setters store, their section validation and their errors are unchanged.

## `Debug` of `ConfigStore`

- **D1 — Keys only.** `ConfigStore` implements `Debug` manually. When the lock is
  available, the output lists all top-level keys of the document, including keys
  whose values are not tables, in unspecified order and without any values.
  For example, `ConfigStore { keys: ["App", "Database"], .. }`. This example is
  illustrative: the exact text format, field labels, punctuation and key order
  are not stable, and callers must not parse the output. The alternate form
  `{:#?}` provides the same information, but its pretty-printing is not
  specified. Use `finish_non_exhaustive()` to indicate that the content is
  omitted.
- **D2 — Locking.** `Debug` uses `try_read` and does not wait for the lock. If
  acquisition would block, it indicates that the keys could not be read because
  the store is locked, instead of listing keys. If the lock is poisoned but can
  be acquired, it recovers the data like other store reads, lists the keys and
  visibly reports the poisoned state. These are content guarantees, not a
  promise of exact marker text or formatting. The key names are copied under the
  lock and formatted after it is released, so user code behind the formatter
  never runs while the lock is held.

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
  attribute is added. Strictness applies per type to the table that type loads;
  it does not detect an unclaimed top-level section such as a misspelled
  `[Ap]` in place of `[App]`. Reasons: a configuration file shared by several
  scopes or written by a newer application version must keep loading; strictness
  is a per-type choice that the derive already supports; and making strictness
  the default after 0.1.0 would be a breaking change, so the decision has to be
  made now rather than deferred again. A future non-rejecting inspection API
  across scopes remains a separate
  [backlog idea](../BACKLOG.md#store-and-errors).

## Versioning and public dependencies

For `0.x.y` releases, conferencier adopts a stricter compatibility policy than
the flexibility SemVer permits during initial development: treat the minor
version as the compatibility boundary. A patch release within `0.x` may contain
bug fixes and additive changes that preserve backward compatibility. A breaking
change requires the next minor release, for example `0.2.0` after `0.1.x`. This
policy also applies between pre-releases of the same minor version; for example,
`0.1.0-beta.2` must remain backward compatible with `0.1.0-beta.1`. Starting
with `1.0.0`, public API compatibility follows SemVer. This RFC does not define
whether an MSRV increase is breaking for 1.x; the project will publish a
separate MSRV policy before the first `1.0.0` release.

The current MSRV is Rust 1.85 and remains supported throughout the 0.1.x line.
Under the 0.x compatibility policy, raising the MSRV is a breaking change and
requires a later minor version, no earlier than 0.2.0. A new minor version does
not itself require an MSRV increase; it may retain 1.85. When the MSRV is raised,
update `rust-version`, the CI MSRV jobs, contributor documentation and the
release's changelog and migration notes.

The public API exposes types from both `toml` and `tokio`: `toml` is re-exported
from the crate root, its values and errors appear in public interfaces, and
[`SharedScope`](../../conferencier/src/scope.rs) is an alias for
`Arc<tokio::sync::RwLock<T>>`. Changing the major version of either dependency
can change these public types and is therefore a breaking change for
conferencier. Such an upgrade requires the next `0.x` minor release and a
migration note. Compatible upgrades within the same dependency major version
remain possible. This policy does not promise simultaneous support for multiple
major versions of either dependency.

`conferencier` and `conferencier-derive` are released in lockstep with the same
version, including the same pre-release identifier. The `conferencier` crate
depends on the matching `conferencier-derive` version through an exact version
requirement, not a compatible range. This keeps the derive macro paired with
the library support code for which it generates calls.

The derive compatibility contract includes the accepted `#[derive(...)]` and
`#[confer(...)]` syntax and the behavior of the generated code. Removing or
changing the meaning of an input that worked in an earlier 0.1.x release, or
changing generated behavior in a way that requires downstream changes, is a
breaking change under this policy. New syntax and behavior may be added when
they preserve existing inputs and behavior.

The `#[doc(hidden)]` `conferencier::__private` module is implementation support
for generated code, not an independently stable public API. Its contents may
change within 0.1.x when coordinated with the matching derive release; every
published derive macro must generate code compatible with the matching
`conferencier` release. The project does not guarantee compatibility for
downstream code that directly uses `__private` or pairs a derive macro with a
different library version.

## Public API checks

After implementing this RFC and before publishing `0.1.0-beta.1`, run
`cargo-semver-checks` against the published `0.0.8` baseline for both
`conferencier` and `conferencier-derive`, treating the candidate as a compatible
release for the purpose of this comparison:

```sh
cargo semver-checks --package conferencier --baseline-version 0.0.8 --release-type minor --default-features
cargo semver-checks --package conferencier --baseline-version 0.0.8 --release-type minor --only-explicit-features
cargo semver-checks --package conferencier-derive --baseline-version 0.0.8 --release-type minor
```

The release type is `minor` because, for a `0.y.z` version with `y > 0`, the
checker's `minor` level corresponds to this RFC's compatible 0.x release: it
reports every breaking change but allows additive changes, such as newly
deprecated items, that the policy permits within 0.1.x. `patch` would also
reject those additive changes. The release type must be given explicitly: from
the version numbers alone, the checker treats `0.0.8 → 0.1.0-beta.1` and any
change between pre-releases as a major change and skips the breaking-change
checks.

For `conferencier`, the first command checks the default `derive` feature and
the second checks with no features enabled, matching its supported
`--no-default-features` configuration. These are `cargo-semver-checks` feature
selectors, not Cargo's `--no-default-features` flag. `conferencier-derive` has
no crate features and needs only one check.

This comparison is an audit, not a passing CI gate: the transition to 0.1
intentionally contains breaking changes. Review every reported break against
the [Compatibility](#compatibility) section and the migration guide; investigate
and document any break not already accounted for. The checker analyzes Rust
item APIs, not behavioral compatibility, changes to the derive attribute
language, generated-code behavior or the `#[doc(hidden)]` support in
`conferencier::__private`. Review those areas manually and keep the derive
inputs and generated code compatible with the matching library release.

After the `0.1.0-beta.1` release tag exists, add a CI check for both published
crates using `cargo-semver-checks`. Check `conferencier` with both feature
selectors shown above and `conferencier-derive` once. Compare each candidate
against the immediately preceding published 0.1.x release tag, including
pre-releases. Set `baseline-rev` explicitly because the default crates.io
baseline excludes pre-releases. After each 0.1.x release, advance the configured
baseline to its tag so the next candidate is checked against the latest
published API. Set the release type explicitly to `minor`, as above, to enforce
the backward-compatibility policy for every candidate.

## Trait evolution

`ConfigScope` and `ConfigTable` remain public, implementable traits. Manual
implementations are supported; neither trait is sealed.

The required method signatures, bounds and documented behavior of both traits
are part of the stable 0.1 API. A 0.1.x release must not add a new required
trait method or otherwise require existing manual implementations to change.
Future additive methods may be added only with a default implementation that
provides meaningful behavior for existing implementations. If no such default
fits the contract, expose the capability through an extension trait or a
standalone function instead. A new required method or a change to an existing
required method is reserved for the next breaking release as defined above.

## Compatibility

This RFC changes behavior that users can observe. It is released in
0.1.0-beta.1, the first release of the 0.1.x line.

The exact `Display` message templates specified in E3 are part of the 0.1.x
compatibility contract; changing them is breaking and deferred to 0.2.0.
`ConfigStore`'s `Debug` output has only the content guarantees in D1–D2, not a
stable text format, and must not be parsed by callers.

- **Breaking:** `Display` of `Io`, `Parse`, `Serialize` and `SaveFileCleanup` no
  longer contains the cause (E1, E3). Code that prints an error with `{}` and relies
  on the cause must print the source chain, for example with `anyhow`'s `{:#}` or by
  walking `source()`. Code that compares message strings must be updated.
- **Breaking:** `Error::Io` is non-exhaustive and has optional `operation` and
  `target_path` fields (E2, E5). Replace direct construction with
  `Error::io_error(path, source)` or
  `Error::io_error_with_operation(path, operation, source)`, and add `..` to
  patterns such as `Error::Io { path, operation, source, .. }`.
- **Breaking:** a `save_file` task join failure is returned as
  `Error::SaveFileTaskJoin` instead of `Error::Io` (E2). Callers matching `Io`
  for this failure must handle the new variant.
- **Breaking:** `Error::Parse` is a non-exhaustive struct variant (E4–E5).
  Patterns `Error::Parse(_)` become `Error::Parse { .. }` or
  `Error::Parse { source, .. }`. Replace direct construction with
  `Error::parse_error(None, source)` or the existing `Error::from(source)`.
- **Breaking:** `Error::Serialize` is a non-exhaustive struct variant with an
  optional `path` (E5–E6). Patterns `Error::Serialize(_)` become
  `Error::Serialize { .. }` or `Error::Serialize { source, .. }`. Replace direct
  construction with `Error::serialize_error(None, source)` or the existing
  `Error::from(source)`.
- **Breaking:** `Error::SaveFileCleanup` is non-exhaustive and gains
  `target_path` (E3, E5–E6). Replace direct construction with
  `Error::save_file_cleanup_error(target_path, temporary_path, save_error,
  cleanup_error)` and add `..` to field patterns.
- **Breaking:** `MissingKey`, `TypeMismatch`, `ValueParse` and `UnknownKey`
  become non-exhaustive (E5). Replace direct construction with the
  `Error::missing_key_error`, `Error::type_mismatch_error`,
  `Error::value_parse_error` and `Error::unknown_key_error` constructors. Add
  `..` to field patterns.
- **Breaking:** the existing key-error constructors are renamed (E5), with
  unchanged parameters and no deprecated aliases. This mainly affects manual
  implementations of `ConfigScope` and `ConfigTable`:

  | 0.0.8 | 0.1.0-beta.1 |
  | --- | --- |
  | `Error::missing_key(...)` | `Error::missing_key_error(...)` |
  | `Error::type_mismatch(...)` | `Error::type_mismatch_error(...)` |
  | `Error::value_parse(...)` | `Error::value_parse_error(...)` |
  | `Error::unknown_key(...)` | `Error::unknown_key_error(...)` |
- **Breaking:** the library no longer enables Tokio's `macros` and `rt-multi-thread`
  features (T1). An application that used `#[tokio::main]`, `#[tokio::test]` or the
  multi-threaded runtime without enabling these features itself fails to compile
  until it adds them to its own `tokio` dependency.
- **Breaking:** calls such as `set_string("App", "name", "demo".into())` or
  `set_string_vec("App", "langs", vec!["en".into()])` no longer infer the target
  type (S1, S2). Pass the value directly: `set_string("App", "name", "demo")`.
  Similarly, `set_string_vec("App", "langs", vec![])` needs an element type, for
  example `Vec::<String>::new()`. Calls that pass a `String` or a typed
  `Vec<String>` keep compiling.
- **Breaking:** the `Debug` output of `ConfigStore` no longer contains values (D1).
- `f32` fields that previously failed with `non-finite float` now load (F1). An
  application that relied on this rejection must check the value itself.
- K1 changes nothing.

`Error` is already `#[non_exhaustive]`, so adding `SaveFileTaskJoin` is
source-compatible for external matches, which already require a fallback arm.
No existing variant is removed; changes to existing variants are listed above.

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
10. **Leave error variants directly constructible.** This avoids the migration
    in E5, but adding a field would break construction and patterns that list all
    fields. Rejected in favor of non-exhaustive variants and constructors for
    all nine variants.
11. **Infer the target path from `save_error` or `temporary_path`.** It would avoid
     one field in `SaveFileCleanup`, but a primary failure may carry only a
     temporary path, and temporary paths are not a public naming contract from
     which the target can be recovered reliably. Rejected in favor of E6.
12. **A wrapping variant `SaveFile { target_path, source: Box<Error> }`** for every
     `save_file` failure. It names the target in one place, but callers would
     match nested errors for ordinary I/O failures, and it overlaps with
     `SaveFileCleanup`. Rejected in favor of `Io.target_path` and
     `Serialize.path` (E2, E6).
13. **Leave the target only in `SaveFileCleanup`.** Callers know the path they
     passed, but it is lost when the error is propagated with `?`, and a closed
     `Serialize(toml::ser::Error)` could not gain a path after 0.1. Rejected.

## Out of scope

- The internal findings A8 and B1–B5 of the audit; they follow the plan in the
  [backlog](../BACKLOG.md#planned-for-010-beta1).
- A library-level redaction mechanism for values, fields or error diagnostics.
- A blocking variant of `from_file`.
- Changes to the key-related error messages.

## Testing

- Unit tests in `error.rs` assert every message of E3, with and without a path,
  and assert that `source()` returns the wrapped error for each wrapping variant.
- Tests preserve `From<std::io::Error>`, `From<toml::de::Error>` and
  `From<toml::ser::Error>` and assert that each conversion leaves the available
  path, operation and save target unset.
- The `Io` display tests cover pathless operations with and without
  `target_path`, including `InspectTarget` and `ReplaceTarget` using the known
  target path, and temporary-file operations appending that target as context.
  They also cover `operation: None` with both `path` and `target_path` present,
  verifying the first-matching-row precedence in E3.
- A test asserts that no wrapping variant's `Display` contains the message of its
  source.
- A doctest for `display_error_chain` verifies that it includes the outer error
  and every source in order.
- Tests in `store.rs` assert `operation: Some(Read)`, the read path and
  `target_path: None` for file-read errors from `from_file` and `load_file`.
  They also assert the operation, the directly affected path and `target_path`
  for target inspection, temporary-file creation, preparation, writing and
  replacement. Each file-operation `save_file` error has an explicit operation
  and the target path. A separate join-failure test asserts
  `SaveFileTaskJoin`, its target path and `Display`, and that its source is the
  original `tokio::task::JoinError`.
- A serialization failure from `save_file` yields
  `Error::Serialize { path: Some(target), .. }` and leaves the target untouched;
  the same failure from `save_str` has `path: None`.
- A test of simultaneous save and cleanup failure asserts `target_path`,
  `temporary_path`, the nested operation and the updated `Display` and source
  chain. It also verifies that the cleanup cause is not repeated by `source()`.
- Tests of `from_file` and `load_file` with invalid TOML assert
  `Error::Parse { path: Some(p), .. }` with the read path; tests of `from_str`
  and `load_str` assert `path: None`.
- A doctest for synchronous file loading verifies that a parse error from
  `from_str` can be given the source file path without losing its parser source.
- Tests outside the library crate construct all nine error variants with the
  public constructors and inspect their fields using patterns with `..`.
  Compile-fail tests outside the crate verify that direct construction and
  patterns without `..` are rejected for all nine variants.
- `cargo test -p conferencier --no-default-features --locked` and the workspace tests
  pass with the reduced Tokio features; a test or doctest runs an async method on
  a current-thread runtime.
- CI checks the library without dev targets, both with the default `derive`
  feature and without default features, using
  `cargo check -p conferencier --lib --locked` and
  `cargo check -p conferencier --lib --no-default-features --locked`. This checks
  that the normal dependencies supply the features needed to compile the library;
  test and doctest dev-dependencies cannot mask a missing Tokio feature.
- During implementation, inspect the production Tokio feature graph with
  `cargo tree -p conferencier --edges normal,build,features --invert tokio --locked`
  both with and without `--no-default-features`, and verify that it enables only
  `sync`, `rt` and `fs`. This manual check covers unwanted features; the CI
  compile checks alone do not enforce their absence.
- Derive tests load `inf`, `+inf`, `-inf` and `nan` into `f32` and `Vec<f32>`
  fields and verify infinities with equality and NaN with `is_nan()`, without
  asserting its sign or payload. Tests accept both finite endpoints and reject
  finite values just outside `f32::MIN` and `f32::MAX`, including a value above
  `f32::MAX` that would cast to a finite `f32`. Vector range errors include the
  element index. A round trip of a non-finite `f32` default through save and load
  succeeds and checks the reloaded NaN with `is_nan()`.
- Setter tests pass `&str`, `String`, arrays of `&str` and iterators. They also
  cover a borrowed `Vec<String>`, a borrowed slice of `&str` via
  `.iter().copied()`, and borrowed numeric, boolean and datetime slices via
  `.iter().copied()`. Empty arrays are stored using an inferred `vec![]` for an
  integer setter and a typed empty vector for the string setter.
- Tests assert that ordinary and alternate `Debug` output contains all top-level
  keys, including non-table keys, and no values, without relying on key order.
  With the write lock held, `Debug` does not wait and indicates that it could not
  read keys because the store is locked; a poisoned lock still yields the keys
  and visibly reports its state. These tests do not require a particular text
  format. The contention test uses a bounded wait and releases the lock even on
  timeout, so a regression cannot leave the test blocked indefinitely.
- The guide's custom scope `Debug` example is tested as a doctest.
- Before publishing `0.1.0-beta.1`, compare both public crates with 0.0.8 using
  the commands in [Public API checks](#public-api-checks), including both
  `conferencier` feature configurations, and reconcile all reported breaks with
  the compatibility and migration documentation. Once the beta.1 release tag
  exists, CI compares both crates with the immediately preceding published
  0.1.x tag, including pre-releases; advance the configured baseline after each
  release.

## Documentation deliverables

- Rustdoc of `Error` and `IoOperation` (variants, operations, constructors, and
  the diagnostic warning in E7). Clarify that `path` is a TOML table diagnostic
  path only for key errors, but a filesystem path for `Io`, `Parse` and
  `Serialize`; document `SaveFileCleanup`'s `target_path` and `temporary_path`
  separately. Update examples to use public constructors rather than directly
  constructing `Io` or `SaveFileCleanup`, and use `..` in patterns for every
  non-exhaustive variant, including `MissingKey`. Use the renamed key-error
  constructors in all examples, including the manual `ConfigTable`
  implementation. Document the synchronous
  file-read example with `Error::io_error_with_operation` and
  `IoOperation::Read`.
- Rustdoc of `ConfigStore` (`Debug`, setters including borrowed slices and typed
  empty string arrays, `from_file`, `load_file`), the crate-level note about
  Tokio features (T2), and `ConfigScope` and `ConfigTable` (manual
  implementations remain supported; their required signatures, bounds and
  documented behavior are stable throughout 0.1.x). Replace `ConfigTable`'s
  current statement that its signatures may change while the API is in alpha.
- `README.md`: remove the 0.0.x early-development notice from the introduction.
  Add a short "Status and compatibility" section after "Project scope" and
  before "Learn more", describing the beta status and the 0.1.x compatibility
  promise, including pre-releases and the deferral of breaking changes to
  0.2.0. State that Rust 1.85 is supported throughout 0.1.x, retain the MSRV
  badge, and link the migration guide with an absolute URL.
- `CONTRIBUTING.md`: document the 0.1.x MSRV commitment and the public API
  compatibility-check process, including the feature configurations and rolling
  baseline. State that major upgrades of public `toml` or `tokio` types are
  breaking changes under the 0.x policy.
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
- [Changelog](../../CHANGELOG.md): under **Unreleased**, record `IoOperation`,
  `SaveFileTaskJoin` and new public constructors under `### Added`. Under
  `### Changed`, record each compatibility break as a `**Breaking:**` entry,
  including the `Io.operation`, `Io.target_path`, `Parse.path`, `Serialize.path`
  and `SaveFileCleanup.target_path` fields, other error-shape changes, Tokio
  features, setter signatures, `ConfigStore`'s `Debug` output and constructor
  renames. Record acceptance of non-finite `f32` values as a non-breaking change
  in the same section.
- Add `Amended by: RFC-0011` to the headers of RFC-0001, RFC-0006 and RFC-0007
  when this RFC is implemented.

## Resolved questions

- **Q1 — Cause in `Display` or in `source()`?** Keep E1: wrapped errors expose
  their cause through `source()` without duplicating it in `Display`. The
  troubleshooting guide documents a small opt-in helper for formatting the full
  chain, and the example applications use it where the cause is useful. E7 warns
  that printing a full chain may disclose sensitive input.
- **Q2 — Operation in `Error::Io`.** The path alone is ambiguous for save failures
  involving a temporary file. E2 adds an optional, stable `IoOperation`; errors
  from callers can leave it as `None`. E6 makes every `save_file` error name its
  target: `Io.target_path`, `Serialize.path`, `SaveFileCleanup.target_path` and
  `SaveFileTaskJoin.target_path`.
- **Q3 — Debug content.** Top-level keys are enough; their order and the exact
  text format are unspecified. D2 retains non-blocking lock acquisition and
  makes lock contention and a poisoned lock visible while still showing the
  available keys when possible. The E3 `Display` templates, in contrast, are
  stable throughout 0.1.x.
- **Q4 — Strict unknown keys by default.** Keep the lenient default and make
  `deny_unknown_keys` opt-in per type. This preserves compatibility with shared
  and forward-extended configuration files. A separate API for inspecting
  unclaimed keys without rejecting them remains in the backlog.
- **Q5 — Extensibility of error variants.** Review noted that the
  `#[non_exhaustive]` attribute on `Error` protects against new variants, not new
  fields in existing variants. E5 makes all nine variants non-exhaustive and
  provides constructors, allowing fields to be added when suitable defaults
  exist.
- **Q6 — Sensitive values in diagnostics.** Conferencier cannot determine which
  values an application considers sensitive. E7 documents the risk instead of
  adding redaction; the application can customize its scope's `Debug`, but that
  does not sanitize errors or their sources.
- **Q7 — Parse path before the runtime starts.** A caller that reads text
  synchronously can attach the known path to `Error::Parse` by updating its
  public `path` field, or by rebuilding it with `Error::parse_error`. This is
  sufficient for the documented use case, so no additional `with_path` helper
  is added.
- **Q8 — Evolution of `ConfigScope` and `ConfigTable`.** Keep both traits open
  for manual implementations. Their existing required method signatures, bounds
  and documented behavior are stable in 0.1.x; do not add a required method in
  that line. Additive methods need a meaningful default implementation, or the
  capability must be exposed through an extension trait or standalone function.
  Changes that require manual implementors to update are reserved for a breaking
  release.
- **Q9 — Compatibility policy for `0.x` and public dependency majors.** Treat
  the minor version as the compatibility boundary throughout `0.x`: patch
  releases and pre-releases within the same minor preserve backward
  compatibility, and a breaking change requires the next minor (for example
  `0.2.0`). After `1.0.0`, follow SemVer for public API compatibility. Since
  major versions of `toml` and `tokio` change types exposed by the public API,
  upgrading either is a breaking change under this policy; compatible upgrades
  within a dependency major remain allowed. Release `conferencier` and
  `conferencier-derive` in lockstep at the same version and make the former
  depend on the exact matching version of the latter. The accepted derive
  syntax and generated-code behavior are part of the compatibility contract;
  `#[doc(hidden)]` support in `conferencier::__private` is an implementation
  detail that may change in coordination with its matching derive release.
- **Q10 — Public API compatibility checks.** Before publishing beta.1, compare
  both published crates against 0.0.8 with `cargo-semver-checks` using the
  explicit release type `minor`, which detects breaking changes but permits
  additive ones. Check `conferencier` with `--default-features` and
  `--only-explicit-features` (default and no-default feature configurations),
  and check `conferencier-derive` once; reconcile findings with the migration
  documentation. After beta.1 is tagged, compare each candidate with the
  immediately preceding published 0.1.x release tag, including pre-releases,
  and advance the configured baseline after each release. Manually review
  derive syntax, generated-code behavior and the `#[doc(hidden)]` `__private`
  support, which the tool does not check.
- **Q11 — Beta policy and stable exit criteria.** Treat beta.1 as the first
  release covered by the 0.1.x compatibility commitment: each subsequent
  pre-release and release must remain backward compatible with the immediately
  preceding published 0.1.x version; defer any necessary break to 0.2.0.
  Publish stable 0.1.0 only after the release gates in the
  [backlog](../BACKLOG.md#stable-010-exit-criteria) are met, including
  completion of the planned work, successful CI and package checks, reviewed
  API-compatibility results, example-app validation, complete migration
  documentation and no known pending breaking changes.
- **Q12 — `IoOperation` names and meanings.** Keep the six proposed variants
  and define them as stable high-level operation phases, not as specific system
  calls; they do not promise a particular platform-specific implementation.
  The non-exhaustive enum can gain new variants, but existing names and meanings
  are not changed in 0.1.x.
- **Q13 — Pathless I/O display.** When `operation` is known but the directly
  affected `path` is absent, keep the operation-specific message. If
  `target_path` is available, `InspectTarget` and `ReplaceTarget` use it as the
  displayed path; temporary-file operations include it as save-target context.
- **Q14 — `spawn_blocking` join failure.** Return the new
  `Error::SaveFileTaskJoin { target_path, source }` variant rather than wrapping
  the join failure in `std::io::Error`. A Tokio task join failure does not
  identify a failed file-operation phase, so it has no `IoOperation`. Preserve
  the save's `target_path` and display `failed to save "<target>"`; return the
  typed `tokio::task::JoinError` directly through `source()`.
- **Q15 — Generic error conversions.** Keep all existing `From` conversions
  from I/O and TOML errors for ergonomic `?` propagation. They set no path,
  operation or save target; callers with contextual information use the public
  constructors instead.
- **Q16 — I/O constructor path type.** Keep `Option<PathBuf>` for the optional
  path arguments. It lets callers use an unambiguous `None`; a generic inner
  `impl Into<PathBuf>` would require type annotation for pathless calls.
- **Q17 — Mutability of `Parse.path`.** Keep the public, mutable `path` field
  so callers can attach diagnostic context after reading a file synchronously.
  The field holds context, not an invariant; no wrapper type or `with_path`
  helper is added.
- **Q18 — Raising the MSRV.** Keep Rust 1.85 supported throughout 0.1.x.
  Under the 0.x compatibility policy, raising the MSRV is breaking and can
  happen only in a later 0.x minor version; that minor release is not required
  to raise it. When raised, update the `rust-version` metadata, CI, contributor
  documentation and migration notes. The project will define and publish a
  separate MSRV policy before the first 1.0.0 release; this RFC does not decide
  whether an MSRV increase in 1.x requires a major version.
- **Q19 — Extensibility of key-error variants.** Mark `MissingKey`,
  `TypeMismatch`, `ValueParse` and `UnknownKey` non-exhaustive alongside the
  wrapping variants. Their public constructors, renamed with the `_error`
  suffix, preserve manual error creation; downstream direct construction and
  exhaustive field patterns must migrate. New fields may be added compatibly
  in 0.1.x only when the existing constructors can supply suitable defaults.
- **Q20 — `SaveFileCleanup` constructor signature.** Accept target and temporary
  paths as `impl Into<PathBuf>`, accept the original failure as `Error` and box
  it internally, and accept the cleanup failure as `std::io::Error`.
- **Q21 — `IoOperation` trait implementations.** Derive `Debug`, `Clone`, `Copy`,
  `PartialEq` and `Eq`. `Debug` supports the derived `Error` formatter; the
  remaining traits support ordinary copying and comparisons. Do not derive
  `Hash` without a concrete use case.

## Open questions

None.

## References

- [AUDIT-001: Quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), findings A1–A7.
- [RFC-0001: Baseline architecture](../devel-0.0.x/RFC-0001_baseline_architecture.md): the original `Error`, Tokio features and `f32` rule.
- [RFC-0006: Strict unknown keys](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), Q6.
- [RFC-0007: Default expressions](../devel-0.0.x/RFC-0007_default_expressions.md), S2.
- Rust error-handling project group, [guidelines for `source` and `Display`](https://blog.rust-lang.org/inside-rust/2021/07/01/What-the-error-handling-project-group-is-working-towards/#guidelines-for-implementing-display-fmt-and-error-source).
