use logwatcher::{LogWatcher, LogWatcherAction, StartFrom};
use std::error::Error;
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::mpsc::Sender;
use tokio::sync::Notify;

pub type LineInfo = (u64, String);

pub struct Watcher {
    log_watcher: LogWatcher,
    tx: Sender<LineInfo>,
}

impl Watcher {
    pub fn new(file: &str, tx: Sender<LineInfo>) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            // TODO: before opening this file, check if it's larger than 0 byte, if yes, we rotate it before
            //       registering
            log_watcher: LogWatcher::register(file, StartFrom::End)?,
            tx,
        })
    }

    pub fn work(mut self) -> Arc<Notify> {
        let panicked = Arc::new(Notify::new());
        let notifier = panicked.clone();

        std::thread::spawn(move || {
            let tx = self.tx;

            self.log_watcher.watch(&mut move |pos, len, line: String| {
                let state = pos + len as u64;

                if let Err(e) = tx.blocking_send((state, line)) {
                    panic!("Can't send to mpsc: {}", e); // this is a fatal error
                } else {
                    trace!("Line sent via mpsc!");
                }

                LogWatcherAction::None
            });

            notifier.notify_one();
        });

        panicked
    }
}
