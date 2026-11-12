#![cfg_attr(feature = "derive", doc = include_str!("../README.md"))]
#![cfg_attr(
    not(feature = "derive"),
    doc = "A minimal, single-purpose TOML configuration library for Tokio applications. Enable the `derive` feature for `#[derive(ConfigScope)]` and `#[derive(ConfigTable)]`."
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod error;
mod scope;
mod store;
mod table;
mod table_path;
mod value_conversion;

pub use crate::error::{Error, Result};
pub use crate::scope::ConfigScope;
/// Shared [`tokio::sync::RwLock`] wrapper used by derived scopes.
pub use crate::scope::SharedScope;
pub use crate::store::ConfigStore;
pub use crate::table::ConfigTable;

/// The [`toml`] crate used by this library; its types appear in the public API.
pub use toml;
/// TOML datetime type, usable directly as a field type in derived scopes.
pub use toml::value::Datetime;

// The derive macros share the traits' names but live in the macro namespace, so
// `use conferencier::ConfigScope;` imports both (likewise for `ConfigTable`).
#[cfg(feature = "derive")]
pub use conferencier_derive::{ConfigScope, ConfigTable};

#[doc(hidden)]
pub mod __private {
    pub use async_trait::async_trait;
    pub use std::sync::Arc;
    pub use tokio::sync::RwLock;

    use crate::scope::SharedScope;
    pub use crate::table_path::{child_path, element_path};
    pub use crate::value_conversion::{
        array, boolean, boolean_vec, datetime, datetime_vec, float, float_vec, integer,
        integer_vec, string, string_vec, table, table_element,
    };

    /// Returns a snapshot of `section`: empty when absent, an error when not a table.
    pub fn read_section(store: &crate::ConfigStore, section: &str) -> crate::Result<toml::Table> {
        store.read_section(section)
    }

    /// Wraps `value` in the shared scope type used by the derive implementation.
    pub fn new_shared_scope<T>(value: T) -> SharedScope<T> {
        Arc::new(RwLock::new(value))
    }
}
