//! The [`ConferModule`] trait and the [`SharedConferModule`] handle it works with.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::error::Result;
use crate::store::SharedConfer;

/// Shared, asynchronous handle to a module derived with [`ConferModule`].
pub type SharedConferModule<T> = Arc<RwLock<T>>;

/// Trait implemented by structs annotated with `#[derive(ConferModule)]`.
#[async_trait]
pub trait ConferModule: Send + Sync + Sized + 'static {
    /// Instantiates the module from the provided [`SharedConfer`], performing an initial load.
    async fn from_confer(store: SharedConfer) -> Result<SharedConferModule<Self>>;
    /// Reloads the loaded fields from the module's section of the store.
    ///
    /// The module is updated only if every field converts successfully; on error it
    /// keeps its previous values. Ignored fields are never changed.
    ///
    /// This locks `module` for writing, so it never completes while the calling task
    /// holds a guard of `module`.
    async fn load(module: &SharedConferModule<Self>, store: SharedConfer) -> Result<()>;
    /// Writes the loaded fields into the store, replacing the module's whole section.
    ///
    /// Keys in the section that no loaded field maps to are removed. This updates only
    /// the in-memory store; [`Confer::save_file`](crate::Confer::save_file) writes it to a file.
    ///
    /// This locks `module` for reading, so it may never complete while the calling task
    /// holds a guard of `module` (see [`tokio::sync::RwLock`] for its fairness rules).
    async fn save(module: &SharedConferModule<Self>, store: SharedConfer) -> Result<()>;
}
