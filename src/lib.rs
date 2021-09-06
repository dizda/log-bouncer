#[macro_use]
extern crate log;

pub mod output;
mod publisher;
mod watcher;

use crate::output::stdout::StdOut;
use crate::publisher::Publisher;
use crate::watcher::Watcher;
use std::error::Error;
use tokio::sync::mpsc;

const FILE: &'static str = "test.log";

pub async fn run() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Started!");

    let (tx, rx) = mpsc::channel::<String>(1);

    let watcher = Watcher::new(FILE, tx)?;
    watcher.work();

    Publisher::new(StdOut {}, rx).publish().await;

    Ok(())
}
