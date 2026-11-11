#![cfg_attr(feature = "with-derive", doc = include_str!("../README.md"))]
#![cfg_attr(
    not(feature = "with-derive"),
    doc = "Async, TOML-backed configuration hub with an ergonomic derive macro."
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod confer_module;
pub mod confer_table;
pub mod error;
mod section_guard;
mod store;
mod table_path;
mod value_conversion;

pub use crate::confer_module::ConferModule;
/// Shared [`tokio::sync::RwLock`] wrapper used by derived modules.
pub use crate::confer_module::SharedConferModule;
pub use crate::confer_table::ConferTable;
pub use crate::error::{ConferError, Result};
pub use crate::store::{Confer, SharedConfer};

/// The [`toml`] crate used by this library; its types appear in the public API.
pub use toml;
/// TOML datetime type, usable directly as a field type in derived modules.
pub use toml::value::Datetime;

// The derive macros share the traits' names but live in the macro namespace, so
// `use conferencier::ConferModule;` imports both (likewise for `ConferTable`).
#[cfg(feature = "with-derive")]
pub use conferencier_derive::{ConferModule, ConferTable};

#[doc(hidden)]
pub mod __private {
    pub use async_trait::async_trait;
    pub use std::sync::Arc;
    pub use tokio::sync::RwLock;

    use crate::confer_module::SharedConferModule;
    pub use crate::table_path::{child_path, element_path};
    pub use crate::value_conversion::{
        array, boolean, boolean_vec, datetime, datetime_vec, float, float_vec, integer,
        integer_vec, string, string_vec, table, table_element,
    };

    /// Returns a snapshot of `section`: empty when absent, an error when not a table.
    pub async fn read_section(store: &crate::Confer, section: &str) -> crate::Result<toml::Table> {
        store.read_section_table(section).await
    }

    /// Wraps `value` in the shared module type used by the derive implementation.
    pub fn new_shared_module<T>(value: T) -> SharedConferModule<T> {
        Arc::new(RwLock::new(value))
    }
}
