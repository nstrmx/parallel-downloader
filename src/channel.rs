use std::sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex};
use log::error;

#[derive(Clone)]
pub struct SharedChannel<T> {
    name: String,
    tx: Arc<Mutex<Sender<T>>>,
    rx: Arc<Mutex<Receiver<T>>>,
    lock_try_max: u8,
}

impl<T: Clone> SharedChannel<T> {
    pub fn new(name: &str) -> Self {
        let (tx, rx) = channel::<T>();
        let shared_tx = Arc::new(Mutex::new(tx));
        let shared_rx = Arc::new(Mutex::new(rx));
        return SharedChannel{
            name: name.to_string(),
            tx: shared_tx,
            rx: shared_rx,
            lock_try_max: 100,
        };
    }

    pub fn send(&self, data: T) -> Option<()> {
        for _i in 0..self.lock_try_max {
            match self.tx.lock() {
                Ok(locked_tx) => {
                    if let Ok(result) = locked_tx.send(data.clone()) {
                        return Some(result);
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    error!("error locking shared channel {} tx: {}", self.name, err);
                }
            };
        }
        return None;
    }

    pub fn recv(&self) -> Option<T> {
        for _i in 0..self.lock_try_max {
            match self.rx.lock() {
                Ok(locked_rx) => {
                    if let Ok(result) = locked_rx.recv() {
                        return Some(result);
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    error!("error locking shared channel {} rx: {}", self.name, err);
                }
            };
        }
        return None;
    }

    pub fn try_recv(&self) -> Option<T> {
        for _i in 0..self.lock_try_max {
            match self.rx.lock() {
                Ok(locked_rx) => {
                    if let Ok(result) = locked_rx.try_recv() {
                        return Some(result);
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    error!("error locking shared channel {} rx: {}", self.name, err);
                }
            };
        }
        return None;
    }
}