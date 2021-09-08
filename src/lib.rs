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
use clap::{AppSettings, Clap};
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// # File-trailer
///
/// File trailer will listen a log file then:
///
///  - publish any new message to AMQP
///  - rotate logs automatically
///
#[derive(Debug, clap::Clap, Clone)]
#[clap(name = "file-trailer")]
pub struct Opt {
    /// Override the config file
    #[clap(parse(from_os_str), short, long)]
    pub file: PathBuf,

    /// If the filesize go beyond that value, the file will get rotated
    #[clap(short, long, default_value = "1000")]
    pub max_filesize: u64,

    /// Check if the file needs to be rotated
    /// value in seconds
    #[clap(short, long, default_value = "5")]
    pub rotate_file_interval: u64,

    /// Rotated files will have a date on their filenames,
    /// can change the current structure `%Y-%m-%d-%H-%M-%S`
    pub date_format: Option<String>,
}

// TODO: 1. Add Opt such as Clap
//       2. Rotate at a Line break

pub async fn run() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    info!("Started!");

    let opts: Opt = Opt::parse();

    // Bounded 1 channel to make sure the watcher won't make any more progress in case rabbitmq
    // doesn't accept any more items.
    let (publish_tx, publish_rx) = mpsc::channel::<LineInfo>(1);
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
    let watcher = Watcher::new(opts.file, rotator.get_position(), publish_tx)?.work();

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
