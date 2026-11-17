use std::future::Future;
use std::pin::Pin;

use conferencier::ConfigStore;
use tokio::sync::{broadcast, watch};

use crate::events::Event;
use crate::persist::PersistenceHandle;

pub type ComponentResult<T> = Result<T, String>;
pub type ComponentFuture<'a, T> = Pin<Box<dyn Future<Output = ComponentResult<T>> + Send + 'a>>;

#[derive(Clone)]
pub struct Context {
    pub store: ConfigStore,
    pub persistence: PersistenceHandle,
    pub events: broadcast::Sender<Event>,
    pub shutdown: watch::Receiver<bool>,
}

impl Context {
    pub fn publish(&self, event: Event) -> bool {
        self.events.send(event).is_ok()
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
    fn shutdown<'a>(&'a mut self) -> ComponentFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}
