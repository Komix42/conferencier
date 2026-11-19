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
/// `#[confer(...)]`, short for *conferencier*, accepts `section = "Name"` and
/// `deny_unknown_keys` on the struct, and `rename = "key"`, `default`,
/// `default = <expr>`, `ignore`, `ignore = <expr>`, `table` and `field_defaults`
/// on fields. The [attribute reference](https://docs.rs/conferencier/%5E0.1.0-beta.1/conferencier/guide/derive/index.html)
/// describes their semantics and the supported field types.
///
/// # Compile-time checks
///
/// Unsupported struct or field shapes, incompatible attributes and duplicate
/// loaded keys produce compile errors. The struct must satisfy the trait's
/// `Send + Sync + 'static` bounds. Through 0.1.x, a name such as `ConferApp`
/// also requires an explicit `section`; see the
/// [migration guide](https://docs.rs/conferencier/%5E0.1.0-beta.1/conferencier/guide/migration/index.html#from-004-to-005).
///
/// # Example
///
/// ```
/// use conferencier::{ConfigScope, ConfigStore};
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
/// Fields accept the same `#[confer(...)]` attributes as [`ConfigScope`]. The struct
/// accepts `deny_unknown_keys`, which applies only to this type, and rejects
/// `section` unless it also derives `ConfigScope`. The
/// [nested tables guide](https://docs.rs/conferencier/%5E0.1.0-beta.1/conferencier/guide/tables/index.html)
/// describes table fields and how tables behave on reload.
///
/// # Example
///
/// ```
/// use conferencier::{ConfigTable, Error};
/// use conferencier::toml::Table;
///
/// #[derive(ConfigTable)]
/// struct Server {
///     #[confer(default = "localhost".into())]
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
