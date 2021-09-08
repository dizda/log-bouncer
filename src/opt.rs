use clap::{AppSettings, Clap};
use std::path::PathBuf;

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
    #[clap(parse(from_os_str), short, long, env)]
    pub file: PathBuf,

    /// If the filesize go beyond that value, the file will get rotated
    #[clap(short, long, default_value = "1000", env)]
    pub max_filesize: u64,

    /// Check if the file needs to be rotated
    /// value in seconds
    #[clap(short, long, default_value = "5", env)]
    pub rotate_file_interval: u64,

    /// Rotated files will have a date on their filenames,
    /// can change the current structure `%Y-%m-%d-%H-%M-%S`
    #[clap(short, long)]
    pub date_format: Option<String>,

    /// Uri of the AMQP server to publish to
    #[clap(long, default_value = "amqp://guest:guest@127.0.0.1:5672/%2f", env)]
    pub amqp_uri: String,

    #[clap(long, env)]
    pub amqp_exchange: String,

    #[clap(long, env)]
    pub amqp_routing_key: String,
}

pub fn parse() -> Opt {
    Opt::parse()
}
