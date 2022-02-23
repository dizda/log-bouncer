#[forbid(unsafe_code)]
#[macro_use]
extern crate tracing;

pub mod opt;
pub mod output;
mod publisher;
mod reader;
mod rotator;
mod tail;

pub use opt::{parse, Opt};

use crate::output::amqp::AmqpOutput;
use crate::publisher::Publisher;
use crate::reader::{LineInfo, Reader};
use crate::rotator::Rotator;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub async fn run(opts: Opt) -> Result<(), Box<dyn Error>> {
    // Build a logger subscriber
    let log = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env());

    if opts.json {
        // activates json logging output
        log.json().finish().init();
    } else {
        // or simply plain text
        log.finish().init();
    }

    info!("Started!");

    // Bounded 1 channel to make sure the watcher won't make any more progress in case rabbitmq
    // doesn't accept any more items.
    let (publish_tx, publish_rx) = mpsc::channel::<LineInfo>(opts.buffer_publish);
    // The last position of the file to sync
    let (state_tx, state_rx) = watch::channel::<u64>(0);

    // in case the user submit "test.log", canonicalize will get the absolute path
    let absolute_path = std::fs::canonicalize(&opts.file)?;

    // Rotate the file periodically
    let rotator = Rotator::new(
        absolute_path.clone(),
        Duration::from_secs(opts.rotate_file_interval),
        Duration::from_millis(opts.save_state_interval),
        state_rx,
        opts.max_filesize,
        opts.date_format,
    )?;
    state_tx.send(rotator.get_position())?; // we store the last position

    // Tail the file and send new entries
    let tail = Reader::new(absolute_path, rotator.get_position(), publish_tx)?;
    let watcher = tail.work();

    let rotator_handle = rotator.watch();

    // let output = output::stdout::StdOut {};
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
