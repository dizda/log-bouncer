use crate::output::OutputAdapter;
use crate::watcher::LineKind;
use tokio::sync::mpsc::Receiver;

pub struct Publisher<Output: OutputAdapter> {
    rx: Receiver<LineKind>,
    fnc: Output,
}

impl<Output: OutputAdapter> Publisher<Output> {
    pub fn new(output: Output, rx: Receiver<LineKind>) -> Self {
        Self { fnc: output, rx }
    }

    /// Send lines to the defined output
    pub async fn publish(&mut self) {
        while let Some(string) = self.rx.recv().await {
            if let Err(e) = self.fnc.send(string).await {
                error!("{}", e);
                break; // we exit the software
            }
        }
    }
}
