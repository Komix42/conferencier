#![cfg_attr(feature = "derive", doc = include_str!("../README.md"))]
#![cfg_attr(
    not(feature = "derive"),
    doc = r#"A minimal, single-purpose TOML configuration library for Tokio applications.

The `derive` feature is disabled in this build. Store access and manual trait
implementations remain available:

- [`ConfigStore`] is a cloneable handle to one in-memory TOML document. Its clones
  share data; in-memory operations are synchronous and only file I/O is async.
- [`ConfigScope`] describes typed configuration and runtime state, shared through
  [`SharedScope<T>`]. Loading and saving explicitly copy data to and from a store.
- [`ConfigTable`] converts plain values to and from nested TOML tables.
- [`table_path`] formats diagnostic paths for manual implementations.
- [`Error`] and [`Result`] carry file, TOML and field-conversion errors.
- [`toml`] and [`Datetime`] re-export the TOML types used by the public API.

Enable the default `derive` feature for `#[derive(ConfigScope)]`,
`#[derive(ConfigTable)]` and the full
[attribute reference](https://docs.rs/conferencier#derive-attributes)."#
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod error;
mod scope;
mod store;
mod table;
pub mod table_path;
mod value_conversion;

pub use crate::error::{Error, Result};
pub use crate::scope::ConfigScope;
pub use crate::scope::SharedScope;
pub use crate::store::ConfigStore;
pub use crate::table::ConfigTable;

/// The [`toml`] crate used by this library; its types appear in the public API.
///
/// Prefer `conferencier::toml::{Table, Value}` in manual table implementations
/// and raw store access so the types match this crate's TOML dependency.
/// Conferencier does not preserve the input document's comments or formatting.
pub use toml;
/// TOML date, time or datetime value, usable directly in derived scopes and tables.
///
/// This re-exports [`toml::value::Datetime`]. TOML 1.1 permits times with omitted
/// seconds, and the type preserves that distinction: `Time::second` and
/// `Time::nanosecond` are optional. Store datetime getters accept both TOML
/// datetime values and strings parsed as this type.
pub use toml::value::Datetime;

// The derive macros share the traits' names but live in the macro namespace, so
// `use conferencier::ConfigScope;` imports both (likewise for `ConfigTable`).
#[cfg(feature = "derive")]
pub use conferencier_derive::{ConfigScope, ConfigTable};

#[doc(hidden)]
pub mod __private {
    pub use std::sync::Arc;
    pub use tokio::sync::RwLock;

    use crate::scope::SharedScope;
    pub use crate::table_path::{child_path, element_path, section_path};
    pub use crate::value_conversion::{
        array, boolean, boolean_vec, datetime, datetime_vec, float, float_vec, integer,
        integer_vec, string, string_vec, table, table_element,
    };

    /// Returns a snapshot of `section`: empty when absent, an error when not a table.
    pub fn read_section(store: &crate::ConfigStore, section: &str) -> crate::Result<toml::Table> {
        store.read_section(section)
    }

    /// Rejects the first key not present in the struct's loaded field names.
    pub fn deny_unknown_keys(path: &str, table: &toml::Table, known: &[&str]) -> crate::Result<()> {
        match table
            .keys()
            .filter(|key| !known.contains(&key.as_str()))
            .min()
        {
            Some(key) => Err(crate::Error::unknown_key(path, key.clone())),
            None => Ok(()),
        }
    }

    /// Wraps `value` in the shared scope type used by the derive implementation.
    pub fn new_shared_scope<T>(value: T) -> SharedScope<T> {
        Arc::new(RwLock::new(value))
    }
}
