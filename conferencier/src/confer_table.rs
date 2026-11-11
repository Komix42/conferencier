//! Conversion between plain Rust values and nested TOML tables.

use toml::Table;

use crate::Result;

/// A plain configuration value stored as a nested TOML table.
///
/// Unlike [`ConferModule`](crate::ConferModule), a table has no section or lock of
/// its own. `path` identifies its location in diagnostics, not a store address.
/// Errors for keys in this table use `path` as their section; nested errors use
/// the nested path. Unknown keys may be ignored when loading.
///
/// Manual implementations are supported. This trait's signature, including its
/// TOML types, may change before version 0.1.
///
/// # Example
///
/// A manual implementation for a type that cannot derive the trait:
///
/// ```
/// use conferencier::toml::{Table, Value};
/// use conferencier::{ConferError, ConferTable, Result};
///
/// struct Endpoint(std::net::SocketAddr);
///
/// impl ConferTable for Endpoint {
///     fn from_table(path: &str, mut table: Table) -> Result<Self> {
///         let raw = match table.remove("addr") {
///             Some(Value::String(raw)) => raw,
///             Some(other) => {
///                 return Err(ConferError::type_mismatch(path, "addr", "string", other.type_str()));
///             }
///             None => return Err(ConferError::missing_key(path, "addr")),
///         };
///         raw.parse()
///             .map(Endpoint)
///             .map_err(|err| ConferError::value_parse(path, "addr", err.to_string()))
///     }
///
///     fn to_table(&self, _path: &str) -> Result<Table> {
///         let mut table = Table::new();
///         table.insert("addr".into(), Value::String(self.0.to_string()));
///         Ok(table)
///     }
/// }
///
/// let table: Table = "addr = \"127.0.0.1:8080\"".parse().unwrap();
/// let endpoint = Endpoint::from_table("App.endpoint", table)?;
/// assert_eq!(endpoint.0.port(), 8080);
///
/// let error = Endpoint::from_table("App.endpoint", Table::new()).err().unwrap();
/// assert_eq!(error.to_string(), "missing key App.endpoint.addr");
/// # Ok::<(), ConferError>(())
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a nested configuration table",
    note = "derive it with `#[derive(ConferTable)]` or implement `ConferTable` manually"
)]
pub trait ConferTable: Sized {
    /// Constructs a value from an owned snapshot, allowing values to be moved out.
    fn from_table(path: &str, table: Table) -> Result<Self>;

    /// Returns the complete table to persist.
    ///
    /// The owning module replaces its section on save, removing keys not returned
    /// here. `path` is used only for error reporting.
    fn to_table(&self, path: &str) -> Result<Table>;
}
