#[macro_use]
extern crate tracing;

pub mod output;

use crate::output::stdout::StdOut;
use crate::output::OutputAdapter;
use logwatcher::{LogWatcher, LogWatcherAction};
use std::error::Error;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

const FILE: &'static str = "test.log";

pub async fn run() -> Result<(), Box<dyn Error>> {
    let log = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env());
    log.finish().init();

    let (tx, mut rx) = mpsc::channel::<String>(1);
    let mut log_watcher = LogWatcher::register(FILE)?;
    let mut cpt: usize = 0;

    std::thread::spawn(move || {
        log_watcher.watch(&mut move |line: String| {
            // TODO: may not be good if this daemon stops while the proxy is still running
            //       it may loose track of the last position and start over again.
            cpt += 1;
            debug!("{}: {}", cpt, &line);

            if let Err(e) = tx.blocking_send(line) {
                error!("{}: {}", cpt, e);
            }

            LogWatcherAction::None
        });
    });

    process(StdOut {}, rx).await;

    Ok(())
}

async fn process<T: OutputAdapter>(fnc: T, mut rx: Receiver<String>) {
    while let Some(i) = rx.recv().await {
        fnc.send(i).await;
    }
}
