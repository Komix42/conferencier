//! Async, TOML-backed configuration hub with an ergonomic derive macro.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod confer_module;
pub mod error;
mod section_guard;
mod store;
mod value_conversion;

/// Shared [`tokio::sync::RwLock`] wrapper used by derived modules.
pub use crate::confer_module::SharedConferModule;
pub use crate::error::{ConferError, Result};
pub use crate::store::{Confer, SharedConfer};

#[cfg(feature = "with-derive")]
pub use conferencier_derive::ConferModule;

#[doc(hidden)]
pub mod __private {
    pub use async_trait::async_trait;
    pub use std::sync::Arc;
    pub use tokio::sync::RwLock;

    use crate::confer_module::SharedConferModule;

    /// Wraps `value` in the shared module type used by the derive implementation.
    pub fn new_shared_module<T>(value: T) -> SharedConferModule<T> {
        Arc::new(RwLock::new(value))
    }
}
