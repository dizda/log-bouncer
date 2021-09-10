use chrono::Utc;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::io::SeekFrom;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("corrupted saved state: {0}")]
    CorruptedSavedState(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("SystemTime: {0}")]
    SystemTime(#[from] std::time::SystemTimeError),
}

type Result<T> = std::result::Result<T, Error>;

/// Rotator has 2 missions
///   1. Rotate at launch if target file exists
///   2. Check periodically if file is larger than defined size then rotate
///
/// The rotate will rename the file from `input.log` to `input-%Y-%m-%d-%H-%M-%S.log`
/// eg. `systemd.log.2021-09-07-03-37-53`
pub struct Rotator {
    /// Log file that needs to be watched & rotated
    filename: PathBuf,
    /// Rotation checks interval
    rotation_interval: Duration,
    /// Save state interval
    save_state_interval: Duration,
    /// Receive the current offset position on the file
    state_rx: watch::Receiver<u64>,
    /// The SavedState will be saved in a file.
    state: SavedState,
    /// Date format the logs will contain once rotated
    date_format: String,
    /// Rotate after reaching this file size
    max_size: u64,
    /// The position that has to be resumed from
    pos: u64,
}

impl Rotator {
    pub fn new(
        filename: PathBuf,
        rotation_interval: Duration,
        save_state_interval: Duration,
        state_rx: watch::Receiver<u64>,
        max_size: u64,
        date_format: String,
    ) -> Result<Self> {
        // create if the file hasn't been created
        let _file = Rotator::touch_file(&filename)?;

        let mut saved_state = SavedState::new(&filename)?;

        let pos = Self::recover_position(&mut saved_state)?;

        Ok(Self {
            filename: filename.to_owned(),
            date_format,
            state_rx,
            state: saved_state,
            max_size,
            rotation_interval,
            save_state_interval,
            pos,
        })
    }

    /// Get position we should start to read the file from
    pub fn get_position(&self) -> u64 {
        self.pos
    }

    /// Create or use a file
    fn touch_file(filename: &PathBuf) -> Result<File> {
        filename.to_str().expect("Invalid path");

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(filename)?;

        Ok(file)
    }

    fn recover_position(saved_state: &mut SavedState) -> Result<u64> {
        match saved_state.read_file() {
            Ok(pos) => {
                info!("Saved state exists, we recover it");
                Ok(pos)
            }
            Err(e) => match e {
                Error::CorruptedSavedState(_) => {
                    warn!("Corrupted saved state, we create a new one");
                    let pos = 0; // starts from scratch
                    saved_state.save(pos).unwrap();
                    Ok(pos)
                }
                _ => Err(e),
            },
        }
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

    /// Move a file then create a new one
    async fn rotate(&self) -> Result<()> {
        let now = Utc::now();
        let timestamp = now.format(&self.date_format).to_string();
        let new_filename = format!("{}.{}", self.filename.to_str().unwrap(), timestamp);
        debug!("Renaming {:?} to `{}`...", &self.filename, new_filename);

        fs::rename(&self.filename, &new_filename).await?;
        // then create a new file
        File::create(&self.filename)?;

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
            self.rotation_interval.as_millis()
        );
        let mut rotate_interval = tokio::time::interval(self.rotation_interval);
        let mut state_interval = tokio::time::interval(self.save_state_interval);

        // don't catch up the missed ticks
        rotate_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        state_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // first tick completes immediately
        rotate_interval.tick().await;
        state_interval.tick().await;

        loop {
            tokio::select! {
                _ = rotate_interval.tick() => {
                    trace!("Tick(rotate): do a job");
                    match self.can_be_rotated().await {
                        Ok(res) => {
                            if res {
                                if let Err(e) = self.rotate().await {
                                    error!("Can't rotate the file: `{}`", e);
                                } else {
                                    // file has been rotated, we reset the last position
                                    if let Err(e) = self.state.reset() {
                                        error!("Can't reset the state, after rotating the file: `{}`", e);
                                    }

                                    // we discard this value as we just changed the file
                                    let _pos = *self.state_rx.borrow_and_update();
                                }
                            } else {
                                debug!("File can't be rotated, yet");
                            }
                        }
                        Err(e) => debug!("Can't rotate the file: `{}`", e),
                    }
                }
                _ = state_interval.tick() => {
                    trace!("Tick(state): do a job");

                    // THIS BLOCKS THE THIS ENTIRE LOOP THREAD,
                    // which is okay as we don't need to check the file every X seconds if nothing
                    // has been written in it.
                    self.state_rx.changed().await.expect("State_rx::changed() failed");

                    // get the value
                    let pos = *self.state_rx.borrow_and_update();

                    if let Err(e) = self.state.save(pos) {
                        error!("Can't save current state: `{}`", e);
                    }
                }
            }
        }
    }
}

/// The SavedState will be saved in a file.
pub struct SavedState {
    /// Filename of the log file in order to get the metadata
    filename: PathBuf,
    /// State file
    state_file: File,
    // /// File's `created_at()` timestamp
    // timestamp: u64,
    /// Last position saved
    /// To make sure to not trigger writes every time for nothing
    position: u64,
}

impl SavedState {
    pub fn new(filename: &PathBuf) -> Result<Self> {
        let state_filename = format!(
            ".{}.file-trailer-saved-state",
            (*filename).to_str().unwrap()
        );

        let state_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&state_filename)?;

        Ok(Self {
            filename: filename.to_owned(),
            state_file,
            position: 0,
        })
    }

    /// Recover the saved state if exists
    pub fn read_file(&mut self) -> Result<u64> {
        let metadata = self.state_file.metadata().unwrap();

        if metadata.len() > 0 {
            let mut string = String::new();
            self.state_file.read_to_string(&mut string)?;

            let state = string
                .split(";")
                .map(|e| e.parse::<u64>())
                .filter_map(std::result::Result::ok)
                .collect::<Vec<u64>>();

            if state.len() != 2 {
                Err(Error::CorruptedSavedState("Invalid size".into()))?;
            }

            let file_created_at = state.get(0).unwrap(); // unwrap() is safe here

            if *file_created_at == self.get_date_created()? {
                // same file, we recover the saved position
                Ok(state.get(1).unwrap().clone()) // unwrap() is safe here too
            } else {
                // this is a new file, we start from 0
                Ok(0)
            }
        } else {
            // The state hasn't existed yet, we start from position 0
            Ok(0)
        }
    }

    /// Get the `created_at` from the file, converted to a timestamp
    pub fn get_date_created(&self) -> Result<u64> {
        let metadata = std::fs::metadata(&self.filename)?;

        if let Ok(date_created) = metadata.created() {
            Ok(date_created
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs())
        } else {
            Ok(0u64)
        }
    }

    /// Reset the position to the beginning of the file
    pub fn reset(&mut self) -> Result<()> {
        self.save(0)
    }

    /// Save state in a file
    pub fn save(&mut self, pos: u64) -> Result<()> {
        debug!("Saving a state at position <{}>", pos);

        let data = format!("{};{}", self.get_date_created()?, pos);
        self.state_file.set_len(0)?; // truncate the file before writing it
        self.state_file.seek(SeekFrom::Start(0))?; // reset the cursor position to the beginning
        self.state_file.write_all(data.as_bytes())?;

        self.position = pos;

        Ok(())
    }
}
