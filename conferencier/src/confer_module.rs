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
    /// Refreshes the module state from the shared store.
    async fn load(module: &SharedConferModule<Self>, store: SharedConfer) -> Result<()>;
    /// Persists the module state back to the shared store.
    async fn save(module: &SharedConferModule<Self>, store: SharedConfer) -> Result<()>;
}
