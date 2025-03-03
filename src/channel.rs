use std::sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex};
use anyhow::{bail, Result};
use log::error;

#[derive(Clone)]
pub struct SharedChannel<T> {
    name: String,
    tx: Arc<Mutex<Sender<T>>>,
    rx: Arc<Mutex<Receiver<T>>>,
}

impl<T: Clone> SharedChannel<T> {
    pub fn new(name: &str) -> Self {
        let (tx, rx) = channel::<T>();
        let shared_tx = Arc::new(Mutex::new(tx));
        let shared_rx = Arc::new(Mutex::new(rx));
        SharedChannel{
            name: name.to_string(),
            tx: shared_tx,
            rx: shared_rx,
        }
    }

    pub fn send(&self, data: T) -> Result<()> {
        match self.tx.lock() {
            Ok(tx) => if let Err(e) = tx.send(data.clone()) {
                bail!(format!("error sending to shared channel {} tx: {e}", self.name));
            } else {
                Ok(())
            },
            Err(e) => bail!(format!("error locking shared channel {} tx: {e}", self.name)),
        }
    }

    pub fn recv(&self) -> Result<T> {
        match self.rx.lock() {
            Ok(rx) => Ok(rx.recv()?),
            Err(e) => bail!(format!("error locking shared channel {} rx: {e}", self.name)),
        }
    }

    pub fn try_recv(&self) -> Result<T> {
        match self.rx.lock() {
            Ok(rx) => Ok(rx.try_recv()?),
            Err(e) => bail!(format!("error locking shared channel {} rx: {e}", self.name)),
        }
    }
}
