use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use conferencier::{ConfigScope, ConfigStore, SharedScope};
use tokio::sync::{broadcast, watch};

use crate::events::Event;
use crate::persist::PersistenceHandle;

pub type ComponentResult<T> = Result<T, String>;
pub type ComponentFuture<'a, T> = Pin<Box<dyn Future<Output = ComponentResult<T>> + Send + 'a>>;

#[derive(Clone)]
pub struct Context {
    pub store: ConfigStore,
    pub config_path: PathBuf,
    pub persistence: PersistenceHandle,
    pub events: broadcast::Sender<Event>,
    pub shutdown: watch::Receiver<bool>,
}

impl Context {
    pub fn publish(&self, event: Event) -> bool {
        self.events.send(event).is_ok()
    }

    pub async fn save<T: ConfigScope>(&self, scope: &SharedScope<T>) -> ComponentResult<()> {
        T::save(scope, &self.store)
            .await
            .map_err(|error| error.to_string())?;
        self.persistence.changed()
    }

    /// Replaces a field, restoring it if saving or requesting persistence fails.
    /// A successful request does not wait for the file write.
    pub async fn update<T: ConfigScope, V: Send>(
        &self,
        scope: &SharedScope<T>,
        field: fn(&mut T) -> &mut V,
        value: V,
    ) -> ComponentResult<()> {
        let previous = {
            let mut config = scope.write().await;
            std::mem::replace(field(&mut config), value)
        };
        if let Err(error) = self.save(scope).await {
            *field(&mut *scope.write().await) = previous;
            return Err(error);
        }
        Ok(())
    }

    pub async fn save_defaults<T: ConfigScope>(
        &self,
        section: &str,
        scope: &SharedScope<T>,
    ) -> ComponentResult<()> {
        if !self.store.has_section(section) {
            self.save(scope).await?;
        }
        Ok(())
    }
}

pub trait Component: Send + Sync {
    fn name(&self) -> &'static str;
    fn section(&self) -> &'static str;
    fn start<'a>(&'a self, context: Context) -> ComponentFuture<'a, Box<dyn RunningComponent>>;
}

pub trait RunningComponent: Send {
    fn name(&self) -> &'static str;
    fn handle_command<'a>(&'a mut self, args: &'a str) -> ComponentFuture<'a, ()>;
    /// Waits for event producers to stop before subscribers are shut down.
    fn quiesce<'a>(&'a mut self) -> ComponentFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown<'a>(&'a mut self) -> ComponentFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
