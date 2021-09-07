use chrono::Utc;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, Write};
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("SystemTime: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
}

type Result<T> = std::result::Result<T, Error>;

const DATE_FORMAT: &'static str = "%Y-%m-%d-%H-%M-%S";

/// Rotator has 2 missions
///   1. Rotate at launch if target file exists
///   2. Check periodically if file is larger than defined size then rotate
///
/// The rotate will rename the file from `input.log` to `input-%Y-%m-%d-%H-%M-%S.log`
/// eg. `systemd.log.2021-09-07-03-37-53`
pub struct Rotator {
    filename: String,
    interval: Duration,
    state_rx: watch::Receiver<u64>,
    state: SavedState,
    date_format: String,
    max_size: u64,
}

impl Rotator {
    pub fn new(
        filename: &str,
        interval: Duration,
        state_rx: watch::Receiver<u64>,
        max_size: u64,
        date_format: Option<String>,
    ) -> Result<Self> {
        // create if the file hasn't been created
        let _file = Rotator::touch_file(&filename)?;

        let mut saved_state = SavedState::new(filename)?;

        if !saved_state.is_exists() {
            info!("Saved state doesn't exist, we create it");
            saved_state.save(0);
        } else {
            info!("Saved state exists, we recover it");
        }

        Ok(Self {
            filename: filename.to_owned(),
            date_format: date_format.unwrap_or_else(|| DATE_FORMAT.to_owned()),
            state_rx,
            state: saved_state,
            max_size,
            interval,
        })
    }

    /// Create or use a file
    fn touch_file(filename: &str) -> Result<File> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&filename)?;

        Ok(file)
    }

    async fn check_file_exists(&self) -> Result<bool> {
        let metadata = fs::metadata(&self.filename).await?;

        Ok(metadata.is_file())
    }

    async fn can_be_rotated(&self) -> Result<bool> {
        if !self.check_file_exists().await? {
            return Ok(false);
        }

        let metadata = fs::metadata(&self.filename).await?;

        if metadata.len() > self.max_size {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn rotate(&self) -> Result<()> {
        let now = Utc::now();
        let timestamp = now.format(&self.date_format).to_string();
        let new_filename = format!("{}.{}", self.filename, timestamp);
        debug!("Renaming `{}` to `{}`...", &self.filename, new_filename);

        fs::rename(&self.filename, &new_filename).await?;

        info!("File rotated to `{}`", new_filename);

        Ok(())
    }

    /// Launch the cron job
    pub fn watch(mut self) -> JoinHandle<()> {
        tokio::spawn(async move { self.work().await })
    }

    /// The job that execute log rotation
    async fn work(&mut self) {
        info!(
            "Will check for file rotation every {}ms",
            self.interval.as_millis()
        );
        let mut interval = tokio::time::interval(self.interval);

        // first tick completes immediately
        interval.tick().await;

        loop {
            interval.tick().await;
            trace!("Tick: do a job");

            // TODO: Better to use an AtomicU64 here?
            let pos = *self.state_rx.borrow_and_update();

            if let Err(e) = self.state.save(pos) {
                error!("Can't save current state: `{}`", e);
            }

            match self.can_be_rotated().await {
                Ok(res) => {
                    if res {
                        if let Err(e) = self.rotate().await {
                            error!("Can't rotate the file: `{}`", e);
                        }
                    } else {
                        debug!("File can't be rotated, yet");
                    }
                }
                Err(e) => debug!("Can't rotate the file: `{}`", e),
            }

            trace!("Tick: lap");
        }
    }
}

pub struct SavedState {
    filename: String,
    /// State file
    state_file: File,
}

impl SavedState {
    pub fn new(filename: &str) -> Result<Self> {
        let state_filename = format!(".{}-file-trailer-saved-state", filename);

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&state_filename)?;

        Ok(Self {
            filename: filename.to_owned(),
            state_file: file,
        })
    }

    pub fn is_exists(&self) -> bool {
        self.state_file.metadata().unwrap().len() > 0
    }

    pub fn save(&mut self, pos: u64) -> Result<()> {
        debug!("Saving a sate at position <{}>", pos);
        let metadata = std::fs::metadata(&self.filename)?;
        let date_created = metadata.created()?.duration_since(SystemTime::UNIX_EPOCH)?;

        let data = format!("{};{}", date_created.as_secs(), pos);
        self.state_file.set_len(0)?; // truncate the file before writing it
        self.state_file.flush()?;
        self.state_file.write_all(data.as_bytes())?;

        Ok(())
    }
}
