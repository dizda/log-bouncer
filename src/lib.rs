#[macro_use]
extern crate tracing;

pub mod output;
mod watcher;

use crate::output::stdout::StdOut;
use crate::output::OutputAdapter;
use crate::watcher::Watcher;
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

    let mut watcher = Watcher::new(FILE, tx)?;
    watcher.work();

    send_lines(StdOut {}, rx).await;

    Ok(())
}

/// Send lines to the defined output
async fn send_lines<T: OutputAdapter>(fnc: T, mut rx: Receiver<String>) {
    while let Some(string) = rx.recv().await {
        if let Err(e) = fnc.send(string).await {
            error!("{}", e);
            break; // we exit the software
        }
    }
}
