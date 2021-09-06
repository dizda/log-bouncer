use logwatcher::{LogWatcher, LogWatcherAction};
use std::error::Error;
use tokio::sync::mpsc::Sender;

pub type LineKind = String;

pub struct Watcher {
    log_watcher: LogWatcher,
    tx: Sender<LineKind>,
}

impl Watcher {
    pub fn new(file: &str, tx: Sender<LineKind>) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            // TODO: before opening this file, check if it's larger than 0 byte, if yes, we rotate it before
            //       registering
            log_watcher: LogWatcher::register(file)?,
            tx,
        })
    }

    pub fn work(mut self) {
        std::thread::spawn(move || {
            let tx = self.tx;

            self.log_watcher.watch(&mut move |line: String| {
                if let Err(e) = tx.blocking_send(line) {
                    panic!("Can't send to mpsc: {}", e); // this is a fatal error
                } else {
                    trace!("Line sent via mpsc!");
                }

                LogWatcherAction::None
            });
        });
    }
}
