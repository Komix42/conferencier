//! Conversion between plain Rust values and nested TOML tables.

use toml::Table;

use crate::Result;

/// A plain configuration value stored as a nested TOML table.
///
/// Unlike [`ConfigScope`](crate::ConfigScope), a table has no section or lock of
/// its own. `table_path` identifies its location in diagnostics, not a store
/// address. Errors for keys in this table carry it as their `table_path`; nested
/// errors use the nested path, for example `App.upstream[1]`. A dot in
/// `table_path` does not cause a store lookup, and these functions never read or
/// write a file.
///
/// # Implementations
///
/// With the default `derive` feature, `#[derive(ConfigTable)]` implements this
/// trait for named-field structs. The
/// [nested tables guide](https://docs.rs/conferencier/%5E0.1.0/conferencier/guide/tables/index.html)
/// describes the table field shapes, their attributes and how tables behave on reload.
///
/// Manual implementations own the input table and can move values out instead of cloning
/// them. Use [`Error`](crate::Error)'s constructors to attach `table_path` and the
/// failing key to conversion errors. When delegating to a nested table, extend
/// `table_path` with [`table_path`](crate::table_path) so that its errors carry
/// the same paths as derived tables. Return a complete table from `to_table`, not a patch.
/// Manual implementations remain supported. The required signatures, bounds
/// and documented behavior are stable throughout 0.1.x; neither this trait nor
/// `ConfigScope` gains methods during that series.
///
/// # Example
///
/// A manual implementation for a type that cannot derive the trait:
///
/// ```
/// use conferencier::toml::{Table, Value};
/// use conferencier::{Error, ConfigTable, Result};
///
/// struct Endpoint(std::net::SocketAddr);
///
/// impl ConfigTable for Endpoint {
///     fn from_table(table_path: &str, mut table: Table) -> Result<Self> {
///         let raw = match table.remove("addr") {
///             Some(Value::String(raw)) => raw,
///             Some(other) => {
///                 return Err(Error::type_mismatch_error(
///                     table_path,
///                     "addr",
///                     "string",
///                     other.type_str(),
///                 ));
///             }
///             None => return Err(Error::missing_key_error(table_path, "addr")),
///         };
///         raw.parse()
///             .map(Endpoint)
///             .map_err(|err| Error::value_parse_error(table_path, "addr", err.to_string()))
///     }
///
///     fn to_table(&self, _table_path: &str) -> Result<Table> {
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
/// # Ok::<(), Error>(())
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a nested configuration table",
    note = "derive it with `#[derive(ConfigTable)]` or implement `ConfigTable` manually"
)]
pub trait ConfigTable: Sized {
    /// Constructs a value from an owned table, using `table_path` for error reporting.
    ///
    /// For an absent key, the derived implementation uses its field's fallback,
    /// `None` for an `Option`, or returns [`Error::MissingKey`](crate::Error::MissingKey).
    /// Unknown keys are ignored unless the type has `#[confer(deny_unknown_keys)]`.
    ///
    /// Ignored fields are initialized after this table's loaded fields convert.
    /// Each call creates a fresh value; side effects of default expressions or
    /// initializers that have already run are not undone if a later conversion fails.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`](crate::Error::MissingKey),
    /// [`Error::TypeMismatch`](crate::Error::TypeMismatch) or
    /// [`Error::ValueParse`](crate::Error::ValueParse) for invalid fields, or
    /// [`Error::UnknownKey`](crate::Error::UnknownKey) for an unknown key in a strict table.
    /// A manual implementation may return other errors as appropriate.
    fn from_table(table_path: &str, table: Table) -> Result<Self>;

    /// Converts this value into the complete table to persist.
    ///
    /// The derived implementation writes only loaded fields, omits absent optional
    /// fields, writes empty arrays of tables as `[]`, and recursively converts nested tables.
    /// It never mutates `self`. `table_path` is used only for error reporting.
    ///
    /// The owning scope replaces its whole section on save, so keys not returned
    /// here disappear from the persisted nested table.
    ///
    /// # Errors
    ///
    /// Returns a conversion error, such as an unsigned integer outside TOML's
    /// signed 64-bit range, or an error from a nested table implementation.
    /// No partial table is returned, and the caller's store is not modified.
    fn to_table(&self, table_path: &str) -> Result<Table>;
}
