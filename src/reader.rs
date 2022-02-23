use crate::tail;
use crate::tail::TailedFile;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::Notify;

const TAIL_WAIT_DURATION: Duration = Duration::from_millis(500);

pub type LineInfo = (u64, String);

/// Read a file, then send every new line to the other thread
pub struct Reader {
    /// Path of the file to monitor
    path: PathBuf,
    /// The recovered position from the last launch
    pos: u64,
    /// Send each line to the publisher
    tx: Sender<LineInfo>,
}

impl Reader {
    pub fn new(path: PathBuf, pos: u64, tx: Sender<LineInfo>) -> Result<Self, Box<dyn Error>> {
        info!("Recovered the cursor from the position <{}>", pos);

        Ok(Self { path, pos, tx })
    }

    pub fn work(mut self) -> Arc<Notify> {
        let panicked = Arc::new(Notify::new());
        let notifier = panicked.clone();

        std::thread::spawn(move || {
            let tx = self.tx;

            let mut tail = TailedFile::new(&self.path).unwrap();
            tail.set_pos(self.pos); // recover previous position

            loop {
                match tail.follow() {
                    Ok(lines) => {
                        for line in lines {
                            if let Err(e) = tx.blocking_send((tail.pos(), line)) {
                                error!("Can't send to mpsc: {}", e); // this is a fatal error
                                break;
                            }
                        }
                    }
                    Err(err) => match err {
                        tail::Error::FileRotated | tail::Error::FileTruncated => warn!("{}", err),
                        _ => {
                            error!("{}", err); // this may be fatal, too
                            break;
                        }
                    },
                };

                sleep(TAIL_WAIT_DURATION);
            }

            // will exit the software
            notifier.notify_one();
        });

        panicked
    }
}
