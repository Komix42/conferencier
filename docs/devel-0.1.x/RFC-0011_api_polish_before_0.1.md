# RFC-0011: Public API polish before 0.1.0

Date: 2026-11-17
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.1.0-beta.1
Amends: [RFC-0001](../devel-0.0.x/RFC-0001_baseline_architecture.md), [RFC-0005](../devel-0.0.x/RFC-0005_api_redesign.md), [RFC-0006](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), [RFC-0007](../devel-0.0.x/RFC-0007_default_expressions.md), [RFC-0009](../devel-0.0.x/RFC-0009_save_file_replacement.md)
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
- File paths in `Display` are escaped, and `Error::Io` names its file without
  the `path: Some(...)` wrapper. `Error::Parse` becomes a struct variant with an
  optional path, filled in by file methods or attached by callers that parse text
  themselves (E2, E4).
- All nine data-bearing `Error` variants are non-exhaustive (E5). Public
  constructors use a uniform `_error` suffix; the `save_file`-specific variants
  are created internally. `Io` can identify the operation, task join failures
  have their own variant, and every `save_file` error names the save target
  (E2–E6).
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
- Changing the initial `Display` templates of the key-related errors
  (`MissingKey`, `TypeMismatch`, `ValueParse`, `UnknownKey`) for beta.1; they
  wrap no source and keep the templates specified by RFC-0006. The `ValueParse`
  messages generated by the derive change only as described in F1 and E7.
  Their exact wording is not a 0.1.x compatibility guarantee (E3).

## Error messages and sources

- **E1 — One place for each cause.** An error variant that wraps another error
  returns it from `std::error::Error::source` and does not include its message in
  `Display`. This applies to `Io`, `Parse`, `Serialize`, `SaveFileCleanup` and
  `SaveFileTaskJoin`. The wrapped errors remain public fields, so callers can
  still match on them.

  Applications that want the full chain can opt in to formatting it, with each
  source on a new line prefixed by `Caused by:`. The troubleshooting guide
  includes a small helper with no additional dependency:

  ```rust,ignore
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
  ```

  Example applications use the helper where the underlying cause is useful.
  Formatting a complete chain is an explicit application choice: as noted in E7,
  a source may contain sensitive input. Conferencier does not add a library-level
  error-chain formatting helper.

- **E2 — Path rendering and I/O operation.** Where a message names a file, the
  rendered path escapes double quotes, backslashes, Unicode control characters
  and Unicode line and paragraph separators. In particular, a path cannot
  introduce a literal newline, carriage return or terminal escape character.
  For example, a newline may be rendered as `\n` and a backslash as `\\`. The
  initial beta.1 messages enclose paths in double quotes (E3), but the exact
  escape syntax is not a 0.1.x compatibility guarantee.

  Rendering may be lossy for non-Unicode paths. The original path remains in
  the corresponding variant field (`path`, `target_path` or `temporary_path`);
  the human-readable text must not be parsed to recover it. This escaping applies
  only to file paths, not to other diagnostic text or error sources. It does not
  make an entire error or source chain single-line, safe for logging or redacted
  (E7).

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

  Unsupported save targets, such as a final-component symlink, directory or
  non-regular file, are `Io` errors with `operation: Some(InspectTarget)` and an
  `InvalidInput` source whose message gives the reason. They do not have a
  separate `Error` variant; `Display` describes the inspection failure without
  its reason, and callers that need the reason use the source chain.

  `Error::Io` also gains `target_path: Option<PathBuf>`, the destination of the
  save that failed. `save_file` sets it to the path passed to it on every `Io`
  error it returns; all other operations and the `Io` constructors in E5 leave
  it `None`. For `InspectTarget` and `ReplaceTarget`, `path` and `target_path`
  name the same file. This overlap is intentional: `target_path` consistently
  identifies the save destination, while `path` continues to identify the file
  directly involved in the failed operation.

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

