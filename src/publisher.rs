use crate::output::OutputAdapter;
use crate::watcher::LineInfo;
use tokio::sync::{mpsc, watch};

pub struct Publisher<Output: OutputAdapter> {
    rx: mpsc::Receiver<LineInfo>,
    fnc: Output,
    state_tx: watch::Sender<u64>,
}

impl<Output: OutputAdapter> Publisher<Output> {
    pub fn new(output: Output, rx: mpsc::Receiver<LineInfo>, state_tx: watch::Sender<u64>) -> Self {
        Self {
            fnc: output,
            rx,
            state_tx,
        }
    }

    /// Send lines to the defined output
    pub async fn publish(&mut self) {
        while let Some((pos, line)) = self.rx.recv().await {
            if let Err(e) = self.fnc.send(line).await {
                error!("{}", e);
                break; // we exit the software
            } else {
                // if successfully published, we memorize the last position sent
                // which will be used to be stored in a file as a saved state in order to recover it
                self.state_tx.send(pos).unwrap();
            }
        }
    }
}
