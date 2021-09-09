#[macro_use]
extern crate log;

pub mod opt;
pub mod output;
mod publisher;
mod rotator;
mod watcher;

pub use opt::{parse, Opt};

use crate::output::amqp::AmqpOutput;
use crate::output::stdout::StdOut;
use crate::publisher::Publisher;
use crate::rotator::Rotator;
use crate::watcher::{LineInfo, TailReader};
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

// TODO: 1. Rotate at a Line break

pub async fn run(opts: Opt) -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Started!");

    // Bounded 1 channel to make sure the watcher won't make any more progress in case rabbitmq
    // doesn't accept any more items.
    let (publish_tx, publish_rx) = mpsc::channel::<LineInfo>(opts.buffer_publish);
    // The last position of the file to sync
    let (state_tx, state_rx) = watch::channel::<u64>(0);

    // Rotate the file periodically
    let rotator = Rotator::new(
        opts.file.clone(),
        Duration::from_secs(opts.rotate_file_interval),
        state_rx,
        opts.max_filesize,
        opts.date_format,
    )?;
    state_tx.send(rotator.get_position()); // we store the last position

    // Tail the file and send new entries
    let watcher = TailReader::new(opts.file, rotator.get_position(), publish_tx)?.work();

    let rotator_handle = rotator.watch();

    // let output = StdOut {};
    let output =
        AmqpOutput::new(&opts.amqp_uri, &opts.amqp_exchange, &opts.amqp_routing_key).await?;

    // Send the new entries to the publisher, eg. amqp
    let mut publisher = Publisher::new(output, publish_rx, state_tx);

    tokio::select! {
        _ = rotator_handle => {}
        _ = watcher.notified() => {}
        _ = publisher.publish() => {}
    };

    Ok(())
}
