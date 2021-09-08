#[macro_use]
extern crate log;

pub mod output;
mod publisher;
mod rotator;
mod watcher;

use crate::output::amqp::AmqpOutput;
use crate::output::stdout::StdOut;
use crate::publisher::Publisher;
use crate::rotator::Rotator;
use crate::watcher::{LineInfo, Watcher};
use std::error::Error;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

const FILE: &'static str = "test.log";
const MAX_FILESIZE: u64 = 1000;
const ROTATE_FILE_PERIOD: Duration = Duration::from_secs(5);
const FILENAME_DATE_FORMAT: Option<&'static str> = None;

// TODO: 1. Add Opt such as Clap
//       2. Rotate at a Line break

pub async fn run() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Started!");

    // Bounded 1 channel to make sure the watcher won't make any more progress in case rabbitmq
    // doesn't accept any more items.
    let (publish_tx, publish_rx) = mpsc::channel::<LineInfo>(1);
    // The last position of the file to sync
    let (state_tx, state_rx) = watch::channel::<u64>(0);

    // Rotate the file periodically
    let rotator = Rotator::new(
        FILE,
        ROTATE_FILE_PERIOD,
        state_rx,
        MAX_FILESIZE,
        FILENAME_DATE_FORMAT,
    )?;
    state_tx.send(rotator.get_position()); // we store the last position

    // Tail the file and send new entries
    let watcher = Watcher::new(FILE, rotator.get_position(), publish_tx)?.work();

    let rotator_handle = rotator.watch();

    // let output = StdOut {};
    let output = AmqpOutput::new(
        "amqp://guest:guest@127.0.0.1:5672/%2f",
        "traffic_exchange",
        "traffic_log",
    )
    .await?;

    // Send the new entries to the publisher, eg. amqp
    let mut publisher = Publisher::new(output, publish_rx, state_tx);

    tokio::select! {
        _ = rotator_handle => {}
        _ = watcher.notified() => {}
        _ = publisher.publish() => {}
    };

    Ok(())
}
