//! The [`ConfigScope`] trait and the [`SharedScope`] handle it works with.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::error::Result;
use crate::store::ConfigStore;

/// Shared, asynchronous handle to a scope derived with [`ConfigScope`].
pub type SharedScope<T> = Arc<RwLock<T>>;

/// Trait implemented by structs annotated with `#[derive(ConfigScope)]`.
#[async_trait]
pub trait ConfigScope: Send + Sync + Sized + 'static {
    /// Instantiates the scope from the provided [`ConfigStore`], performing an initial load.
    async fn from_store(store: ConfigStore) -> Result<SharedScope<Self>>;
    /// Reloads the loaded fields from the scope's section of the store.
    ///
    /// The scope is updated only if every field converts successfully; on error it
    /// keeps its previous values. Ignored fields are never changed.
    ///
    /// This locks `scope` for writing, so it never completes while the calling task
    /// holds a guard of `scope`.
    async fn load(scope: &SharedScope<Self>, store: ConfigStore) -> Result<()>;
    /// Writes the loaded fields into the store, replacing the scope's whole section.
    ///
    /// Keys in the section that no loaded field maps to are removed. This updates only
    /// the in-memory store; [`ConfigStore::save_file`](crate::ConfigStore::save_file) writes it to a file.
    ///
    /// This locks `scope` for reading, so it may never complete while the calling task
    /// holds a guard of `scope` (see [`tokio::sync::RwLock`] for its fairness rules).
    async fn save(scope: &SharedScope<Self>, store: ConfigStore) -> Result<()>;
}
