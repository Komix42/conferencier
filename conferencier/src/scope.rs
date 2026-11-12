//! The [`ConfigScope`] trait and the [`SharedScope`] handle it works with.

use std::future::Future;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::Result;
use crate::store::ConfigStore;

/// Shared handle to a scope's configuration and runtime state.
///
/// This is an alias for [`Arc<RwLock<T>>`], not a separate wrapper: Tokio's
/// [`read`](RwLock::read), [`write`](RwLock::write), [`try_read`](RwLock::try_read)
/// and [`try_write`](RwLock::try_write) methods are directly available.
/// Cloning the handle shares the same value and lock; it does not copy `T`.
///
/// Keep guards short-lived. A guard can cross an `.await`, but the lock is not
/// reentrant: drop every guard before calling [`ConfigScope::load`] or
/// [`ConfigScope::save`] on this handle in the same task. A held read guard can
/// also block `save` when a writer is queued, because Tokio's lock is fair.
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use conferencier::SharedScope;
/// use tokio::sync::RwLock;
///
/// # #[tokio::main]
/// # async fn main() {
/// let scope: SharedScope<u64> = Arc::new(RwLock::new(0));
/// let task_scope = scope.clone();
/// tokio::spawn(async move {
///     *task_scope.write().await += 1;
/// })
/// .await
/// .expect("task panicked");
///
/// assert_eq!(*scope.read().await, 1);
/// # }
/// ```
pub type SharedScope<T> = Arc<RwLock<T>>;

/// A component's typed configuration and runtime state, shared between Tokio tasks.
///
/// A derived scope owns one top-level TOML section. Loaded fields describe the
/// settings persisted in that section; fields marked `#[confer(ignore)]` or
/// `#[confer(ignore = <expr>)]` hold runtime state that is not persisted.
/// Nested configuration values use [`ConfigTable`](crate::ConfigTable).
///
/// # Store and scope are separate
///
/// [`from_store`](Self::from_store), [`load`](Self::load) and [`save`](Self::save)
/// explicitly copy data between a scope and a [`ConfigStore`]. A derived scope
/// does not retain the store or synchronize with it in the background. Changing
/// the store does not change an existing scope until `load` is called; changing
/// a scope does not change the store until `save` is called. None of these
/// functions reads or writes a file.
///
/// # Implementations
///
/// With the default `derive` feature, `#[derive(ConfigScope)]` implements this
/// trait for named-field structs. `#[confer(section = "App")]` selects the section;
/// otherwise it is the full struct name. The temporary migration check for a name
/// such as `ConferApp` requires an explicit section through 0.1.x and is removed
/// in 0.2.0. See the crate's [attribute reference](crate#derive-attributes).
///
/// Manual implementations can write `async fn load` and `async fn save`; the compiler
/// checks that the returned futures are `Send`, so generic code can spawn them on Tokio tasks.
/// No `#[async_trait]` is needed. Manual implementations are responsible for the
/// loading, saving and error semantics described by this trait.
///
/// # Example
///
/// ```
/// # #[cfg(feature = "derive")]
/// # fn main() -> conferencier::Result<()> {
/// use conferencier::{ConfigScope, ConfigStore};
///
/// #[derive(ConfigScope)]
/// #[confer(section = "App")]
/// struct AppConfig {
///     port: u16,
///     #[confer(ignore)]
///     requests: u64,
/// }
///
/// let store = ConfigStore::from_str("[App]\nport = 8080")?;
/// let app = AppConfig::from_store(&store)?; // No runtime needed for construction.
///
/// # tokio::runtime::Runtime::new()?.block_on(async {
/// app.write().await.requests = 1;
/// store.set_integer("App", "port", 9090)?;
/// AppConfig::load(&app, &store).await?;
/// {
///     let state = app.read().await;
///     assert_eq!(state.port, 9090);
///     assert_eq!(state.requests, 1);
/// }
///
/// app.write().await.port = 9000;
/// AppConfig::save(&app, &store).await?;
/// assert_eq!(store.get_integer("App", "port")?, 9000);
/// # Ok(())
/// # })
/// # }
/// # #[cfg(not(feature = "derive"))]
/// # fn main() {}
/// ```
pub trait ConfigScope: Send + Sync + Sized + 'static {
    /// Builds a new shared scope from a snapshot of its section in `store`.
    ///
    /// A missing section is loaded as an empty table, so field defaults apply.
    /// Required keys without defaults still fail. Unknown keys are ignored.
    /// Ignored fields are initialized once, in declaration order, after all
    /// loaded fields convert successfully.
    ///
    /// The derived implementation is synchronous and acquires no scope lock, so
    /// construction also works before a Tokio runtime starts. It borrows the store
    /// only for this call; each successful call creates an independent scope.
    ///
    /// # Errors
    ///
    /// Returns a type mismatch if the section exists but is not a table, or the
    /// error from a required key or field conversion. No store values are changed.
    fn from_store(store: &ConfigStore) -> Result<SharedScope<Self>>;

    /// Reloads the loaded fields from the scope's section of the store.
    ///
    /// With loaded fields, the derived implementation converts a whole-section snapshot
    /// before acquiring the scope's write lock. It updates all loaded fields together,
    /// only after every conversion succeeds. Ignored fields of the scope itself are never changed.
    /// Nested [`ConfigTable`](crate::ConfigTable) values are rebuilt, including their
    /// ignored fields; side effects of their initializers are not rolled back on error.
    ///
    /// This reads only the in-memory store. Call [`ConfigStore::load_file`] first
    /// when the source is an edited file. Reloading several scopes is not one
    /// transaction: each successful scope load takes effect independently.
    /// A scope with only ignored fields still validates its section but needs no scope lock.
    ///
    /// # Locking
    ///
    /// For a scope with loaded fields, this locks `scope` for writing. Drop every guard
    /// of `scope` before awaiting it in the same task, or a successful conversion will
    /// wait for that guard forever.
    ///
    /// # Errors
    ///
    /// The same section and conversion errors as [`from_store`](Self::from_store).
    /// On error the scope keeps all its previous values, and the store is unchanged.
    fn load(
        scope: &SharedScope<Self>,
        store: &ConfigStore,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Writes the loaded fields into the store, replacing the scope's whole section.
    ///
    /// With loaded fields, the derived implementation takes a read lock of `scope`,
    /// converts them into a new table, releases the scope lock and replaces the store section
    /// in one write. Readers never observe a partially saved section. Other sections
    /// are unchanged; unknown keys and nested tables in the owned section are removed.
    /// Ignored fields are not written.
    /// A scope with only ignored fields saves an empty section without locking `scope`.
    ///
    /// This updates only the in-memory store. [`ConfigStore::save_file`] persists
    /// the store separately. A later scope mutation is not part of the saved snapshot.
    ///
    /// # Locking
    ///
    /// Drop every guard of `scope` before awaiting this function in the same task.
    /// It waits behind a held write guard and can also wait behind a held read guard
    /// if another writer is queued (see [`tokio::sync::RwLock`] for its fairness rules).
    ///
    /// # Errors
    ///
    /// Returns a field conversion error, such as an unsigned integer above TOML's
    /// `i64` range, or a type mismatch if the store section is not a table.
    /// Conversion errors are reported first. On error the store remains unchanged.
    fn save(
        scope: &SharedScope<Self>,
        store: &ConfigStore,
    ) -> impl Future<Output = Result<()>> + Send;
}
