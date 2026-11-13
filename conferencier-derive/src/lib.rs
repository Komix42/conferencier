//! Derive macros `ConfigScope` and `ConfigTable` for
//! [conferencier](https://docs.rs/conferencier).
//!
//! Applications normally use these macros through `conferencier`, which re-exports them
//! with its default `derive` feature. The generated code calls into `conferencier`,
//! so it is always a dependency. The attribute reference is in the `conferencier`
//! documentation.

mod codegen;
mod crate_path;
mod model;
mod parser;

use proc_macro::TokenStream;
use syn::DeriveInput;

use crate::model::DeriveKind;

/// Derives `conferencier::ConfigScope` for a named-field configuration struct.
///
/// The generated implementation constructs a shared scope synchronously with
/// `from_store`, reloads loaded fields with async `load`, and replaces its whole
/// store section with async `save`. Neither the scope nor the macro starts a
/// background task or reads a file.
///
/// # Helper attributes
///
/// `#[confer(...)]` is this crate's derive helper attribute, an abbreviation of
/// *conferencier*, not a required prefix for struct names.
///
/// - Struct: `section = "App"` selects a literal top-level TOML section. Otherwise
///   the full struct name is used, without a raw identifier's `r#` prefix.
/// - Struct: `deny_unknown_keys` rejects unmapped keys in this scope's section on
///   load. It does not apply to nested table types or other sections.
/// - Fields: `rename = "key"`, `default = <literal>`, `ignore`,
///   `ignore = <expr>` and `table` control conversion and runtime-only state.
/// - A field marked `table` must use `T`, `Option<T>` or `Vec<T>` with
///   `T: conferencier::ConfigTable`.
///
/// Ignored fields directly in the scope are initialized once in `from_store`
/// and survive reloads. Unknown TOML keys are ignored on load and removed on save
/// unless `deny_unknown_keys` is set.
/// See the [attribute reference](https://docs.rs/conferencier#derive-attributes)
/// for supported scalar types, defaults and nested-table semantics.
///
/// # Compile-time checks
///
/// Unsupported struct or field shapes, incompatible attributes and duplicate
/// loaded keys produce compile errors. The struct must satisfy the trait's
/// `Send + Sync + 'static` bounds.
///
/// Through 0.1.x, a name whose old implicit section stripped a leading `Confer`
/// word, such as `ConferApp`, requires an explicit `section`. Use `"App"` to
/// preserve the old section or `"ConferApp"` to adopt the full name. This temporary
/// migration check is removed in 0.2.0.
///
/// # Example
///
/// ```
/// use conferencier::{ConfigScope as _, ConfigStore};
/// use conferencier_derive::ConfigScope;
///
/// #[derive(ConfigScope)]
/// #[confer(section = "App")]
/// struct AppConfig {
///     #[confer(default = 8080)]
///     port: u16,
///     #[confer(ignore)]
///     requests: u64,
/// }
///
/// let app = AppConfig::from_store(&ConfigStore::new())?;
/// let state = app.try_read().expect("no other guard exists");
/// assert_eq!(state.port, 8080);
/// assert_eq!(state.requests, 0);
/// # Ok::<(), conferencier::Error>(())
/// ```
#[proc_macro_derive(ConfigScope, attributes(confer))]
pub fn scope_derive(input: TokenStream) -> TokenStream {
    expand(input, DeriveKind::Scope)
}

/// Derives `conferencier::ConfigTable` for a plain nested configuration value.
///
/// The generated synchronous `from_table` and `to_table` functions convert an
/// owned TOML table into the struct and the struct into a complete table.
/// No store handle, section or lock is attached to the value; the `path` argument
/// supplies diagnostic context such as `App.server`.
///
/// # Helper attributes
///
/// Named fields accept `#[confer(rename = "key")]`, `default = <literal>`,
/// `ignore`, `ignore = <expr>` and `table`, with the same field conversion rules
/// as [`ConfigScope`]. The struct attribute `deny_unknown_keys` rejects unmapped
/// keys in this table. It applies only to this type. A table-only derive does not
/// accept `section`; a struct deriving both traits can use it for the scope
/// implementation.
///
/// Ignored fields are initialized whenever a new table value is constructed,
/// including on a scope reload. Unknown keys are ignored on load and omitted
/// from the returned table unless `deny_unknown_keys` is set. `ConfigScope` alone
/// does not implement this trait.
///
/// The temporary `Confer` prefix migration check applies only to the scope
/// implementation, not to a table-only derive.
///
/// # Example
///
/// ```
/// use conferencier::{ConfigTable as _, Error};
/// use conferencier::toml::Table;
/// use conferencier_derive::ConfigTable;
///
/// #[derive(ConfigTable)]
/// struct Server {
///     #[confer(default = "localhost")]
///     host: String,
///     port: u16,
/// }
///
/// let table: Table = "port = 8080".parse().unwrap();
/// let server = Server::from_table("App.server", table)?;
/// assert_eq!(server.host, "localhost");
/// assert_eq!(server.port, 8080);
/// assert_eq!(server.to_table("App.server")?.len(), 2);
/// # Ok::<(), Error>(())
/// ```
#[proc_macro_derive(ConfigTable, attributes(confer))]
pub fn table_derive(input: TokenStream) -> TokenStream {
    expand(input, DeriveKind::Table)
}

/// Runs the derive and turns errors into compile errors.
fn expand(input: TokenStream, derive: DeriveKind) -> TokenStream {
    match try_expand(input, derive) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error().into(),
    }
}

/// Parses the derive input and produces the final token stream.
fn try_expand(input: TokenStream, derive: DeriveKind) -> syn::Result<TokenStream> {
    let input: DeriveInput = syn::parse(input)?;
    let scope = parser::parse_scope(input, derive)?;
    let crate_path = crate_path::conferencier_path()?;
    let tokens = match derive {
        DeriveKind::Scope => codegen::generate(scope, crate_path)?,
        DeriveKind::Table => codegen::generate_table(scope, crate_path)?,
    };
    Ok(tokens.into())
}
