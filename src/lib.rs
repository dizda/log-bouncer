#[macro_use]
extern crate tracing;

pub mod output;
mod publisher;
mod watcher;

use crate::output::stdout::StdOut;
use crate::output::OutputAdapter;
use crate::publisher::Publisher;
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

    Publisher::new(StdOut {}, rx).publish().await;

    Ok(())
}
