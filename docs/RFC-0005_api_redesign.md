# RFC-0005: Public API redesign: naming, shared store and async boundaries

Date: 2026-11-12
Status: Approved
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5
Amends: [RFC-0001](./RFC-0001_baseline_architecture.md), [RFC-0003](./RFC-0003_field_attributes.md), [RFC-0004](./RFC-0004_nested_tables.md)

## Document Scope

This RFC redesigns the public API of conferencier while the crate is at 0.0.x. It sets the final public names and changes the shape of the API: the store becomes a cloneable handle with synchronous in-memory operations, the trait functions borrow the store instead of taking an `Arc`, and the trait no longer uses `async_trait`. The mapping of fields to keys, the semantics of loading and saving, the derive attributes and the runtime error messages do not change. TOML files need no changes if users preserve their existing section names explicitly. The default section name changes, with a temporary compile-time guard against silently changing the section of a `Confer*` struct (N7).

It amends the following parts of earlier RFCs, whose text stays unchanged (see [Documentation deliverables](#documentation-deliverables)):

- [RFC-0001](./RFC-0001_baseline_architecture.md): the [canonical type aliases and trait surface](./RFC-0001_baseline_architecture.md#canonical-type-aliases-and-trait-surface), the summaries of the [core crate](./RFC-0001_baseline_architecture.md#core-crate-conferencier) (including the `with-derive` feature) and of the [derive crate](./RFC-0001_baseline_architecture.md#derive-crate-conferencier-derive), the [asynchronous execution model](./RFC-0001_baseline_architecture.md#asynchronous-execution-model), and the default section rule in the [derive macro architecture](./RFC-0001_baseline_architecture.md#conferencier-derive-procedural-macro);
- [RFC-0003](./RFC-0003_field_attributes.md): the name `from_confer`, in particular in [A5](./RFC-0003_field_attributes.md#field-attributes);
- [RFC-0004](./RFC-0004_nested_tables.md): the names of [the `ConferTable` trait](./RFC-0004_nested_tables.md#the-confertable-trait) and its derive, the [store addition `set_section_table`](./RFC-0004_nested_tables.md#store-addition-set_section_table) and the trait signatures in [Changes to `ConferModule`](./RFC-0004_nested_tables.md#changes-to-confermodule).

The proposal starts from the [Public API redesign](./BACKLOG.md#technical-debt) entry of the backlog. The planned RFC-0006 (strict mode for unknown keys) and RFC-0007 (expressions in `default`) use the names from this RFC and are implemented after it, in the same release. Decisions made while drafting and reviewing are recorded in [Resolved questions](#resolved-questions).

## Synopsis

Before (0.0.4):

```rust
use conferencier::{Confer, ConferModule, SharedConferModule};

#[derive(ConferModule)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(default = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = Confer::from_file_async("app.toml").await?;
    let app: SharedConferModule<AppConfig> = AppConfig::from_confer(store.clone()).await?;

    let (task_store, task_app) = (store.clone(), app.clone());
    tokio::spawn(async move {
        task_store.load_file("app.toml").await?;
        AppConfig::load(&task_app, task_store.clone()).await
    })
    .await
    .expect("task panicked")?;

    app.write().await.port = 9090;
    AppConfig::save(&app, store.clone()).await?;
    println!("{}", store.save_str().await?);
    Ok(())
}
```

After (0.0.5):

```rust
use conferencier::{ConfigScope, ConfigStore, SharedScope};

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(default = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_file("app.toml").await?;
    let app: SharedScope<AppConfig> = AppConfig::from_store(&store)?;

    let (task_store, task_app) = (store.clone(), app.clone());
    tokio::spawn(async move {
        task_store.load_file("app.toml").await?;
        AppConfig::load(&task_app, &task_store).await
    })
    .await
    .expect("task panicked")?;

    app.write().await.port = 9090;
    AppConfig::save(&app, &store).await?;
    println!("{}", store.save_str()?);
    Ok(())
}
```

The struct, its attributes and the TOML file stay the same. Calls borrow the store instead of cloning it, `from_store` and `save_str` no longer wait for anything, and only file I/O and the scope's own lock are awaited.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [Glossary](#glossary)
- [Names](#names)
- [The store](#the-store)
- [The scope trait](#the-scope-trait)
- [Migration](#migration)
- [Compile-time diagnostics](#compile-time-diagnostics)
- [Implementation notes (non-normative)](#implementation-notes-non-normative)
- [Compatibility](#compatibility)
- [Alternatives considered](#alternatives-considered)
- [Out of scope](#out-of-scope)
- [Milestones](#milestones)
- [Testing](#testing)
- [Documentation deliverables](#documentation-deliverables)
- [Resolved questions](#resolved-questions)

## Motivation

- **The working name is part of the API.** The crate is called conferencier because `confer` was already taken on crates.io, but the public identifiers still use the working name: `Confer`, `SharedConfer`, `ConferModule`, `SharedConferModule`, `ConferTable`, `ConferError`, `from_confer` and the modules `confer_module` and `confer_table`.
- **"Module" is ambiguous.** In Rust, a module is a `mod`. The trait `ConferModule` lives in the Rust module `conferencier::confer_module`, and the documentation has to speak of "a module's section". The term also does not say what the README presents as a main use case: the struct shares runtime state between tasks through its ignored fields, not only values from the TOML file.
- **An `Arc` in every call.** `from_confer`, `load` and `save` take `SharedConfer` (`Arc<Confer>`) by value, although the derived code uses the store only during the call. Callers clone the `Arc` for every call: `AppConfig::load(&app, store.clone())`.
- **Async operations that never wait.** RFC-0001 made every method of the store async, so that its `tokio::sync::RwLock` cooperates with the scheduler and all call sites look alike. The lock is never held across an `.await`, though: every operation is a short operation on an in-memory table. For such data, Tokio's documentation recommends the locks of the standard library. The async API adds `.await` to every getter. Synchronous code can reach the store only by blocking on a future: before the runtime starts through a temporary executor, and on a runtime thread, for example in a `Drop` implementation or a synchronous callback, without a reliable way, because Tokio's `block_on` panics there.
- **`#[async_trait]`.** The macro boxes every future, and rustdoc shows the trait functions as `fn load<'life0, 'async_trait>(...) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'async_trait>>`. Return-position `impl Future` in traits is stable since Rust 1.75, below the MSRV of 1.85.
- **Inconsistent store names.** `from_string` stands next to `load_str` and `save_str`. The constructor `from_file` blocks while unsuffixed methods are otherwise async, and `from_file_async` is the only `_async` suffix in the API. Sections are handled by `add_section` and `remove_section`, but also by `get_section_table` and `set_section_table`.
- **Two paths for every item.** The public modules `confer_module`, `confer_table` and `error` contain the items that the crate root re-exports; the documentation and the generated code use both paths.
- **Feature name.** Crates such as serde and clap call the feature that enables their derive macros `derive`. `with-derive = ["conferencier-derive"]` also creates an implicit feature named `conferencier-derive`.
- **Timing.** Almost every public name changes. At 0.0.x, breaking changes are expected (0.0.3 and 0.0.4 both had them), and one migration now is cheaper than several later.

## Goals and non-goals

Goals:

- Final names for all public items, without the working name and with one term for the structs that derive the main trait.
- One breaking migration where removed names and changed signatures are caught by the compiler, and the temporary N7 guard forces affected derives to choose their section explicitly. The compiler cannot verify that the chosen section matches the user's TOML file.
- A store API that is synchronous where nothing waits and borrowed where nothing is kept.
- The field mapping, the semantics of `load` and `save` ([RFC-0004](./RFC-0004_nested_tables.md) R1–R7), the attributes and the runtime error messages stay unchanged. The deliberate behavior change is the default section name (N7); existing TOML sections can be preserved with an explicit attribute.

Non-goals:

- New features. Strict mode and expressions in `default` get their own RFCs; other ideas stay in the [backlog](./BACKLOG.md).
- Deprecated aliases or a compatibility layer for the old API (see [Alternatives considered](#alternatives-considered), item 6). N7's temporary diagnostic does not preserve the old naming rule.
- Changes to the `#[confer(...)]` attribute and its arguments.

## Glossary

- **Store** — a `ConfigStore`: a handle to an in-memory TOML document that is shared by all its clones. Previously `Confer` behind `SharedConfer`.
- **Scope** — a struct implementing `ConfigScope`, usually through the derive. A scope belongs to one component of the application and owns one top-level section: its loaded fields map to keys of the section, its ignored fields hold runtime state. Previously "module".
- **Scope handle** — a `SharedScope<T>` (`Arc<tokio::sync::RwLock<T>>`), cloned to every task that uses the scope. Previously `SharedConferModule<T>`.
- **Table type** — a type implementing `ConfigTable`, as defined in RFC-0004 for `ConferTable`.

## Names

- **N1 — Store: `ConfigStore`.** `Confer` becomes `ConfigStore`. `SharedConfer` is removed, because the store itself is the shared handle (S1).
- **N2 — Scope: `ConfigScope`.** The trait and the derive `ConferModule` become `ConfigScope`, `SharedConferModule<T>` becomes `SharedScope<T>` and `from_confer` becomes `from_store`. `load` and `save` keep their names: `reload_from_store` or `save_to_store` would repeat what the argument type already says. The choice of "scope" is explained in [Resolved questions](#resolved-questions).
- **N3 — Tables and errors.** The trait and the derive `ConferTable` become `ConfigTable`; `from_table` and `to_table` keep their names and signatures. `ConferError` becomes `Error`, next to the existing `Result` alias, like `std::io::Error` and `std::io::Result`. Its variants, fields and `Display` output stay the same, as do the constructors `missing_key`, `type_mismatch` and `value_parse`. `value_parse_owned` is removed, because `value_parse` already accepts a `String` through `impl Into<String>`.
- **N4 — Only the crate root is public.** The modules `confer_module`, `confer_table` and `error` become private (`store` already is). Every public item is reachable from the crate root only, so the documentation, compiler messages and generated code all use one path. `conferencier::toml` and `conferencier::Datetime` stay. `__private` stays hidden and is not part of the API.
- **N5 — Feature `derive`.** `with-derive` becomes `derive`, declared as `derive = ["dep:conferencier-derive"]`, so no implicit `conferencier-derive` feature exists. It stays enabled by default.
- **N6 — Unchanged.**
  - The attribute `#[confer(...)]` and all its arguments. It is the crate-specific derive helper attribute. Like `#[serde(...)]`, it abbreviates the crate name, while a generic name such as `#[config(...)]` could be read by another derive on the same struct.
  - `Result`, `load`, `save`, `from_table`, `to_table`, the typed getters and setters, and the error constructors listed in N3.
- **N7 — The default section name is the struct name.** Without `section`, `ConfigScope` uses the full struct identifier as the section name, without a raw identifier's `r#` prefix: `AppConfig` owns `[AppConfig]`. An explicit `section` is used as written. The old rule that stripped a leading `Confer` word is removed.
Ok  - **Temporary migration guard.** From 0.0.5 through the last 0.1.x release, `#[derive(ConfigScope)]` rejects an implicit section if the unraw'd struct name starts with `Confer` immediately followed by an uppercase Unicode character. This is exactly the old stripping condition: `ConferApp` and `r#ConferApp` are affected; `ConferenceSettings`, `Confer` and `Confer_App` are not. An explicit `#[confer(section = "...")]` is accepted for any struct name. The guard applies only to `ConfigScope`, not to a `ConfigTable`-only derive.
  - The compile error explains the old and new names and asks the user to select a section explicitly: for `ConferApp`, `#[confer(section = "App")]` preserves the old mapping, while `#[confer(section = "ConferApp")]` selects the full name.
  - The guard is removed in 0.2.0, so it covers both the remaining 0.0.x releases and the whole 0.1.x series, including users who migrate only once 0.1.0 is out. After its removal, `ConferApp` without an explicit section uses `[ConferApp]`. The 0.2.0 migration notes must warn that an upgrade directly from 0.0.4 bypasses the guard; users who need the old mapping must add an explicit section.

The documentation uses "scope" instead of "module" throughout, including the messages of the derive.

## The store

- **S1 — `ConfigStore` is a handle.** `ConfigStore` holds its table in an `Arc` and implements `Clone`, `Debug` and `Default`. Constructors return `ConfigStore`, and the store is shared with `store.clone()`, like `reqwest::Client` or `sqlx::Pool`.
  - A clone shares the data instead of copying it: a value set through one clone is visible through all of them, and `load_str` or `load_file` replaces the document for all of them. The rustdoc of `ConfigStore` says so. This is what cloning the `Arc` behind `SharedConfer` does today; the type now carries that meaning itself.
  - `ConfigStore::new()` and `ConfigStore::default()` create a new, empty store.
  - No `Arc` remains in the public API of the store, and no `into_shared` helper is needed.
- **S2 — In-memory operations are synchronous.** Every operation that works only on the in-memory document is a synchronous method: the constructors `new` and `from_str`, `load_str`, `save_str`, the section and value accessors, and the typed getters and setters. Only file I/O stays async: `from_file`, `load_file` and `save_file` use `tokio::fs`. This replaces the [asynchronous execution model](./RFC-0001_baseline_architecture.md#asynchronous-execution-model) of RFC-0001, whose two reasons no longer hold:
  - *Lock acquisition.* The lock of the store is held only for operations on the in-memory document, never across an `.await` and never while user code runs (S5). Acquisition blocks a worker thread when contended. The choice assumes relatively small documents and low contention, in line with [Tokio's guidance on synchronous locks](https://tokio.rs/tokio/tutorial/shared-state#on-using-stdsyncmutex-and-tokiosyncmutex); it does not promise bounded waiting time or constant-time critical sections.
  - *Uniform call sites.* The derived code no longer needs async calls to read or replace a section. It awaits only the lock of the scope (D1). Call sites get shorter: `store.get_integer("App", "port")?`.

  This is lock hygiene inside Tokio, not a step toward other runtimes: the scope lock stays on `tokio::sync::RwLock` and file I/O on `tokio::fs`. That synchronous methods also work in code without a runtime is a side effect, not a goal (see [Resolved questions](#resolved-questions)).
- **S3 — Names of constructors and methods.** Operations on the whole document come in pairs: `load_*` replaces the document and `save_*` serializes it, with the suffix `_str` or `_file`. The constructors are:
  - `ConfigStore::new()`, an empty store;
  - `ConfigStore::from_str(source: &str) -> Result<ConfigStore>`, replacing `from_string` and completing the grid: `from_str`/`from_file` next to `load_str`/`load_file` and `save_str`/`save_file`. `ConfigStore` also implements `FromStr` with `Err = Error`, so `source.parse::<ConfigStore>()` works too. The inherent method carries `#[allow(clippy::should_implement_trait)]`: the lint exists to point authors to the trait, and here the trait is implemented and the inherent signature matches it exactly, so there is nothing to confuse;
  - `ConfigStore::from_file(path)`, async, replacing both the blocking `from_file` and `from_file_async`. There is no blocking file constructor: code that reads the configuration before the runtime starts uses `ConfigStore::from_str(&std::fs::read_to_string(path)?)` (see [Resolved questions](#resolved-questions)).

  `get_section_table` becomes `get_section`, `set_section_table` becomes `set_section` and `section_exists` becomes `has_section`, the only name in the section family that put the noun first. `add_section` becomes `ensure_section`: it creates a missing section, leaves an existing table unchanged and rejects a non-table entry. The new name makes its idempotence explicit and distinguishes it from `set_section`, which replaces the complete table. Every section operation now starts with a verb — `get_section`, `set_section`, `ensure_section`, `remove_section`, `has_section`, `list_sections`. Their types (`Option<Table>`, `Table`, `bool`) already say that a section is a table.

  `remove_section` returns `()` instead of `Result<()>`: it has no failure path, and once the method is synchronous (S2), the `?` would visibly promise an error that cannot happen. `remove_key` keeps its `Result`, because it genuinely fails with `TypeMismatch` when the entry named by `section` is not a table — the signatures differ because the failure modes differ. All other methods keep their names and signatures, apart from losing `async` (S2).
- **S4 — The absence rule, stated.** The store already distinguishes peeking from requiring; the rustdoc of `ConfigStore` now states the rule instead of leaving it implicit. Peeks report absence in the type and treat a non-table top-level entry as absent: `get_value` and `get_section` return `Option`, `has_section` returns `bool`, and `list_sections` lists only table entries. Typed getters require the key and fail with `MissingKey` or `TypeMismatch`. Operations that write into, replace or enumerate a section require the entry named `section`, if present, to be a table and fail with `TypeMismatch` otherwise: `set_value`, the typed setters, `set_section`, `ensure_section`, `remove_key` and `list_keys`. This codifies the existing behavior; no semantics change.
- **S5 — Locking.** The store uses `std::sync::RwLock`, without a new dependency.
  - No user code runs while the lock is held: the store calls no `ConfigTable` implementation or initializer under the lock and returns no guard. Callers therefore cannot deadlock on it, also not by calling the store again.
  - Every write leaves the document consistent, because it is a single map operation or replaces a whole table. If a panic in the store ever poisons the lock, later calls recover the data with `PoisonError::into_inner` instead of panicking too.
  - File operations do not hold the lock during I/O: `load_file` reads and parses the file before it replaces the document, and `save_file` serializes under a read lock and writes the file afterwards, as today.
  - Lock-held work is not constant-time: cloning a section and serializing the document scale with their contents. The design assumes relatively small configuration documents and low contention, not a latency guarantee for arbitrarily large stores. Unlike Tokio's FIFO/write-preferring `RwLock`, the standard lock provides no reader/writer priority guarantee; its policy depends on the platform.
  - If real workloads exceed these assumptions, the implementation and possibly the API must be reevaluated based on measurements. A snapshot serialized outside the lock or offloading expensive work may help, but cloning also costs time and memory. A Tokio lock is not a drop-in feature-flag replacement: awaiting acquisition would change the synchronous methods, and `blocking_read` / `blocking_write` panic when called from an async execution context. An async lock would also not offload the CPU work of serialization.

## The scope trait

- **D1 — Trait shape.**

  ```rust
  pub type SharedScope<T> = Arc<tokio::sync::RwLock<T>>;

  pub trait ConfigScope: Send + Sync + Sized + 'static {
      fn from_store(store: &ConfigStore) -> Result<SharedScope<Self>>;
      fn load(scope: &SharedScope<Self>, store: &ConfigStore)
          -> impl Future<Output = Result<()>> + Send;
      fn save(scope: &SharedScope<Self>, store: &ConfigStore)
          -> impl Future<Output = Result<()>> + Send;
  }
  ```

  - All three functions borrow the store, because the derived code uses it only during the call. An implementation that needs to keep the store can clone the handle.
  - The scope handle stays `Arc<tokio::sync::RwLock<T>>`. Application tasks hold guards of a scope across `.await`, which is what the async lock is for.
  - The semantics are those of RFC-0004 and RFC-0003: whole-section snapshots, atomic `load` and `save`, ignored fields initialized once, in `from_store`. The rule that a task must not call `load` or `save` while it holds a guard of the same scope stays as well.
- **D2 — `from_store` is synchronous.** With S2, constructing a scope waits for nothing: it reads a snapshot of the section, converts it, evaluates the initializers of ignored fields and wraps the result in a new lock. A scope can therefore be created outside a runtime. `load` and `save` stay async, because they lock the scope. Consequence for later extensions: a hook that `from_store` runs, as in the backlog idea "Hooks around load and save", has to be synchronous.
- **D3 — No `async_trait`.** The trait uses return-position `impl Future`.
  - The `+ Send` bound lets generic code spawn the futures, for example `tokio::spawn(async move { D::load(&scope, &store).await })`. An `async fn` in a public trait cannot promise `Send`, and rustc warns about it (`async_fn_in_trait`).
  - Implementations can still write `async fn load(...)`; the compiler checks that the future is `Send`. The derived code does so.
  - The `async-trait` dependency and its re-export in `__private` are removed, so manual implementations no longer use `#[async_trait]`.

## Migration

| 0.0.4 | 0.0.5 |
| --- | --- |
| `Confer` | `ConfigStore` |
| `SharedConfer` (`Arc<Confer>`) | `ConfigStore` |
| `Confer::new()` | `ConfigStore::new()` |
| `Confer::from_string(source)?` | `ConfigStore::from_str(source)?` or `source.parse::<ConfigStore>()?` |
| `Confer::from_file(path)?` (blocking) | `ConfigStore::from_file(path).await?`; outside a runtime `ConfigStore::from_str(&std::fs::read_to_string(path)?)?` |
| `Confer::from_file_async(path).await?` | `ConfigStore::from_file(path).await?` |
| `store.get_section_table(section).await` | `store.get_section(section)` |
| `store.set_section_table(section, table).await?` | `store.set_section(section, table)?` |
| `store.section_exists(section).await` | `store.has_section(section)` |
| `store.add_section(section).await?` | `store.ensure_section(section)?` |
| `store.remove_section(section).await?` | `store.remove_section(section)` — returns `()` |
| other in-memory methods, e.g. `store.get_integer(section, key).await?` | the same name without `.await`: `store.get_integer(section, key)?` |
| `store.load_file(path).await?`, `store.save_file(path).await?` | unchanged |
| `ConferModule` (trait and derive) | `ConfigScope` |
| `SharedConferModule<T>` | `SharedScope<T>` |
| `T::from_confer(store.clone()).await?` | `T::from_store(&store)?` |
| `T::load(&module, store.clone()).await?` | `T::load(&scope, &store).await?` |
| `T::save(&module, store.clone()).await?` | `T::save(&scope, &store).await?` |
| `ConferTable` (trait and derive) | `ConfigTable` |
| `ConferError` | `Error` |
| `ConferError::value_parse_owned(...)` | `Error::value_parse(...)` |
| paths through `conferencier::confer_module`, `conferencier::confer_table`, `conferencier::error` | the crate root, e.g. `conferencier::ConfigScope` |
| feature `with-derive` | feature `derive` |
| struct `ConferApp` relying on the default section `[App]` | add `#[confer(section = "App")]` to keep it, or add `#[confer(section = "ConferApp")]` and rename the TOML section to `[ConferApp]` (N7) |

- Existing `#[confer(...)]` attributes and TOML files need no changes unless a scope is affected by N7. For such a scope, explicitly choosing the old name preserves its TOML section; choosing the full struct name instead also requires updating the TOML file.
- Removed names, changed signatures and an affected `ConfigScope` derive without an explicit section cause errors until migrated. The guard cannot check whether a chosen section matches the actual TOML file. It does not apply to manual trait implementations and, once removed in 0.2.0, will not protect an upgrade that skips the guarded releases.
- A manual `ConferTable` implementation only needs the new names. A manual `ConferModule` implementation drops `#[async_trait]`, makes `from_store` synchronous and keeps `async fn` for `load` and `save`.

## Compile-time diagnostics

The derive keeps all its existing checks, with updated names in their messages, and adds the temporary N7 migration diagnostic:

- ``unsupported field type `...`; nested structs require #[confer(table)] on the field and #[derive(ConfigTable)] on the type``;
- the `on_unimplemented` message of `ConfigTable`: `` `{Self}` cannot be used as a nested configuration table ``, with the note ``derive it with `#[derive(ConfigTable)]` or implement `ConfigTable` manually``;
- `unsupported attribute on struct for #[derive(ConfigScope)]`;
- an implicit section on `ConferApp` is rejected with a message explaining both `#[confer(section = "App")]` and `#[confer(section = "ConferApp")]` (N7).

The exact wording is fixed by the `trybuild` snapshots.

## Implementation notes (non-normative)

- **Core** (`conferencier`):
  - `store.rs`: `ConfigStore { table: Arc<std::sync::RwLock<Table>> }` with private helpers that lock and recover from poisoning (S5). The in-memory methods and the section snapshot used by the derive lose `async`.
  - `lib.rs`: the modules become private and their items are re-exported at the root. The derive macros and the crate documentation from `README.md` depend on `cfg(feature = "derive")`. In `__private`, `read_section` becomes synchronous, `new_shared_module` becomes `new_shared_scope`, and `async_trait` is removed.
  - Source files follow the new terms: `confer_module.rs` → `scope.rs` and `confer_table.rs` → `table.rs`, declared as the private `mod scope;` and `mod table;`. `store.rs`, `error.rs`, `table_path.rs` and `value_conversion.rs` keep their names, which carry no working name. Renames go through `git mv`, so the history stays followable.
  - `error.rs`: `Error` without `value_parse_owned`.
  - `Cargo.toml`: the feature `derive`, `required-features = ["derive"]` for the examples and the test targets that use the derive, no `async-trait`.
- **Derive** (`conferencier-derive`):
  - The macros `ConfigScope` and `ConfigTable`.
  - Generated code refers to items at the crate root (`ConfigScope`, `ConfigTable`, `ConfigStore`, `SharedScope`, `Error`) and calls `value_parse` instead of `value_parse_owned`.
  - `from_store` is generated as a plain function, `load` and `save` as `async fn`. The generated parameter `module` becomes `scope`.
  - The default section is the unraw'd struct name; a separate, temporary validation retains the old prefix predicate only for the N7 diagnostic.
  - Internal names such as `model::Module` and `DeriveKind::Module` can follow the new term.
- **Tests and examples:** `module_integration.rs` and `module_snapshots.rs` are renamed to `scope_integration.rs` and `scope_snapshots.rs`, and the `trybuild` case `pass_generic_module` becomes `pass_generic_scope`. The corresponding test targets in `Cargo.toml` are renamed and require `derive`.
- **Prototype:** a scratch crate with this trait shape compiles and runs on Rust 1.85 and passes `clippy -D warnings` on stable. It covered a generic implementation that uses `async fn` for `load` and `save`, a `from_store` called outside a runtime, a generic function that spawns `D::load` on a Tokio task, and a store behind `std::sync::RwLock` with poison recovery.

## Compatibility

- **Breaking** for users of the renamed items and changed signatures, listed as **Breaking:** in the changelog with the table from [Migration](#migration).
- During the guarded 0.0.x and 0.1.x releases, affected derives must select a section explicitly instead of silently adopting the new name. The compiler cannot validate the choice against an existing TOML document. The changelog calls N7 out as its own **Breaking:** entry, including the risk of bypassing the guard when upgrading directly to 0.2.0 or later.
- Unchanged: TOML files and section names (N7 aside), the field mapping, the semantics of `load` and `save` (RFC-0004 R1–R7), the attributes, the error variants and messages, and the MSRV (1.85).
- Applications that disable the default features and enable `with-derive` explicitly rename it to `derive`. Users who depend on `conferencier-derive` directly change only the macro names.
- Dependencies: `async-trait` is removed. Tokio is still required, for the scope lock and for file I/O.

## Alternatives considered

1. **Constructors return `Arc<ConfigStore>`, as today.** The typical use needs no `Arc::new`, but every store is an `Arc`, also in a single task, inside an application state that is already shared, or in a `static`. It is also inconsistent with `Default`, which returns the bare store.
2. **Constructors return a bare `ConfigStore`**, which the application wraps in an `Arc` once. This is conventional and flexible, because an `Arc` can be added but not removed. However, a store is almost always shared, so most applications would write `Arc::new` and have `Arc<ConfigStore>` in their signatures again. The handle (S1) serves this common case; an application with a single task pays only for one reference count.
3. **Keep the async store** of RFC-0001. It would leave room for store operations that wait, such as reloading from a remote source, which is outside the scope of the crate. It keeps the costs described in [Motivation](#motivation), and changing it later would break the API again.
4. **Keep `#[async_trait]`.** It works, but boxes every future, keeps a dependency and makes the trait hard to read in rustdoc, while the MSRV supports the native form.
5. **`async fn` in the trait without a `Send` bound.** Shorter, but generic code could not spawn the futures, and rustc warns about it.
6. **Deprecated aliases** such as `#[deprecated] pub type Confer = ConfigStore;`. Aliases cannot cover the derive macros, the feature name or the changed signatures (`&ConfigStore`, synchronous methods), so old code would not compile anyway. Stable procedural macros cannot emit deprecation warnings (see RFC-0003). At 0.0.x, one migration table is simpler.
7. **`parse` as the string constructor**, following `Url::parse` and avoiding clippy's `should_implement_trait` lint on an inherent `from_str`. It breaks the symmetry of `from_str`/`from_file` with `load_str`/`load_file` and `save_str`/`save_file`, and the lint's concern does not apply here: the `FromStr` implementation exists and the inherent method has the same signature, so one documented `#[allow]` is cheaper than the asymmetry. Verified against `clippy -D warnings` on stable and against Rust 1.85.
8. **Renaming the attribute**, for example to `#[config(...)]` or `#[scope(...)]`. A generic name can collide with another derive on the same struct (N6).
9. **Keeping the public modules** as alternative paths. Two paths for every item, used inconsistently by the documentation and the generated code.
10. **A newtype scope handle with methods**, such as `app.load(&store).await`. It would have to forward or hide the API of the lock, while the alias keeps `read()` and `write()` directly available. It can be proposed later.
11. **Scopes as plain values.** With the synchronous store, `from_store` could return a bare `Self`, with synchronous `load(&mut self, &ConfigStore)` and `save(&self, &ConfigStore)`, and sharing would be up to the application (`Arc::new(RwLock::new(AppConfig::from_store(&store)?))`). The trait would lose async entirely, the rule about guards held while calling `load` or `save` would disappear, any lock would work, and it would match how serde and most configuration crates return values. Rejected: sharing a scope between Tokio tasks through one `SharedScope<T>` handle is a key feature of the crate, not something to push onto the application, and the enforced guarding is intentional. `load` would also convert while the caller holds the write lock, and the documented model of runtime state in a shared scope, plus most documentation and tests, would need a rewrite.
12. **Synchronous core methods with shared async helpers.** Keep the shared handle, but require methods such as `load_from(&mut self, &ConfigStore)` and `save_to(&self, &ConfigStore)` and provide async wrappers that lock and delegate. This would let a caller modify and save through an already-held guard without recursively locking. It would also enlarge the trait and, in that form, hold the scope lock during store access and conversion, contrary to RFC-0004 R4's lock discipline. A failed save would not roll back a preceding mutation. Rejected for now without a concrete need for that stronger coordination; D1 keeps the existing staged load and save.

## Out of scope

- Strict mode for unknown keys (RFC-0006) and expressions in `default` (RFC-0007), planned for the same release.
- The other ideas in the [backlog](./BACKLOG.md), such as path-based getters or structured error paths.
- New store capabilities, such as a method that copies a store, or blocking file I/O (a blocking constructor or blocking variants of `load_file` and `save_file`).
- The unused `serde` dependency, which is listed in the backlog as technical debt and is removed separately.
- The `save_file` race on its fixed-name temporary file, a pre-existing bug listed in the backlog as technical debt. Its fix changes no API and is scheduled independently of this RFC.

## Milestones

1. **Core** — the store handle and the synchronous store (S1–S5), the renamed items, private modules, `Error` and the feature; unit tests.
2. **Trait and derive** — D1–D3, generated paths and diagnostics; `trybuild` snapshots.
3. **Tests and examples** — migrated, plus the new tests from [Testing](#testing).
4. **Documentation** — see [Documentation deliverables](#documentation-deliverables); RFC status → Implemented. An implementation report is written only if the implementation has to depart from this RFC.

All milestones land in 0.0.5; RFC-0006 and RFC-0007 follow on top of them.

## Testing

- **Migrated:** all existing tests, examples and doctests use the new names. Assertions remain unchanged where behavior is unchanged; section-name tests are updated for N7.
- **New tests:**
  - Clones of a `ConfigStore` share data: a value set through a clone is visible through the original, and `load_str` through one clone replaces the document for all clones.
  - `ConfigStore::from_str` and `str::parse::<ConfigStore>()` produce equal stores and the same `Error::Parse` for invalid input.
  - A missing file makes `from_file` return `Error::Io` carrying the path.
  - Store operations and `from_store` work in a plain `#[test]` without a Tokio runtime.
  - A generic function that spawns `D::load` for any `D: ConfigScope` on a Tokio task compiles and runs (D3).
  - The hygiene tests in `scope_integration.rs` and `nested_tables.rs`, which use fields named like the generated locals (`module`, `store`, ...), also get a field named `scope`, the new name of the generated parameter.
  - N7: `ConferApp` without `section` fails to compile with both migration choices in the diagnostic; an explicit `App` or `ConferApp` section loads and saves the chosen section.
  - N7 boundaries: `ConferenceSettings`, `Confer` and `Confer_App` remain valid without an explicit section; `r#ConferApp`, `ConferConferApp` and `ConferÉtat` are guarded. A `ConfigTable`-only derive is not guarded; a struct deriving both traits is guarded by `ConfigScope`.
- **`trybuild`:** snapshots are regenerated with `TRYBUILD=overwrite` and reviewed; the passing cases use the new names.
- **CI gates:** unchanged — `cargo fmt`, `clippy -D warnings`, the test jobs including `--no-default-features`, `cargo doc` with `-D warnings`, the MSRV job and the publish dry-run.

## Documentation deliverables

- Root `README.md` and `conferencier/README.md` (which runs as a doctest):
  - The new names and the term "scope" throughout, including the highlights, the quick start, the derive attributes and troubleshooting.
  - The default section name and temporary migration guard, with both explicit section choices and the planned removal in 0.2.0 (N7).
  - "Reloading configuration": the example takes `&ConfigStore` and `&SharedScope<D>`.
  - "Configuration and runtime state": the example and the paragraph about guards of the scope lock. The store hands out no guards, so the paragraph needs no counterpart for the store.
  - The store as a shared handle: a clone shares the data.
  - The feature `derive`.
  - Explain `#[confer(...)]` as the crate-specific derive helper attribute, not the old type naming convention.
- `conferencier-derive/README.md`: the macro names and the feature.
- Rustdoc of `ConfigStore` (handle semantics, which methods are async and why, the absence rule of S4), `ConfigScope`, `SharedScope`, `ConfigTable` and `Error`.
  - State S5's document-size, contention and fairness assumptions, including that changing to an async lock is not a transparent backend switch.
  - In `ConfigStore::from_file`, show `ConfigStore::from_str(&std::fs::read_to_string(path)?)` as the pre-runtime loading recipe. Explain that this standard-library read does not attach the path to `Error::Io`, and show how to attach it through the public variant, without a new helper; do not restore a blocking file constructor:

    ```rust
    use conferencier::{ConfigStore, Error};

    let path = std::path::Path::new("config.toml");
    let source = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let store = ConfigStore::from_str(&source)?;
    ```
- `CHANGELOG.md` (Unreleased): **Breaking:** entries with the table from [Migration](#migration), a separate **Breaking:** entry for N7 and its temporary guard, and the removed `async-trait` dependency. The 0.2.0 release notes must also warn users upgrading directly from 0.0.4 that the guard no longer applies and that `ConferApp` without an explicit section reads `[ConferApp]`, not `[App]`.
- `.github/ISSUE_TEMPLATE/bug_report.yml`: the feature name in the "Features" dropdown.
- `docs/BACKLOG.md`: the entries use the new names, and the "Public API redesign" entry moves to Done.
- RFC-0001, RFC-0003 and RFC-0004 get RFC-0005 in their `Amended by:` header line when this RFC is marked Implemented. Their text, including the old names, stays unchanged; the amended parts are listed in [Document Scope](#document-scope) only. [IMPL-001](./IMPL_001_baseline.md) stays unchanged.

## Resolved questions

1. **Name of the scope trait** → `ConfigScope`, with `SharedScope<T>` and "scope" in the documentation. It describes a bounded part of an application's configuration and associated runtime state. "Scope" also has other meanings, such as lexical scope or configuration precedence, so the documentation must define the term concretely. The previously proposed `ConfigDomain` was reconsidered because "domain" has strong business-domain and DNS associations. Other alternatives considered were:
    - `ConfigUnit` clashes with `()` and unit structs.
    - `ConfigSection` fits the loaded fields, but suggests that the struct holds only what is in the TOML file, and `#[confer(section = ...)]` on a section reads as a tautology.
    - `ConfigNode` suggests a tree, is hard to tell apart from nested tables and means a cluster node in networked applications.
    - `ConfigRealm` is overblown and an established term for authentication realms.
    - `ConfigPart` is neutral, but says nothing about ownership of a section or about shared state.
    - `Configurable` reads well as `#[derive(Configurable)]`, but the documentation still needs a noun for the struct, and `SharedConfigurable<T>` is awkward.
    - `ConfigModule` only drops the working name and keeps the clash with Rust modules.
2. **Synchronous store** → accepted as proposed (S2, D2). It is lock hygiene inside Tokio, not generalization beyond the crate's Tokio-only, TOML-only scope: Tokio's own documentation recommends `std::sync` locks for short in-memory critical sections that are never held across an `.await`, and nothing within that scope ever waits under the store lock. Had it been rejected, the store methods would have stayed async and `from_store` would have returned `impl Future<Output = Result<SharedScope<Self>>> + Send`; the rest of this RFC would have been unaffected.
3. **Name of the string constructor** → `from_str`, not `parse`, completing the pairs `from_str`/`from_file`, `load_str`/`load_file` and `save_str`/`save_file` (see S3 for the `FromStr` implementation and the `#[allow]`; `parse` is item 7 of [Alternatives considered](#alternatives-considered)).
4. **Blocking file constructor** → none. `from_file` is async; before the runtime starts, `ConfigStore::from_str(&std::fs::read_to_string(path)?)` does the same, only without the path in `Error::Io`, which such code can attach itself. A `from_file_blocking` would have been the only blocking file operation, with no blocking `load_file` or `save_file` next to it, and keeping the status-quo names would have kept the sole `_async` suffix of the API.
5. **Scopes as plain values** → rejected; the trait keeps the shape of D1, and a scope stays a shared handle. The enforced guarding through the Tokio lock is intentional, and sharing a scope between tasks is a key feature of the crate (see [Alternatives considered](#alternatives-considered), item 11). This had to be decided in this RFC either way, because a later change would break the same trait a second time.
6. **Default section name** → the stripping rule is removed (N7); without `section`, the section is the struct name, subject to the temporary migration guard. The old rule needed a word-boundary fix and edge-case notes (`ConferApp` → `App`, but `ConferenceSettings` and `Confer` stay intact). Keeping it was rejected because the naming convention it served is gone. Stripping a trailing `Config` word instead (`AppConfig` → `[App]`) was rejected because it would rebuild the same confusion and silently rename more sections.
7. **Final naming sweep** → a review of the whole public surface against this RFC produced two changes and one written-down rule: `section_exists` became `has_section`, the only noun-first name in the verb-first section family; `remove_section` returns `()`, because it has no failure path, while `remove_key` keeps its honest `Result`; and the peek-versus-require absence rule is stated in S4 and in the rustdoc of `ConfigStore`. Subsequent feedback added `set_section` to that rule and changed `add_section` to `ensure_section` to make its idempotence explicit (S3). Checked and deliberately kept: the `_vec` suffix of the typed accessors (the value type is named after TOML — `string`, `integer`, `boolean` — and the container after Rust), `to_table` next to `save` (a pure conversion versus a write into the store), `list_sections` without a `Result` next to `list_keys` with one (sections cannot fail, keys require a table), `save_str` returning `Result<String>` (serialization can fail), and the `Error` variants `Parse` (document) versus `ValueParse` (value).
8. **Synchronous core with shared async helpers** → not added without a concrete need to modify and save under one guard. This is separate from rejecting plain-value scopes; the trade-offs are recorded in [Alternatives considered](#alternatives-considered), item 12.
9. **Migration guard** → require explicit section names only for derives affected by the old `Confer` prefix rule from 0.0.5 through 0.1.x, then remove the guard in 0.2.0 (N7). Review feedback asked to keep it longer than the originally planned removal in 0.1.0, so that users who migrate only to 0.1.0 are still protected; the guard costs one compile-time check. Requiring `section` for every scope was rejected. Direct upgrades from 0.0.4 to 0.2.0 or later must be warned separately because they skip the guard.
