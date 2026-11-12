# RFC-0005: Final API names and the store handle

Date: 2026-11-12
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.5
Amends: [RFC-0001](./RFC-0001_baseline_architecture.md), [RFC-0003](./RFC-0003_field_attributes.md), [RFC-0004](./RFC-0004_nested_tables.md)

## Document Scope

This RFC sets the final public names of conferencier while the crate is at 0.0.x. Where the old names carry old design decisions, it also changes the shape of the API: the store becomes a cloneable handle with synchronous in-memory operations, the trait functions borrow the store instead of taking an `Arc`, and the trait no longer uses `async_trait`. Every user of the crate is affected. TOML files, the mapping of fields to keys, the semantics of loading and saving, the derive attributes and the error messages do not change, with one exception: a struct whose name starts with a `Confer` word and that has no explicit `section` now owns a section named after the full struct name (N7).

It amends the following parts of earlier RFCs, whose text stays unchanged (see [Documentation deliverables](#documentation-deliverables)):

- [RFC-0001](./RFC-0001_baseline_architecture.md): the [canonical type aliases and trait surface](./RFC-0001_baseline_architecture.md#canonical-type-aliases-and-trait-surface), the summaries of the [core crate](./RFC-0001_baseline_architecture.md#core-crate-conferencier) (including the `with-derive` feature) and of the [derive crate](./RFC-0001_baseline_architecture.md#derive-crate-conferencier-derive), the [asynchronous execution model](./RFC-0001_baseline_architecture.md#asynchronous-execution-model), and the default section rule in the [derive macro architecture](./RFC-0001_baseline_architecture.md#conferencier-derive-procedural-macro);
- [RFC-0003](./RFC-0003_field_attributes.md): the name `from_confer`, in particular in [A5](./RFC-0003_field_attributes.md#field-attributes);
- [RFC-0004](./RFC-0004_nested_tables.md): the names of [the `ConferTable` trait](./RFC-0004_nested_tables.md#the-confertable-trait) and its derive, the [store addition `set_section_table`](./RFC-0004_nested_tables.md#store-addition-set_section_table) and the trait signatures in [Changes to `ConferModule`](./RFC-0004_nested_tables.md#changes-to-confermodule).

The proposal starts from the [API names](./BACKLOG.md#technical-debt) entry of the backlog. The planned RFC-0006 (strict mode for unknown keys) and RFC-0007 (expressions in `default`) use the names from this RFC and are implemented after it, in the same release. Every question raised while drafting has been decided; see [Resolved questions](#resolved-questions).

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
use conferencier::{ConfigDomain, ConfigStore, SharedDomain};

#[derive(ConfigDomain)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(default = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_file("app.toml").await?;
    let app: SharedDomain<AppConfig> = AppConfig::from_store(&store)?;

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

The struct, its attributes and the TOML file stay the same. Calls borrow the store instead of cloning it, `from_store` and `save_str` no longer wait for anything, and only file I/O and the domain's own lock are awaited.

## Table of Contents

- [Document Scope](#document-scope)
- [Synopsis](#synopsis)
- [Motivation](#motivation)
- [Goals and non-goals](#goals-and-non-goals)
- [Glossary](#glossary)
- [Names](#names)
- [The store](#the-store)
- [The domain trait](#the-domain-trait)
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
- One breaking migration that the compiler detects almost completely: every old name is removed, so no code written for 0.0.4 compiles until it is migrated. The single change the compiler cannot flag is the default section name of `Confer*` structs (N7).
- A store API that is synchronous where nothing waits and borrowed where nothing is kept.
- TOML files, the field mapping, the semantics of `load` and `save` ([RFC-0004](./RFC-0004_nested_tables.md) R1–R7), the attributes and the error messages stay unchanged. The one deliberate behavior change is the default section name (N7).

Non-goals:

- New features. Strict mode and expressions in `default` get their own RFCs; other ideas stay in the [backlog](./BACKLOG.md).
- Deprecated aliases or a transition period (see [Alternatives considered](#alternatives-considered), item 6).
- Changes to the `#[confer(...)]` attribute and its arguments.

## Glossary

- **Store** — a `ConfigStore`: a handle to an in-memory TOML document that is shared by all its clones. Previously `Confer` behind `SharedConfer`.
- **Domain** — a struct implementing `ConfigDomain`, usually through the derive. A domain belongs to one component of the application and owns one top-level section: its loaded fields map to keys of the section, its ignored fields hold runtime state. Previously "module".
- **Domain handle** — a `SharedDomain<T>` (`Arc<tokio::sync::RwLock<T>>`), cloned to every task that uses the domain. Previously `SharedConferModule<T>`.
- **Table type** — a type implementing `ConfigTable`, as defined in RFC-0004 for `ConferTable`.

## Names

- **N1 — Store: `ConfigStore`.** `Confer` becomes `ConfigStore`. `SharedConfer` is removed, because the store itself is the shared handle (S1).
- **N2 — Domain: `ConfigDomain`.** The trait and the derive `ConferModule` become `ConfigDomain`, `SharedConferModule<T>` becomes `SharedDomain<T>` and `from_confer` becomes `from_store`. `load` and `save` keep their names: `reload_from_store` or `save_to_store` would repeat what the argument type already says. The choice of the term "domain" is explained in [Resolved questions](#resolved-questions).
- **N3 — Tables and errors.** The trait and the derive `ConferTable` become `ConfigTable`; `from_table` and `to_table` keep their names and signatures. `ConferError` becomes `Error`, next to the existing `Result` alias, like `std::io::Error` and `std::io::Result`. Its variants, fields and `Display` output stay the same, as do the constructors `missing_key`, `type_mismatch` and `value_parse`. `value_parse_owned` is removed, because `value_parse` already accepts a `String` through `impl Into<String>`.
- **N4 — Only the crate root is public.** The modules `confer_module`, `confer_table` and `error` become private (`store` already is). Every public item is reachable from the crate root only, so the documentation, compiler messages and generated code all use one path. `conferencier::toml` and `conferencier::Datetime` stay. `__private` stays hidden and is not part of the API.
- **N5 — Feature `derive`.** `with-derive` becomes `derive`, declared as `derive = ["dep:conferencier-derive"]`, so no implicit `conferencier-derive` feature exists. It stays enabled by default.
- **N6 — Unchanged.**
  - The attribute `#[confer(...)]` and all its arguments. Like `#[serde(...)]`, it abbreviates the crate name, while a generic name such as `#[config(...)]` could be read by another derive on the same struct.
  - `Result`, `load`, `save`, `from_table`, `to_table`, the typed getters and setters, and the error constructors listed in N3.
- **N7 — The default section name is the struct name.** Without `section`, the derive uses the struct name verbatim: `AppConfig` owns `[AppConfig]`. The rule that stripped a leading `Confer` word (`ConferApp` → `[App]`, with a word-boundary check so that `ConferenceSettings` stayed intact) is removed together with the working-name convention it was built for. One rule remains, with no edge cases: the section is the struct name, or exactly what `section = "..."` says. For structs named after the old convention this changes the owned section, which the compiler cannot flag; see [Migration](#migration).

The documentation uses "domain" instead of "module" throughout, including the messages of the derive.

## The store

- **S1 — `ConfigStore` is a handle.** `ConfigStore` holds its table in an `Arc` and implements `Clone`, `Debug` and `Default`. Constructors return `ConfigStore`, and the store is shared with `store.clone()`, like `reqwest::Client` or `sqlx::Pool`.
  - A clone shares the data instead of copying it: a value set through one clone is visible through all of them, and `load_str` or `load_file` replaces the document for all of them. The rustdoc of `ConfigStore` says so. This is what cloning the `Arc` behind `SharedConfer` does today; the type now carries that meaning itself.
  - `ConfigStore::new()` and `ConfigStore::default()` create a new, empty store.
  - No `Arc` remains in the public API of the store, and no `into_shared` helper is needed.
- **S2 — In-memory operations are synchronous.** Every operation that works only on the in-memory document is a synchronous method: the constructors `new` and `from_str`, `load_str`, `save_str`, the section and value accessors, and the typed getters and setters. Only file I/O stays async: `from_file`, `load_file` and `save_file` use `tokio::fs`. This replaces the [asynchronous execution model](./RFC-0001_baseline_architecture.md#asynchronous-execution-model) of RFC-0001, whose two reasons no longer hold:
  - *Lock acquisition.* The lock of the store is held only for a map operation, a clone of a section or the serialization of the document, never across an `.await` and never while user code runs (S5). Waiting for it blocks a worker thread for at most that long, which Tokio's documentation considers fine for such locks.
  - *Uniform call sites.* The derived code no longer needs async calls to read or replace a section. It awaits only the lock of the domain (D1). Call sites get shorter: `store.get_integer("App", "port")?`.

  This is lock hygiene inside Tokio, not a step toward other runtimes: the domain lock stays on `tokio::sync::RwLock` and file I/O on `tokio::fs`. That synchronous methods also work in code without a runtime is a side effect, not a goal (see [Resolved questions](#resolved-questions)).
- **S3 — Names of constructors and methods.** Operations on the whole document come in pairs: `load_*` replaces the document and `save_*` serializes it, with the suffix `_str` or `_file`. The constructors are:
  - `ConfigStore::new()`, an empty store;
  - `ConfigStore::from_str(source: &str) -> Result<ConfigStore>`, replacing `from_string` and completing the grid: `from_str`/`from_file` next to `load_str`/`load_file` and `save_str`/`save_file`. `ConfigStore` also implements `FromStr` with `Err = Error`, so `source.parse::<ConfigStore>()` works too. The inherent method carries `#[allow(clippy::should_implement_trait)]`: the lint exists to point authors to the trait, and here the trait is implemented and the inherent signature matches it exactly, so there is nothing to confuse;
  - `ConfigStore::from_file(path)`, async, replacing both the blocking `from_file` and `from_file_async`. There is no blocking file constructor: code that reads the configuration before the runtime starts uses `ConfigStore::from_str(&std::fs::read_to_string(path)?)` (see [Resolved questions](#resolved-questions)).

  `get_section_table` becomes `get_section`, `set_section_table` becomes `set_section` and `section_exists` becomes `has_section`, the only name in the section family that put the noun first: every section operation now starts with a verb — `get_section`, `set_section`, `add_section`, `remove_section`, `has_section`, `list_sections`. Their types (`Option<Table>`, `Table`, `bool`) already say that a section is a table.

  `remove_section` returns `()` instead of `Result<()>`: it has no failure path, and once the method is synchronous (S2), the `?` would visibly promise an error that cannot happen. `remove_key` keeps its `Result`, because it genuinely fails with `TypeMismatch` when the entry named by `section` is not a table — the signatures differ because the failure modes differ. All other methods keep their names and signatures, apart from losing `async` (S2).
- **S4 — The absence rule, stated.** The store already distinguishes peeking from requiring; the rustdoc of `ConfigStore` now states the rule instead of leaving it implicit. Peeks report absence in the type and treat a non-table top-level entry as absent: `get_value` and `get_section` return `Option`, `has_section` returns `bool`, and `list_sections` lists only table entries. Typed getters require the key and fail with `MissingKey` or `TypeMismatch`. Operations that write into or enumerate a section require the entry named `section`, if present, to be a table and fail with `TypeMismatch` otherwise: `set_value`, the typed setters, `add_section`, `remove_key` and `list_keys`. This codifies the existing behavior; no semantics change.
- **S5 — Locking.** The store uses `std::sync::RwLock`, without a new dependency.
  - No user code runs while the lock is held: the store calls no `ConfigTable` implementation or initializer under the lock and returns no guard. Callers therefore cannot deadlock on it, also not by calling the store again.
  - Every write leaves the document consistent, because it is a single map operation or replaces a whole table. If a panic in the store ever poisons the lock, later calls recover the data with `PoisonError::into_inner` instead of panicking too.
  - File operations do not hold the lock during I/O: `load_file` reads and parses the file before it replaces the document, and `save_file` serializes under a read lock and writes the file afterwards, as today.

## The domain trait

- **D1 — Trait shape.**

  ```rust
  pub type SharedDomain<T> = Arc<tokio::sync::RwLock<T>>;

  pub trait ConfigDomain: Send + Sync + Sized + 'static {
      fn from_store(store: &ConfigStore) -> Result<SharedDomain<Self>>;
      fn load(domain: &SharedDomain<Self>, store: &ConfigStore)
          -> impl Future<Output = Result<()>> + Send;
      fn save(domain: &SharedDomain<Self>, store: &ConfigStore)
          -> impl Future<Output = Result<()>> + Send;
  }
  ```

  - All three functions borrow the store, because the derived code uses it only during the call. An implementation that needs to keep the store can clone the handle.
  - The domain handle stays `Arc<tokio::sync::RwLock<T>>`. Application tasks hold guards of a domain across `.await`, which is what the async lock is for.
  - The semantics are those of RFC-0004 and RFC-0003: whole-section snapshots, atomic `load` and `save`, ignored fields initialized once, in `from_store`. The rule that a task must not call `load` or `save` while it holds a guard of the same domain stays as well.
- **D2 — `from_store` is synchronous.** With S2, constructing a domain waits for nothing: it reads a snapshot of the section, converts it, evaluates the initializers of ignored fields and wraps the result in a new lock. A domain can therefore be created outside a runtime. `load` and `save` stay async, because they lock the domain. Consequence for later extensions: a hook that `from_store` runs, as in the backlog idea "Hooks around load and save", has to be synchronous.
- **D3 — No `async_trait`.** The trait uses return-position `impl Future`.
  - The `+ Send` bound lets generic code spawn the futures, for example `tokio::spawn(async move { D::load(&domain, &store).await })`. An `async fn` in a public trait cannot promise `Send`, and rustc warns about it (`async_fn_in_trait`).
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
| `store.remove_section(section).await?` | `store.remove_section(section)` — returns `()` |
| other in-memory methods, e.g. `store.get_integer(section, key).await?` | the same name without `.await`: `store.get_integer(section, key)?` |
| `store.load_file(path).await?`, `store.save_file(path).await?` | unchanged |
| `ConferModule` (trait and derive) | `ConfigDomain` |
| `SharedConferModule<T>` | `SharedDomain<T>` |
| `T::from_confer(store.clone()).await?` | `T::from_store(&store)?` |
| `T::load(&module, store.clone()).await?` | `T::load(&domain, &store).await?` |
| `T::save(&module, store.clone()).await?` | `T::save(&domain, &store).await?` |
| `ConferTable` (trait and derive) | `ConfigTable` |
| `ConferError` | `Error` |
| `ConferError::value_parse_owned(...)` | `Error::value_parse(...)` |
| paths through `conferencier::confer_module`, `conferencier::confer_table`, `conferencier::error` | the crate root, e.g. `conferencier::ConfigDomain` |
| feature `with-derive` | feature `derive` |
| struct `ConferApp` relying on the default section `[App]` | add `#[confer(section = "App")]`, or rename the TOML section to `[ConferApp]` (N7) |

- `#[confer(...)]` attributes and TOML files need no changes, with the N7 exception above: a struct whose name starts with a `Confer` word and that has no `section` attribute keeps its section only by gaining an explicit one.
- Every row except the N7 row is a compile error until it is migrated. A leftover `.await` on a synchronous method is an error, because a `Result` is not a future, and so is a missing `.await` on `from_file`. A struct affected by N7 compiles unchanged, so migrating includes searching the code for `struct Confer`; every derive struct is edited in this migration anyway, because both derive macros change their names.
- A manual `ConferTable` implementation only needs the new names. A manual `ConferModule` implementation drops `#[async_trait]`, makes `from_store` synchronous and keeps `async fn` for `load` and `save`.

## Compile-time diagnostics

The derive keeps all its checks; only the names in its messages change, for example:

- ``unsupported field type `...`; nested structs require #[confer(table)] on the field and #[derive(ConfigTable)] on the type``;
- the `on_unimplemented` message of `ConfigTable`: `` `{Self}` cannot be used as a nested configuration table ``, with the note ``derive it with `#[derive(ConfigTable)]` or implement `ConfigTable` manually``;
- `unsupported attribute on struct for #[derive(ConfigDomain)]`.

The exact wording is fixed by the `trybuild` snapshots.

## Implementation notes (non-normative)

- **Core** (`conferencier`):
  - `store.rs`: `ConfigStore { table: Arc<std::sync::RwLock<Table>> }` with private helpers that lock and recover from poisoning (S5). The in-memory methods and the section snapshot used by the derive lose `async`.
  - `lib.rs`: the modules become private and their items are re-exported at the root. The derive macros and the crate documentation from `README.md` depend on `cfg(feature = "derive")`. In `__private`, `read_section` becomes synchronous, `new_shared_module` becomes `new_shared_domain`, and `async_trait` is removed.
  - Source files follow the new terms: `confer_module.rs` → `domain.rs` and `confer_table.rs` → `table.rs`, declared as the private `mod domain;` and `mod table;`. `store.rs`, `error.rs`, `table_path.rs` and `value_conversion.rs` keep their names, which carry no working name. Renames go through `git mv`, so the history stays followable.
  - `error.rs`: `Error` without `value_parse_owned`.
  - `Cargo.toml`: the feature `derive`, `required-features = ["derive"]` for the examples and the test targets that use the derive, no `async-trait`.
- **Derive** (`conferencier-derive`):
  - The macros `ConfigDomain` and `ConfigTable`.
  - Generated code refers to items at the crate root (`ConfigDomain`, `ConfigTable`, `ConfigStore`, `SharedDomain`, `Error`) and calls `value_parse` instead of `value_parse_owned`.
  - `from_store` is generated as a plain function, `load` and `save` as `async fn`. The generated parameter `module` becomes `domain`.
  - `default_section_name` reduces to the unraw'd struct name (N7).
  - Internal names such as `model::Module` and `DeriveKind::Module` can follow the new term.
- **Tests and examples:** the test targets `module_integration` and `module_snapshots` and the `trybuild` case `pass_generic_module` are renamed after the new term: `domain_integration`, `domain_snapshots`, `pass_generic_domain`. `Cargo.toml` follows for the targets with `required-features`.
- **Prototype:** a scratch crate with this trait shape compiles and runs on Rust 1.85 and passes `clippy -D warnings` on stable. It covered a generic implementation that uses `async fn` for `load` and `save`, a `from_store` called outside a runtime, a generic function that spawns `D::load` on a Tokio task, and a store behind `std::sync::RwLock` with poison recovery.

## Compatibility

- **Breaking** for every user, listed as **Breaking:** in the changelog with the table from [Migration](#migration).
- No old identifier remains, so code written for 0.0.4 does not compile until it is migrated. The one change that takes effect silently is the default section name of `Confer*` structs (N7); the changelog calls it out as its own **Breaking:** entry.
- Unchanged: TOML files and section names (N7 aside), the field mapping, the semantics of `load` and `save` (RFC-0004 R1–R7), the attributes, the error variants and messages, and the MSRV (1.85).
- Applications that disable the default features and enable `with-derive` explicitly rename it to `derive`. Users who depend on `conferencier-derive` directly change only the macro names.
- Dependencies: `async-trait` is removed. Tokio is still required, for the domain lock and for file I/O.

## Alternatives considered

1. **Constructors return `Arc<ConfigStore>`, as today.** The typical use needs no `Arc::new`, but every store is an `Arc`, also in a single task, inside an application state that is already shared, or in a `static`. It is also inconsistent with `Default`, which returns the bare store.
2. **Constructors return a bare `ConfigStore`**, which the application wraps in an `Arc` once. This is conventional and flexible, because an `Arc` can be added but not removed. However, a store is almost always shared, so most applications would write `Arc::new` and have `Arc<ConfigStore>` in their signatures again. The handle (S1) serves this common case; an application with a single task pays only for one reference count.
3. **Keep the async store** of RFC-0001. It would leave room for store operations that wait, such as reloading from a remote source, which is outside the scope of the crate. It keeps the costs described in [Motivation](#motivation), and changing it later would break the API again.
4. **Keep `#[async_trait]`.** It works, but boxes every future, keeps a dependency and makes the trait hard to read in rustdoc, while the MSRV supports the native form.
5. **`async fn` in the trait without a `Send` bound.** Shorter, but generic code could not spawn the futures, and rustc warns about it.
6. **Deprecated aliases** such as `#[deprecated] pub type Confer = ConfigStore;`. Aliases cannot cover the derive macros, the feature name or the changed signatures (`&ConfigStore`, synchronous methods), so old code would not compile anyway. Stable procedural macros cannot emit deprecation warnings (see RFC-0003). At 0.0.x, one migration table is simpler.
7. **`parse` as the string constructor**, following `Url::parse` and avoiding clippy's `should_implement_trait` lint on an inherent `from_str`. It breaks the symmetry of `from_str`/`from_file` with `load_str`/`load_file` and `save_str`/`save_file`, and the lint's concern does not apply here: the `FromStr` implementation exists and the inherent method has the same signature, so one documented `#[allow]` is cheaper than the asymmetry. Verified against `clippy -D warnings` on stable and against Rust 1.85.
8. **Renaming the attribute**, for example to `#[config(...)]` or `#[domain(...)]`. A generic name can collide with another derive on the same struct (N6).
9. **Keeping the public modules** as alternative paths. Two paths for every item, used inconsistently by the documentation and the generated code.
10. **A newtype domain handle with methods**, such as `app.load(&store).await`. It would have to forward or hide the API of the lock, while the alias keeps `read()` and `write()` directly available. It can be proposed later.
11. **Domains as plain values.** With the synchronous store, `from_store` could return a bare `Self`, with synchronous `load(&mut self, &ConfigStore)` and `save(&self, &ConfigStore)`, and sharing would be up to the application (`Arc::new(RwLock::new(AppConfig::from_store(&store)?))`). The trait would lose async entirely, the rule about guards held while calling `load` or `save` would disappear, any lock would work, and it would match how serde and most configuration crates return values. Rejected: sharing a domain between Tokio tasks through one `SharedDomain<T>` handle is a key feature of the crate, not something to push onto the application, and the enforced guarding is intentional. `load` would also convert while the caller holds the write lock, and the documented model of runtime state in a shared domain, plus most documentation and tests, would need a rewrite.

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

- **Migrated:** all existing tests, examples and doctests use the new names. Their assertions stay as they are, which shows that the behavior is unchanged.
- **New tests:**
  - Clones of a `ConfigStore` share data: a value set through a clone is visible through the original, and `load_str` through one clone replaces the document for all clones.
  - `ConfigStore::from_str` and `str::parse::<ConfigStore>()` produce equal stores and the same `Error::Parse` for invalid input.
  - A missing file makes `from_file` return `Error::Io` carrying the path.
  - Store operations and `from_store` work in a plain `#[test]` without a Tokio runtime.
  - A generic function that spawns `D::load` for any `D: ConfigDomain` on a Tokio task compiles and runs (D3).
  - The hygiene tests in `module_integration.rs` and `nested_tables.rs`, which use fields named like the generated locals (`module`, `store`, ...), also get a field named `domain`, the new name of the generated parameter.
  - The default section name is the struct name: a struct named `ConferApp` without `section` owns `[ConferApp]`. This replaces the parser's unit tests for the stripping rule.
- **`trybuild`:** snapshots are regenerated with `TRYBUILD=overwrite` and reviewed; the passing cases use the new names.
- **CI gates:** unchanged — `cargo fmt`, `clippy -D warnings`, the test jobs including `--no-default-features`, `cargo doc` with `-D warnings`, the MSRV job and the publish dry-run.

## Documentation deliverables

- Root `README.md` and `conferencier/README.md` (which runs as a doctest):
  - The new names and the term "domain" throughout, including the highlights, the quick start, the derive attributes and troubleshooting.
  - The default section name: the struct name, used verbatim (N7).
  - "Reloading configuration": the example takes `&ConfigStore` and `&SharedDomain<D>`.
  - "Configuration and runtime state": the example and the paragraph about guards of the domain lock. The store hands out no guards, so the paragraph needs no counterpart for the store.
  - The store as a shared handle: a clone shares the data.
  - The feature `derive`.
- `conferencier-derive/README.md`: the macro names and the feature.
- Rustdoc of `ConfigStore` (handle semantics, which methods are async and why, the absence rule of S4), `ConfigDomain`, `SharedDomain`, `ConfigTable` and `Error`.
- `CHANGELOG.md` (Unreleased): **Breaking:** entries with the table from [Migration](#migration), a separate **Breaking:** entry for the N7 section-name change of `Confer*` structs, and the removed `async-trait` dependency.
- `.github/ISSUE_TEMPLATE/bug_report.yml`: the feature name in the "Features" dropdown.
- `docs/BACKLOG.md`: the entries use the new names, and the "API names" entry moves to Done.
- RFC-0001, RFC-0003 and RFC-0004 get RFC-0005 in their `Amended by:` header line when this RFC is marked Implemented. Their text, including the old names, stays unchanged; the amended parts are listed in [Document Scope](#document-scope) only. [IMPL-001](./IMPL_001_baseline.md) stays unchanged.

## Resolved questions

1. **Name of the domain trait** → `ConfigDomain`, with `SharedDomain<T>` and "domain" in the documentation. The word is abstract and has other meanings, such as DNS domains or domain-driven design, but it says that the struct belongs to one component and owns one section. The alternatives were weaker:
    - `ConfigUnit` clashes with `()` and unit structs.
    - `ConfigSection` fits the loaded fields, but suggests that the struct holds only what is in the TOML file, and `#[confer(section = ...)]` on a section reads as a tautology.
    - `ConfigNode` suggests a tree, is hard to tell apart from nested tables and means a cluster node in networked applications.
    - `ConfigRealm` is overblown and an established term for authentication realms.
    - `ConfigPart` is neutral, but says nothing about ownership of a section or about shared state.
    - `Configurable` reads well as `#[derive(Configurable)]`, but the documentation still needs a noun for the struct, and `SharedConfigurable<T>` is awkward.
    - `ConfigModule` only drops the working name and keeps the clash with Rust modules.
2. **Synchronous store** → accepted as proposed (S2, D2). It is lock hygiene inside Tokio, not generalization beyond the crate's Tokio-only, TOML-only scope: Tokio's own documentation recommends `std::sync` locks for short in-memory critical sections that are never held across an `.await`, and nothing within that scope ever waits under the store lock. Had it been rejected, the store methods would have stayed async and `from_store` would have returned `impl Future<Output = Result<SharedDomain<Self>>> + Send`; the rest of this RFC would have been unaffected.
3. **Name of the string constructor** → `from_str`, not `parse`, completing the pairs `from_str`/`from_file`, `load_str`/`load_file` and `save_str`/`save_file` (see S3 for the `FromStr` implementation and the `#[allow]`; `parse` is item 7 of [Alternatives considered](#alternatives-considered)).
4. **Blocking file constructor** → none. `from_file` is async; before the runtime starts, `ConfigStore::from_str(&std::fs::read_to_string(path)?)` does the same, only without the path in `Error::Io`, which such code can attach itself. A `from_file_blocking` would have been the only blocking file operation, with no blocking `load_file` or `save_file` next to it, and keeping the status-quo names would have kept the sole `_async` suffix of the API.
5. **Domains as plain values** → rejected; the trait keeps the shape of D1, and a domain stays a shared handle. The enforced guarding through the Tokio lock is intentional, and sharing a domain between tasks is a key feature of the crate (see [Alternatives considered](#alternatives-considered), item 11). This had to be decided in this RFC either way, because a later change would break the same trait a second time.
6. **Default section name** → the stripping rule is removed (N7); without `section`, the section is the struct name, verbatim. The rule served the old `Confer*` naming convention, which this RFC abolishes, and its cost outgrew its use: it needed a word-boundary fix and edge-case notes (`ConferApp` → `App`, but `ConferenceSettings` and `Confer` stay intact), and even with them, predicting a section name meant consulting the rule — for example, `AppConfig` owns `[AppConfig]`, not `[App]`, because only a leading `Confer` was ever stripped. Keeping the rule was rejected because the convention it served is gone. Stripping a trailing `Config` word instead (`AppConfig` → `[App]`) was rejected because it would rebuild the same confusion around a far more common naming pattern and silently rename far more sections.
7. **Final naming sweep** → a review of the whole public surface against this RFC produced two changes and one written-down rule: `section_exists` became `has_section`, the only noun-first name in the verb-first section family; `remove_section` returns `()`, because it has no failure path, while `remove_key` keeps its honest `Result`; and the peek-versus-require absence rule is stated in S4 and in the rustdoc of `ConfigStore`. Checked and deliberately kept: the `_vec` suffix of the typed accessors (the value type is named after TOML — `string`, `integer`, `boolean` — and the container after Rust), `to_table` next to `save` (a pure conversion versus a write into the store), `add_section` next to `set_section` (ensure versus replace), `list_sections` without a `Result` next to `list_keys` with one (sections cannot fail, keys require a table), `save_str` returning `Result<String>` (serialization can fail), and the `Error` variants `Parse` (document) versus `ValueParse` (value).
