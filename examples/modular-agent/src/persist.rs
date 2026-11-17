use std::path::PathBuf;

use conferencier::ConfigStore;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct PersistenceHandle {
    sender: mpsc::UnboundedSender<PersistMessage>,
}

pub enum PersistMessage {
    Changed,
    Flush(oneshot::Sender<Result<(), String>>),
}

impl PersistenceHandle {
    pub fn new(sender: mpsc::UnboundedSender<PersistMessage>) -> Self {
        Self { sender }
    }

    pub fn changed(&self) -> Result<(), String> {
        self.sender
            .send(PersistMessage::Changed)
            .map_err(|_| "configuration persister has stopped".to_owned())
    }

    pub async fn flush(&self) -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(PersistMessage::Flush(sender))
            .map_err(|_| "configuration persister has stopped".to_owned())?;
        receiver
            .await
            .map_err(|_| "configuration persister stopped before flushing".to_owned())?
    }
}

pub async fn run(
    store: ConfigStore,
    path: PathBuf,
    mut receiver: mpsc::UnboundedReceiver<PersistMessage>,
) {
    while let Some(message) = receiver.recv().await {
        match message {
            PersistMessage::Changed => {
                if let Err(error) = store.save_file(&path).await {
                    eprintln!("cannot save {}: {error}", path.display());
                }
            }
            PersistMessage::Flush(reply) => {
                let result = store
                    .save_file(&path)
                    .await
                    .map_err(|error| format!("cannot save {}: {error}", path.display()));
                let _ = reply.send(result);
            }
        }
    }
}