- **E3 — Messages.** The initial beta.1 `Display` messages are as follows,
  subject to the precedence and suffix rules below. `<p>`, `<t>` and `<target>`
  are the escaped contents of the directly involved path, temporary path and
  save destination, respectively, without their surrounding quotes. `<op>` is
  the operation's name as described below, and `<cleanup_error>` is the
  `Display` of `cleanup_error`:

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
  | `Serialize { target_path: Some(_), .. }` | `failed to serialize TOML for "<target>"` |
  | `Serialize { target_path: None, .. }` | `failed to serialize TOML` |
  | `SaveFileCleanup { .. }` | `failed to save "<target>"; removing temporary file "<t>" also failed (<cleanup_error>)` |

  The diagnostic context selected by each row and the path precedence below
  remain guarantees throughout 0.1.x. Exact wording, punctuation and layout are
  not part of the compatibility contract and may change within 0.1.x.

  The initial key-error templates remain `missing key <k>`,
  `expected <expected> at <k> but found <found>`,
  `invalid value at <k>: <message>` and `unknown key <k>`, where `<k>` is the
  key's diagnostic path built from `table_path` and `key`. Their diagnostic
  context remains guaranteed: each includes the key's diagnostic path,
  `TypeMismatch` includes `expected` and `found`, and `ValueParse` includes
  `message`. Their exact wording and the text of derive-generated
  `ValueParse.message` are not stable. This does not relax E7's requirement to
  omit the raw numeric value and retain the vector index. Callers should match
  variants and inspect fields and sources rather than parse messages.
  `ValueParse` has no separate typed reason code.

  The `operation: None, target_path: Some(_)` row applies before the
  `operation: None, path: Some(_)` row, even if both paths are present. This is
  deterministic; library-generated `save_file` I/O errors always have an
  operation, and the public I/O constructors leave `target_path` unset.

  The first matching row determines the displayed context. In beta.1, `<op>`
  uses the operation names above without a path, for example
  `failed to write temporary file`. For `CreateTemporary`, `PrepareTemporary`
  and `WriteTemporary`, a `target_path` of `Some` is included as context, with
  or without `<p>`. In beta.1, the message appends ` for "<target>"`, for
  example `failed to write temporary file "<t>" for "<target>"`.
  `InspectTarget` and `ReplaceTarget` display `path` when present without
  repeating `target_path`; in library-generated save errors both name the same
  file. When `path` is absent, they use `target_path` as the displayed path.
  `SaveFileCleanup` keeps `save_error` as its source. `cleanup_error` is not
  part of the source chain, so its message stays in `Display`. A report such as
  `anyhow`'s `{:#}` then reads, for example:

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
  `..` and callers cannot construct these variants directly. Callers use the
  following public constructors for caller-created error variants:

  - `Error::io_error(path: Option<PathBuf>, source: std::io::Error) -> Error`;
  - `Error::io_error_with_operation(path: Option<PathBuf>, operation: IoOperation, source: std::io::Error) -> Error`;
  - `Error::parse_error(path: Option<PathBuf>, source: toml::de::Error) -> Error`;
  - `Error::serialize_error(target_path: Option<PathBuf>, source: toml::ser::Error) -> Error`;
  - `Error::missing_key_error(table_path: impl Into<String>, key: impl Into<String>) -> Error`;
  - `Error::type_mismatch_error(table_path: impl Into<String>, key: impl Into<String>, expected: &'static str, found: &'static str) -> Error`;
  - `Error::value_parse_error(table_path: impl Into<String>, key: impl Into<String>, message: impl Into<String>) -> Error`;
  - `Error::unknown_key_error(table_path: impl Into<String>, key: impl Into<String>) -> Error`;

  The four key-error constructors already exist as `missing_key`,
  `type_mismatch`, `value_parse` and `unknown_key`. They are renamed with the
  `_error` suffix so that every constructor follows one naming pattern; their
  argument types and positional order are unchanged, and the first argument is
  named `table_path`. The old names are removed without deprecated aliases. Code
  generated by `conferencier-derive` uses the new names, which is sufficient
  because both crates are released in lockstep (see
  [Versioning and public dependencies](#versioning-and-public-dependencies)).

  `MissingKey`, `TypeMismatch`, `ValueParse` and `UnknownKey` use the field name
  `table_path` for the formatted path of the TOML table containing the key.
  `Io.path` and `Parse.path` remain filesystem paths of the file directly
  involved in the operation. Every `target_path` field, on `Io`, `Serialize`,
  `SaveFileTaskJoin` and `SaveFileCleanup`, is the destination passed to
  `save_file`.

  For the same reason, the `path` parameter of `ConfigTable::from_table` and
  `ConfigTable::to_table` is renamed to `table_path`, the value that a manual
  implementation passes to the key-error constructors. Parameter names are not
  part of a trait's API: the types and order are unchanged, existing
  implementations keep compiling with any parameter names, and no migration is
  needed. The general path-building helpers in `table_path`, such as
  `child_path(parent, key)` and `element_path(path, index)`, keep their
  parameter names.

  The I/O constructors use the concrete `Option<PathBuf>` rather than
  `Option<impl Into<PathBuf>>`, so callers can pass `None` without a type
  annotation. When a path is known, callers can pass it as `Some(path.into())`.

  `io_error` stores `operation: None`; `io_error_with_operation` stores
  `operation: Some(operation)`; both store `target_path: None`. A caller that
  needs an `Io.target_path` sets the public field on the constructed error, as
  shown for `Parse.path` in E4. The existing `From<std::io::Error>`,
  `From<toml::de::Error>` and `From<toml::ser::Error>` conversions remain
  available and set every path and the operation to `None`. `SaveFileTaskJoin`
  and `SaveFileCleanup` have no public constructors; `save_file` creates them
  internally. Callers can still inspect and match these non-exhaustive variants,
  but cannot create them.
  How the library constructs `SaveFileCleanup` internally is an implementation
  detail and is not specified here. It keeps the rule of RFC-0009 R3: if
  removing the temporary file succeeds or fails with `NotFound`, the original
  error is returned unchanged; only another cleanup failure produces
  `SaveFileCleanup`. Public constructors for caller-created variants allow new
  fields to be added compatibly when they can supply suitable defaults.

- **E6 — Save target and temporary file.** Every error returned by `save_file`
  identifies the path passed to it, without inferring it from the temporary
  sibling:

  - File-operation `Io` errors carry it in `target_path` (E2).
  - `SaveFileTaskJoin` carries it in its own `target_path` field (E2).
  - `Error::Serialize` becomes a struct variant:

    ```rust,ignore
    #[non_exhaustive]
    Serialize {
        /// The destination of the save, if any.
        target_path: Option<PathBuf>,
        /// The serializer's error.
        source: toml::ser::Error,
    },
    ```

    `save_file` sets `target_path` to its target; `ConfigStore::save_str` sets
    it to `None`. The field is named `target_path`, not `path`, because
    serialization involves no file: like the other `target_path` fields, it
    names the save destination. Serialization still happens before any file
    operation, so the target is left untouched.
  - `Error::SaveFileCleanup` gains `target_path: PathBuf`, in addition to
    `save_error`, `temporary_path` and `cleanup_error`. The primary failure
    remains available in full through `save_error`, including an
    `Io.operation`, the directly affected `Io.path`, `Io.target_path`, the I/O
    kind and the raw OS code. The target path is therefore retained even when
    the primary failure involved only the temporary file.

  `SaveFileTaskJoin` does not guarantee that the target is unchanged or that a
  temporary file was removed. Its source indicates cancellation or panic, but
  not which file-operation phase completed. Cancellation before the task starts
  may leave the filesystem untouched; a panic after temporary-file creation
  may leave that file behind, and a panic after replacement may mean the new
  target is already installed. Callers that need to recover must inspect the
  filesystem state.

- **E7 — Sensitive diagnostics.** Error messages, `Debug` output and source
  chains are not redacted. A TOML parse error's source can include the offending
  line of input; other diagnostics can include values, key names or file paths.
  When a derived `u64` or `usize` is out of range for a TOML integer during save,
  its `ValueParse` message omits the numeric value; a vector error identifies
  the index. In beta.1, the message is `value out of range for TOML integer`,
  with ` (at index N)` appended for a vector error. This narrow exception does
  not redact other diagnostics. Applications should decide how to handle errors
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
  TOML values `inf`, `+inf`, `-inf`, `nan`, `+nan` and `-nan` and stores the
  corresponding `f32` value. The `non-finite float` error is removed. NaN
  remains NaN when saved and loaded again, but its sign and payload are not
  preserved as part of this contract.
- **F2 — Range unchanged.** A finite `f64` value is rejected if it is less than
  `f32::MIN as f64` or greater than `f32::MAX as f64`. Both endpoints are
  accepted. This deliberately rejects any finite value outside the mathematical
  `f32` range, even when Rust's `as f32` conversion would round a value just
  beyond an endpoint back to a finite endpoint. The error is `ValueParse` and
  identifies the index for a vector element. Its initial beta.1 message is
  `value out of range for f32`, followed by ` (at index N)` for vector elements.
  Finite values inside the range are rounded to the nearest `f32` as before.
  Finite `f32` values and infinities saved by conferencier load back to the same
  value; NaN loads as NaN and is checked with `is_nan()`, not equality.

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
  The setter's parameter no longer determines the target type of an
  untyped `.collect()`; pass the iterator directly instead of collecting it,
  or annotate the collection type. The scalar setters `set_integer`, `set_float`, `set_boolean` and `set_datetime`
  are unchanged; widening conversions such as `impl Into<i64>` are not added.

What the setters store, their section validation and their errors are unchanged.
Before acquiring the store's write lock, each setter completes all
caller-provided conversions and input iteration and builds the complete TOML value.
This includes `Into<String>::into`, `IntoIterator::into_iter` and consuming
every item from an iterator. The setter then validates the section and replaces
the value in one update under the lock. Consequently, caller-provided conversion
or iterator code never runs while the write lock is held, and readers see the
old value or the complete replacement, not a partially built array. Input
preparation happens before section validation; if the section is invalid, the
input has still been consumed but the store remains unchanged.

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

  Lenient loading does not preserve unknown keys when a scope is saved. Saving
  a scope replaces its owned section, removing keys and sub-tables that are not
  mapped to its fields. Data owned by another scope must remain in a separate
  section; preserving unknown keys within a scope is a separate feature outside
  this RFC. See the [file guide](../../conferencier/guide/files.md) for the
  save behavior.

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

This support commitment is checked both against the locked dependency graph and
against a fresh dependency resolution on Rust 1.85 using Cargo's MSRV-aware
resolver in fallback mode. The fresh-resolution check validates the dependency
versions currently permitted by the crate manifests; it cannot guarantee that
arbitrary versions selected by a downstream application's full dependency
graph will also support Rust 1.85.

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
requirement, not a compatible range. The workspace dependency declaration
currently uses a caret requirement and changes to `=` with the matching
version, for example `=0.1.0-beta.1`. This keeps the derive macro paired with
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

Before publishing `0.1.0-beta.1`, also run the external package-consumer checks
specified in [Testing](#testing). These are a separate release gate: they
exercise the actual packaged artifacts and a downstream derive expansion, which
the semver comparison does not verify.

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
are part of the stable 0.1 API. A 0.1.x release must not add any method to either
trait or change an existing required method. Even a method with a default
implementation can make an existing downstream call such as `value.method()`
ambiguous when another in-scope trait provides a method with the same name.
Expose new capabilities through a standalone function or, where method syntax
is useful, a separate opt-in extension trait. Adding a method to either core
trait is reserved for the next breaking release as defined above.

## Compatibility

This RFC changes behavior that users can observe. It is released in
0.1.0-beta.1, the first release of the 0.1.x line.

`Error`'s `Display` retains the diagnostic context and path escaping specified
in E2–E3, but its exact wording and formatting, including the key-error
templates, are not stable in 0.1.x. Callers should inspect variants, fields
and sources instead of parsing messages. `ConfigStore`'s `Debug` output has
only the content guarantees in D1–D2, not a stable text format, and must not be
parsed by callers. The derived `Debug` output of `Error` and `IoOperation` is
not part of the compatibility contract either.

`Error` implements `Send + Sync + 'static` throughout the 0.1.x line. Any new
variant or field added compatibly in 0.1.x must preserve these traits.

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
  optional `target_path` (E5–E6). Patterns `Error::Serialize(_)` become
  `Error::Serialize { .. }` or `Error::Serialize { source, .. }`. Replace direct
  construction with `Error::serialize_error(None, source)` or the existing
  `Error::from(source)`.
- **Breaking:** `Error::SaveFileCleanup` is non-exhaustive and gains
  `target_path` (E3, E5–E6). It has no public constructor because only
  `save_file` creates this variant. Stop constructing it directly and add `..`
  to field patterns.
- **Breaking:** `MissingKey`, `TypeMismatch`, `ValueParse` and `UnknownKey`
  become non-exhaustive (E5). Replace direct construction with the
  `Error::missing_key_error`, `Error::type_mismatch_error`,
  `Error::value_parse_error` and `Error::unknown_key_error` constructors. Add
  `..` to field patterns.
- **Breaking:** the key-error field `path` is renamed to `table_path` (E5).
  Update field access and patterns such as
  `Error::MissingKey { path, .. }` to use `table_path`; the filesystem `path`
  fields on `Io` and `Parse` are unchanged.
- **Breaking:** the existing key-error constructors are renamed (E5), with
  unchanged argument types and positional order and no deprecated aliases. This
  mainly affects manual implementations of `ConfigScope` and `ConfigTable`:

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
- **Breaking:** an argument built with an untyped `.collect()`, such as
  `set_string_vec("App", "langs", names.iter().map(|n| n.to_string()).collect())`
  or `set_integer_vec("App", "ports", ports.iter().map(|&p| i64::from(p)).collect())`,
  no longer infers its collection type (S2, S3). Pass the iterator directly
  without `.collect()`, or annotate the type, for example
  `.collect::<Vec<_>>()`.
- **Breaking:** the `Debug` output of `ConfigStore` no longer contains values (D1).
- `f32` fields that previously failed with `non-finite float` now load (F1). An
  application that relied on this rejection must check the value itself.
- Save-time range errors for `u64` and `usize` no longer include the numeric
  value (E7); vector errors continue to include the element index.
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
    fields. Rejected in favor of non-exhaustive variants, public constructors
    where downstream code needs to create errors, and internal creation of the
    `save_file`-specific variants.
11. **Infer the target path from `save_error` or `temporary_path`.** It would avoid
     one field in `SaveFileCleanup`, but a primary failure may carry only a
     temporary path, and temporary paths are not a public naming contract from
     which the target can be recovered reliably. Rejected in favor of E6.
12. **A wrapping variant `SaveFile { target_path, source: Box<Error> }`** for every
     `save_file` failure. It names the target in one place, but callers would
     match nested errors for ordinary I/O failures, and it overlaps with
     `SaveFileCleanup`. Rejected in favor of `Io.target_path` and
     `Serialize.target_path` (E2, E6).
13. **Leave the target only in `SaveFileCleanup`.** Callers know the path they
     passed, but it is lost when the error is propagated with `?`, and a closed
     `Serialize(toml::ser::Error)` could not gain a target after 0.1. Rejected.
14. **Stabilize exact `Display` templates.** This would let callers depend on
    precise wording, but would also defer wording and punctuation improvements
    to the next breaking release. Rejected in favor of stable diagnostic context
    and structured error fields.
15. **Display file paths without escaping.** This keeps paths more familiar,
    especially on Windows, but allows quotes or control characters in a path to
    distort the message. Rejected in favor of E2; other diagnostics still need
    application-level handling.
16. **Name the save target `Serialize.path`.** It would match the field name of
    `Io` and `Parse`, but there `path` is the file directly involved in the
    operation, while serialization involves no file. Callers would read the save
    target from `path` for one variant and from `target_path` for the others.
    Rejected in favor of `Serialize.target_path` (E6).

## Out of scope

- The internal findings A8 and B1–B5 of the audit; they follow the plan in the
  [backlog](../BACKLOG.md#planned-for-010-beta1).
- A library-level redaction mechanism for values, fields or error diagnostics.
- A blocking variant of `from_file`.
- Changes in beta.1 to the `MissingKey`, `TypeMismatch`, `ValueParse` and
  `UnknownKey` messages, other than the save-time integer range message in E7
  and the removal of the `non-finite float` message in F1.

## Testing

- A compile-time assertion in `error.rs` verifies that `Error` implements
  `Send + Sync + 'static`, without adding a test dependency.
- Unit tests in `error.rs` assert the initial messages and diagnostic context
  of E3, with and without a path, including the key-error templates, and assert
  that `source()` returns the wrapped error for each wrapping variant. Exact
  message assertions test the initial implementation, not a 0.1.x text-format
  guarantee.
- Tests preserve `From<std::io::Error>`, `From<toml::de::Error>` and
  `From<toml::ser::Error>` and assert that each conversion leaves the available
  path, operation and save target unset.
- The `Io` display tests cover pathless operations with and without
  `target_path`, including `InspectTarget` and `ReplaceTarget` using the known
  target path, and temporary-file operations including that target as context.
  They also cover `operation: None` with both `path` and `target_path` present,
  verifying the first-matching-row precedence in E3.
- Display tests use synthetic paths containing quotes, backslashes, newline,
  carriage return, a terminal escape character and Unicode line and paragraph
  separators. They verify E2's escaping for `Io`, `Parse`, `Serialize`,
  `SaveFileTaskJoin` and `SaveFileCleanup`, covering every displayed `path`,
  `target_path` and `temporary_path` field, and verify that the original
  `PathBuf` fields are unchanged. The tests do not require creating files with
  those names.
- A test asserts that no wrapping variant's `Display` contains the message of its
  source.
- A doctest for `display_error_chain` verifies that it includes the outer error
  and every source in order, each source on a new `Caused by:` line. Include a
  multi-line source and verify that its text is preserved.
- Tests in `store.rs` assert `operation: Some(Read)`, the read path and
  `target_path: None` for file-read errors from `from_file` and `load_file`.
  They also assert the operation, the directly affected path and `target_path`
  for target inspection, temporary-file creation, preparation, writing and
  replacement. For target inspection and replacement, they assert that `path`
  and `target_path` contain the same path. Each file-operation `save_file` error
  has an explicit operation and the target path. A separate join-failure test
  asserts `SaveFileTaskJoin`, its target path and `Display`, and that its source
  is the original `tokio::task::JoinError`. It does not assert that the target
  is unchanged or that a temporary file was removed.
- Tests for unsupported save targets assert `operation: Some(InspectTarget)`,
  the target in both path fields, an `InvalidInput` source and that the source
  chain exposes the rejection reason.
- A serialization failure from `save_file` yields
  `Error::Serialize { target_path: Some(target), .. }` and leaves the target
  untouched; the same failure from `save_str` has `target_path: None`.
- A test of simultaneous save and cleanup failure asserts `target_path`,
  `temporary_path`, the nested operation and the updated `Display` and source
  chain. It also verifies that the cleanup cause is not repeated by `source()`.
- Tests of `from_file` and `load_file` with invalid TOML assert
  `Error::Parse { path: Some(p), .. }` with the read path; tests of `from_str`
  and `load_str` assert `path: None`.
- A doctest for synchronous file loading verifies that a parse error from
  `from_str` can be given the source file path without losing its parser source.
- Tests outside the library crate use public constructors for caller-created
  variants and inspect fields using patterns with `..`, including `table_path`
  on key errors. Internal tests construct and inspect `SaveFileTaskJoin` and
  `SaveFileCleanup`. Compile-fail tests outside the crate verify that direct
  construction and patterns without `..` are rejected for all nine variants.
- `cargo test -p conferencier --no-default-features --locked` and the workspace tests
  pass with the reduced Tokio features; a test or doctest runs an async method on
  a current-thread runtime.
- CI checks the library without dev targets, both with the default `derive`
  feature and without default features, using
  `cargo check -p conferencier --lib --locked` and
  `cargo check -p conferencier --lib --no-default-features --locked`. This checks
  that the normal dependencies supply the features needed to compile the library;
  test and doctest dev-dependencies cannot mask a missing Tokio feature.
- The MSRV CI job keeps its existing checks with `--locked` and also performs a
  separate fresh dependency resolution on Rust 1.85 with Cargo's MSRV-aware
  resolver in fallback mode, without reusing the workspace `Cargo.lock`. It
  checks the workspace with all targets and features and checks `conferencier`
  without default features against that resolved graph. Keep the fresh lockfile
  isolated so this check does not change the committed lockfile.
- During implementation, inspect the production Tokio feature graph with
  `cargo tree -p conferencier --edges normal,build,features --invert tokio --locked`
  both with and without `--no-default-features`, and verify that it enables only
  `sync`, `rt` and `fs`. This manual check covers unwanted features; the CI
  compile checks alone do not enforce their absence.
- Derive tests load `inf`, `+inf`, `-inf`, `nan`, `+nan` and `-nan` into `f32`
  and `Vec<f32>` fields. They verify infinities with equality and all NaN
  spellings with `is_nan()`, without asserting NaN's sign or payload. Tests
  accept both finite endpoints and reject finite values just outside
  `f32::MIN` and `f32::MAX`, including a value above `f32::MAX` that would cast
  to a finite `f32`. Vector range errors include the element index. A round trip
  of a non-finite `f32` default through save and load succeeds and checks the
  reloaded NaN with `is_nan()`.
- Save tests for out-of-range `u64` and `usize` values assert the exact generic
  `ValueParse` message and that it does not contain the numeric value; vector
  errors still identify the element index.
- Setter tests pass `&str`, `String`, arrays of `&str` and iterators, including
  a mapped iterator passed directly without `.collect()`. They also
  cover a borrowed `Vec<String>`, a borrowed slice of `&str` via
  `.iter().copied()`, and borrowed numeric, boolean and datetime slices via
  `.iter().copied()`. Empty arrays are stored using an inferred `vec![]` for an
  integer setter and a typed empty vector for the string setter.
- A regression test verifies that a string-vector iterator can read from the
  same store while producing its items, and that the setter stores the complete
  array without deadlocking. Also verify that a custom `Into<String>`
  conversion that reads from the same store completes before the write lock is
  acquired. Use a bounded wait so a regression cannot leave a test blocked
  indefinitely.
- Tests assert that ordinary and alternate `Debug` output contains all top-level
  keys, including non-table keys, and no values, without relying on key order.
  With the write lock held, `Debug` does not wait and indicates that it could not
  read keys because the store is locked; a poisoned lock still yields the keys
  and visibly reports its state. These tests do not require a particular text
  format. The contention test uses a bounded wait and releases the lock even on
  timeout, so a regression cannot leave the test blocked indefinitely.
- The guide's custom scope `Debug` example is tested as a doctest.
- Before publishing `0.1.0-beta.1`, build two small downstream consumer
  projects outside the repository workspace against the actual `.crate`
  archives for `conferencier` and `conferencier-derive`. Resolve the matching
  unpublished beta packages from an isolated temporary local registry or
  `[patch.crates-io]` entries pointing to the extracted package archives; do
  not use workspace package paths. In one consumer, enable default features and
  derive a `ConfigScope` or `ConfigTable` using the crate-root re-export. In a
  separate consumer, depend on `conferencier` with default features disabled
  and compile a manual `ConfigScope` or `ConfigTable` implementation. Both
  builds must succeed. This is a one-time beta.1 release gate, not a recurring
  CI job.
- Before publishing `0.1.0-beta.1`, compare both public crates with 0.0.8 using
  the commands in [Public API checks](#public-api-checks), including both
  `conferencier` feature configurations, and reconcile all reported breaks with
  the compatibility and migration documentation. Once the beta.1 release tag
  exists, CI compares both crates with the immediately preceding published
  0.1.x tag, including pre-releases; advance the configured baseline after each
  release.

## Documentation deliverables

- Rustdoc of `Error` and `IoOperation` (variants, operations, constructors,
  `Error`'s `Send + Sync + 'static` guarantee, and the diagnostic warning in E7).
  State that `Display` wording is not stable and that callers should inspect
  variants, fields and sources instead of parsing messages. Explain that escaped
  path text may be lossy and does not sanitize the rest of the diagnostics.
  Document `table_path` as the TOML table
  diagnostic path for key errors, `path` as the filesystem path of the file
  directly involved for `Io` and `Parse`, and every `target_path` as the
  destination passed to `save_file`; document `SaveFileCleanup`'s `target_path` and
  `temporary_path` separately. Use public constructors where provided; document
  `SaveFileTaskJoin` and `SaveFileCleanup` as library-created variants that
  callers can inspect but not construct, and state that a task-join error does
  not guarantee the target is unchanged or the temporary file removed. Use `..`
  in patterns for every non-exhaustive variant, including `MissingKey`. Use
  `table_path` in key-error patterns and the renamed key-error constructors in
  all examples, including the manual `ConfigTable` implementation. Document
  the synchronous file-read example with `Error::io_error_with_operation` and
  `IoOperation::Read`. Replace the current `SaveFileCleanup` example, which
  constructs the variant directly, with an example that only inspects it, for
  example a function taking `&Error` that matches `SaveFileCleanup` and reads
  the nested `Io` kind and optional raw OS code from `save_error`, as required
  by RFC-0009 R3.
- Rustdoc of `ConfigStore` (`Debug`, setters including borrowed slices, typed
  empty string arrays, and the guarantee that caller-provided conversions and
  iteration complete before the write lock is acquired; `from_file`,
  `load_file`), the crate-level note about Tokio features (T2), and `ConfigScope`
  and `ConfigTable` (manual
  implementations remain supported; their required signatures, bounds and
  documented behavior are stable, and neither trait gains methods, throughout
  0.1.x). Replace `ConfigTable`'s current statement that its signatures may
  change while the API is in alpha. Use the `table_path` parameter name in
  `ConfigTable` and its examples, and describe it as the table path passed to
  the key errors' `table_path`, not as a section.
- Every remaining mention of the key errors' `path` field, or of a table path
  under that name, in rustdoc and the guide uses `table_path`, including the
  [nested tables guide](../../conferencier/guide/tables.md) and the
  [`table_path`](../../conferencier/src/table_path.rs) module documentation
  where it refers to the path received by `ConfigTable`.
  Filesystem `path` fields and the parameter names of the `table_path` helpers
  are unchanged.
- `README.md`: remove the 0.0.x early-development notice from the introduction.
  Add a short "Status and compatibility" section after "Project scope" and
  before "Learn more", describing the beta status and the 0.1.x compatibility
  promise, including pre-releases and the deferral of breaking changes to
  0.2.0. State that Rust 1.85 is supported throughout 0.1.x, retain the MSRV
  badge, and link the migration guide with an absolute URL.
- `CONTRIBUTING.md`: document the 0.1.x MSRV commitment and the public API
  compatibility-check process, including the feature configurations and rolling
  baseline. Document that the MSRV CI checks both the locked graph and a fresh
  MSRV-aware dependency resolution. State that major upgrades of public `toml`
  or `tokio` types are breaking changes under the 0.x policy.
- Guide: printing errors with their source chain in
  [troubleshooting](../../conferencier/guide/troubleshooting.md), including the
  line-oriented opt-in helper from E1 and the warning in E7 that diagnostics
  are not generally redacted; the custom scope `Debug` example and synchronous
  file-loading example with path context in
  [concepts](../../conferencier/guide/concepts.md); the I/O error construction
  example in
  [concepts](../../conferencier/guide/concepts.md#reading-a-file-before-the-runtime-starts);
  the `f32` rules in [derive](../../conferencier/guide/derive.md#value-conversion)
  and the round-trip and task-join state notes in
  [files](../../conferencier/guide/files.md).
- The example applications print errors with their source chain.
- [Migration guide](../../conferencier/guide/migration.md): a 0.0.8 → 0.1.0-beta.1
  section covering every **Breaking** item of [Compatibility](#compatibility).
  Advise callers not to parse error messages or rely on their exact wording,
  and point them to the structured fields and sources. Separately from the
  breaking-change list, warn that lenient loading followed by saving a scope
  removes keys not mapped by that scope. This is existing behavior, not a change
  in beta.1: an older application can therefore erase keys written by a newer
  version when it saves the same section. Link to the
  [file guide](../../conferencier/guide/files.md) for the detailed behavior.
- [Changelog](../../CHANGELOG.md): under **Unreleased**, record `IoOperation`,
  `SaveFileTaskJoin` and new public constructors for caller-created errors under
  `### Added`; the save-specific variants have no public constructors. Under
  `### Changed`, record each compatibility break as a `**Breaking:**` entry,
  including the key-error `table_path` rename, `Io.operation`, `Io.target_path`,
  `Parse.path`, `Serialize.target_path` and `SaveFileCleanup.target_path` fields, other
  error-shape changes, Tokio features, setter signatures, `ConfigStore`'s
  `Debug` output and constructor renames. Record acceptance of non-finite `f32`
  values, omission of the raw value in save-time `u64`/`usize` range messages
  and the exact `conferencier-derive` version requirement as non-breaking
  changes in the same section.
- Add `Amended by: RFC-0011` to the headers of RFC-0001, RFC-0005, RFC-0006,
  RFC-0007 and RFC-0009 when this RFC is implemented.

## Resolved questions

- **Q1 — Cause in `Display` or in `source()`?** Keep E1: wrapped errors expose
  their cause through `source()` without duplicating it in `Display`. The
  troubleshooting guide documents a small opt-in helper for formatting the full
  chain, and the example applications use it where the cause is useful. E7 warns
  that printing a full chain may disclose sensitive input.
- **Q2 — Operation in `Error::Io`.** The path alone is ambiguous for save failures
  involving a temporary file. E2 adds an optional, stable `IoOperation`; errors
  from callers can leave it as `None`. E6 makes every `save_file` error name its
  target: `Io.target_path`, `Serialize.target_path`, `SaveFileCleanup.target_path`
  and `SaveFileTaskJoin.target_path`. Every save target uses the field name
  `target_path`; `path` names only a file directly involved in an operation.
- **Q3 — Debug content.** Top-level keys are enough; their order and the exact
  text format are unspecified. D2 retains non-blocking lock acquisition and
  makes lock contention and a poisoned lock visible while still showing the
  available keys when possible. `Error`'s `Display` likewise has content
  guarantees but not a stable text format (E3).
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
  and documented behavior are stable in 0.1.x; add no methods to either trait,
  even with a default, because additions can make downstream method calls
  ambiguous. Expose new capabilities through a standalone function or a
  separate opt-in extension trait. Adding a method to a core trait is reserved
  for a breaking release.
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
  documentation. This API comparison does not replace the separate external
  package-consumer release gate in Testing. After beta.1 is tagged, compare
  each candidate with the immediately preceding published 0.1.x release tag,
  including pre-releases, and advance the configured baseline after each
  release. Manually review derive syntax, generated-code behavior and the
  `#[doc(hidden)]` `__private` support, which the tool does not check.
- **Q11 — Beta policy and stable exit criteria.** Treat beta.1 as the first
  release covered by the 0.1.x compatibility commitment: each subsequent
  pre-release and release must remain backward compatible with the immediately
  preceding published 0.1.x version. The beta label does not suspend this
  guarantee: a breaking change discovered during beta is deferred to 0.2.0.
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
  the save's `target_path` and show it in `Display` (initially
  `failed to save "<target>"`); return the typed `tokio::task::JoinError`
  directly through `source()`. Do not guarantee that the target is unchanged or
  the temporary file removed; the caller must inspect filesystem state if
  recovery matters.
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
  to raise it. CI checks both the locked dependency graph and fresh resolution
  with Cargo's MSRV-aware resolver in fallback mode. This validates the crate's
  declared dependency constraints at the time of the check, not arbitrary
  versions selected by a downstream application's full dependency graph. When
  raised, update the `rust-version` metadata, CI, contributor documentation and
  migration notes. The project will define and publish a separate MSRV policy
  before the first 1.0.0 release; this RFC does not decide whether an MSRV
  increase in 1.x requires a major version.
- **Q19 — Extensibility of key-error variants.** Mark `MissingKey`,
  `TypeMismatch`, `ValueParse` and `UnknownKey` non-exhaustive alongside the
  wrapping variants. Their public constructors, renamed with the `_error`
  suffix, preserve manual error creation; downstream direct construction and
  exhaustive field patterns must migrate. New fields may be added compatibly
  in 0.1.x only when the existing constructors can supply suitable defaults.
- **Q20 — Internal `SaveFileCleanup` construction.** No public constructor is
  provided. The internal construction is an implementation detail; this RFC
  specifies only the resulting variant and keeps RFC-0009 R3's rule that a
  successful or `NotFound` cleanup returns the original error unchanged.
- **Q21 — `IoOperation` trait implementations.** Derive `Debug`, `Clone`, `Copy`,
  `PartialEq` and `Eq`. `Debug` supports the derived `Error` formatter; the
  remaining traits support ordinary copying and comparisons. Do not derive
  `Hash` without a concrete use case.
- **Q22 — `Error` auto-traits.** Guarantee `Send + Sync + 'static` throughout
  0.1.x and verify the bound with a compile-time assertion; adding a variant or
  field must preserve it.
- **Q23 — `Display` stability and paths.** Guarantee diagnostic context, not
  exact message wording, throughout 0.1.x. Escape file paths in `Display` as
  described in E2, without promising that other diagnostics or sources are
  single-line or sanitized.
- **Q24 — Setter input preparation.** Finish caller-provided conversions and
  consume iterators before acquiring the store's write lock. Then validate the
  section and commit the complete value under the lock as one update. If section
  validation fails, the input has already been consumed but the store remains
  unchanged. This prevents caller code from executing while the write lock is
  held and ensures readers see only complete values.
- **Q25 — External package-consumer check.** Before publishing beta.1, compile
  separate downstream consumers against the packaged crate archives: one using
  the re-exported derive with default features and one using manual trait
  implementations with default features disabled. Resolve the unpublished
  beta archives from an isolated local source, not workspace package paths.
  Treat this as a one-time release gate, not a recurring CI job.

## Open questions

None.

## References

- [AUDIT-001: Quality audit of 0.0.8](../devel-0.0.x/AUDIT-001_quality_0.0.8.md), findings A1–A7.
- [RFC-0001: Baseline architecture](../devel-0.0.x/RFC-0001_baseline_architecture.md): the original `Error`, Tokio features and `f32` rule.
- [RFC-0005: Public API redesign](../devel-0.0.x/RFC-0005_api_redesign.md), N3: the key-error constructor names and the direct `Error::Io` construction in its pre-runtime loading recipe.
- [RFC-0006: Strict unknown keys](../devel-0.0.x/RFC-0006_strict_unknown_keys.md), Q6.
- [RFC-0007: Default expressions](../devel-0.0.x/RFC-0007_default_expressions.md), S2.
- [RFC-0009: Reliable file saving](../devel-0.0.x/RFC-0009_save_file_replacement.md), R3–R4: the `Display` and rustdoc example of `SaveFileCleanup` and the error for unsupported save targets.
- Rust error-handling project group, [guidelines for `source` and `Display`](https://blog.rust-lang.org/inside-rust/2021/07/01/What-the-error-handling-project-group-is-working-towards/#guidelines-for-implementing-display-fmt-and-error-source).
