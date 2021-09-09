use crate::output::OutputAdapter;
use crate::watcher::LineInfo;
use tokio::sync::{mpsc, watch};

// TODO: Or we could use a different (probably safer) way to make the publisher concurrent:
//         -When we publish, if success, push the line into a buffer, once the buffer reaches a certain
//         cap, it will be pushed into a file. This file will become the backed up file, and date & time
//         will be added to the name.
//         -Thus, we're sure that no corruption can occurred.
//         -If a message fail, we can retry until it goes through, then we add it to that buffer.
//         -Everytime the buffer is being saved, we trim the head of the log of these msg as they
//         don't need to be there anymore.

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
        // don't decrement the position sent if
        // amqp returns response at a different order
        let _last_pos = 0;

        // The messages are published in a sequential order,
        // we might need to use `last_pos` if we want to send messages to amqp concurrently.
        while let Some((pos, line)) = self.rx.recv().await {
            // todo: we could potentially spawn this in a new thread
            //       to make it concurrent.
            if let Err(e) = self.fnc.send(pos, line).await {
                error!("pos <{}>: {}", pos, e);
                break; // we exit the software
            } else {
                // if successfully published, we memorize the last position sent
                // which will be used to be stored in a file as a saved state in order to recover it
                self.state_tx.send(pos).unwrap();
            }
        }
    }
}
