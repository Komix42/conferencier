//! Conversion between plain Rust values and nested TOML tables.

use toml::Table;

use crate::Result;

/// A plain configuration value stored as a nested TOML table.
///
/// Unlike [`ConfigScope`](crate::ConfigScope), a table has no section or lock of
/// its own. `path` identifies its location in diagnostics, not a store address.
/// Errors for keys in this table use `path` as their section; nested errors use
/// the nested path, for example `App.upstream[1]`. A dot in `path` does not cause
/// a store lookup, and these functions never read or write a file.
///
/// # Derived tables
///
/// With the default `derive` feature, `#[derive(ConfigTable)]` supports the same
/// field attributes as [`ConfigScope`](crate::ConfigScope): `rename`, `default`,
/// `ignore` and `table`. A table-only derive does not accept `section`; when both
/// traits are derived, `section` applies only to the scope.
/// Mark an owning field `#[confer(table)]` and use `T`, `Option<T>` or `Vec<T>`
/// to represent a table, an optional table or an array of tables respectively.
/// Unknown keys are ignored on load unless the type has
/// `#[confer(deny_unknown_keys)]`. `to_table` returns only keys mapped by loaded
/// fields, even for strict types. Strictness belongs to each type and is not
/// inherited from its containing scope or table.
///
/// Tables are plain values, not shared scope handles. Loading a scope rebuilds
/// its nested tables, so ignored fields of a table are initialized on every
/// successful construction, unlike ignored fields directly in a scope.
/// Keep runtime state that must survive reloads in the scope itself.
///
/// # Manual implementations
///
/// Implementations own the input table and can move values out instead of cloning
/// them. Use [`Error`](crate::Error)'s constructors to attach `path` and the failing
/// key to conversion errors. When delegating to a nested table, extend `path`
/// with [`table_path`](crate::table_path) so that its errors carry the same
/// paths as derived tables. Return a complete table from `to_table`, not a patch.
/// These signatures and their TOML types may change while the API is in alpha.
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
///     fn from_table(path: &str, mut table: Table) -> Result<Self> {
///         let raw = match table.remove("addr") {
///             Some(Value::String(raw)) => raw,
///             Some(other) => {
///                 return Err(Error::type_mismatch(path, "addr", "string", other.type_str()));
///             }
///             None => return Err(Error::missing_key(path, "addr")),
///         };
///         raw.parse()
///             .map(Endpoint)
///             .map_err(|err| Error::value_parse(path, "addr", err.to_string()))
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
/// # Ok::<(), Error>(())
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a nested configuration table",
    note = "derive it with `#[derive(ConfigTable)]` or implement `ConfigTable` manually"
)]
pub trait ConfigTable: Sized {
    /// Constructs a value from an owned table, using `path` for error reporting.
    ///
    /// For an absent key, the derived implementation applies an explicit fallback,
    /// uses `None` for an `Option`, or returns [`Error::MissingKey`](crate::Error::MissingKey).
    /// `#[confer(table, field_defaults)]` loads a missing single nested table from
    /// an empty table without requiring the nested type to implement `Default`.
    /// Unknown keys are ignored on load unless `#[confer(deny_unknown_keys)]`
    /// is set. This does not change `to_table`, which includes only loaded fields.
    ///
    /// Ignored fields are initialized after this table's loaded fields convert.
    /// Each call creates a fresh value; side effects of default expressions or
    /// initializers that have already run are not undone if a later conversion fails.
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingKey`](crate::Error::MissingKey),
    /// [`Error::TypeMismatch`](crate::Error::TypeMismatch) or
    /// [`Error::ValueParse`](crate::Error::ValueParse) for invalid fields.
    /// A manual implementation may return other errors as appropriate.
    fn from_table(path: &str, table: Table) -> Result<Self>;

    /// Converts this value into the complete table to persist.
    ///
    /// The derived implementation writes only loaded fields, omits absent optional
    /// fields, writes empty arrays of tables as `[]`, and recursively converts nested tables.
    /// It never mutates `self`. `path` is used only for error reporting.
    ///
    /// The owning scope replaces its whole section on save, so keys not returned
    /// here disappear from the persisted nested table.
    ///
    /// # Errors
    ///
    /// Returns a conversion error, such as an unsigned integer outside TOML's
    /// signed 64-bit range, or an error from a nested table implementation.
    /// No partial table is returned, and the caller's store is not modified.
    fn to_table(&self, path: &str) -> Result<Table>;
}
