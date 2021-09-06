#[macro_use]
extern crate log;

pub mod output;
mod publisher;
mod rotator;
mod watcher;

use crate::output::stdout::StdOut;
use crate::publisher::Publisher;
use crate::rotator::Rotator;
use crate::watcher::Watcher;
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc;

const FILE: &'static str = "test.log";

// TODO: 1. Add Opt such as Clap or StructOpt

pub async fn run() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Started!");

    let (tx, rx) = mpsc::channel::<String>(1);

    let rotator = Rotator::new(FILE, Duration::from_secs(5));
    let watcher = Watcher::new(FILE, tx)?;
    rotator.watch();
    watcher.work();

    Publisher::new(StdOut {}, rx).publish().await;

    Ok(())
}
