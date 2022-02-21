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
    filepath: PathBuf,
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
        filepath: PathBuf,
        rotation_interval: Duration,
        save_state_interval: Duration,
        state_rx: watch::Receiver<u64>,
        max_size: u64,
        date_format: String,
    ) -> Result<Self> {
        info!("Watching the logfile `{}`...", filepath.to_string_lossy());

        // create if the file hasn't been created
        let _file = Rotator::touch_file(&filepath)?;

        let mut saved_state = SavedState::new(&filepath)?;

        let pos = Self::recover_position(&mut saved_state)?;

        Ok(Self {
            filepath: filepath.to_owned(),
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
        let metadata = fs::metadata(&self.filepath).await?;

        Ok(metadata.is_file())
    }

    async fn can_be_rotated(&self) -> Result<bool> {
        if !self.check_file_exists().await? {
            return Ok(false);
        }

        let metadata = fs::metadata(&self.filepath).await?;

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
        let new_filename = format!("{}.{}", self.filepath.to_str().unwrap(), timestamp);
        debug!("Renaming {:?} to `{}`...", &self.filepath, new_filename);

        fs::rename(&self.filepath, &new_filename).await?;
        // then create a new file
        File::create(&self.filepath)?;

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
                // _ = rotate_interval.tick() => {
                //     trace!("Tick(rotate): do a job");
                //     match self.can_be_rotated().await {
                //         Ok(res) => {
                //             if res {
                //                 if let Err(e) = self.rotate().await {
                //                     error!("Can't rotate the file: `{}`", e);
                //                 } else {
                //                     // file has been rotated, we reset the last position
                //                     if let Err(e) = self.state.reset() {
                //                         error!("Can't reset the state, after rotating the file: `{}`", e);
                //                     }
                //
                //                     // we discard this value as we just changed the file
                //                     let _pos = *self.state_rx.borrow_and_update();
                //                 }
                //             } else {
                //                 debug!("File can't be rotated, yet");
                //             }
                //         }
                //         Err(e) => debug!("Can't rotate the file: `{}`", e),
                //     }
                // }
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

use crc::{Algorithm, Crc, CRC_32_ISCSI};
pub const HASHER: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// The SavedState will be saved in a file.
pub struct SavedState {
    /// Filename of the log file in order to get the first line
    filepath: PathBuf,
    /// State file
    state_file: File,
    /// Last position saved
    /// To make sure to not trigger writes every time for nothing
    position: u64,
}

impl SavedState {
    pub fn new(filepath: &PathBuf) -> Result<Self> {
        // get the filename of the logfile
        let file_name = (*filepath)
            .file_name()
            .expect("Can't get the filename of the logfile")
            .to_str()
            .unwrap();
        // using the same directory for our saved state
        let mut state_filepath = filepath
            .parent()
            .expect("Can not get parent directory")
            .to_path_buf();
        let state_filename = format!(".{}.log-bouncer", file_name);

        // using the same directory but a different filename (prefixed with ".")
        state_filepath.push(state_filename);

        debug!(
            "Store the state in the file `{}`",
            state_filepath.to_string_lossy()
        );

        let state_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&state_filepath)?;

        Ok(Self {
            filepath: filepath.to_owned(),
            state_file,
            position: 0,
        })
    }

    /// Recover the saved state if exists
    pub fn read_file(&mut self) -> Result<u64> {
        let mut string = String::new();
        self.state_file.read_to_string(&mut string)?;

        let state = string
            .split(";")
            .map(|e| e.parse::<u64>())
            .filter_map(std::result::Result::ok)
            .collect::<Vec<u64>>();

        if state.len() != 2 {
            Err(Error::CorruptedSavedState(
                "State should contains 2 entries".into(),
            ))?;
        }

        // we recover file's uniq id, which is a u32
        let uniq_id = *state.get(0).unwrap() as u32; // unwrap() is safe here
        debug!("Recovered uniq_id of the file `{}`", uniq_id);

        if uniq_id == self.get_uniq_id()? {
            // same file, we recover the saved position
            Ok(state.get(1).unwrap().clone()) // unwrap() is safe here too
        } else {
            // this is a new file, we start from 0
            Ok(0)
        }
    }

    /// Get the `created_at` from the file, converted to a timestamp
    ///
    /// Seems to not work on a docker image... because of being built in static?
    pub fn get_uniq_id(&self) -> Result<u32> {
        use std::io::{BufRead, BufReader, Cursor};

        let file = File::open(&self.filepath)?;
        let mut reader = BufReader::new(file);

        let mut first_line = String::new();
        reader.read_line(&mut first_line)?;

        let first_line = first_line.trim();
        debug!("File's first line content is `{}`", &first_line);

        let hashed = HASHER.checksum(first_line.as_bytes());
        debug!("File's first line hash is `{}`", hashed);

        Ok(hashed)
    }

    /// Reset the position to the beginning of the file
    pub fn reset(&mut self) -> Result<()> {
        self.save(0)
    }

    /// Save state in a file
    pub fn save(&mut self, pos: u64) -> Result<()> {
        debug!("Saving a state at position <{}>", pos);

        let data = format!("{};{}", self.get_uniq_id()?, pos);
        self.state_file.set_len(0)?; // truncate the file before writing it
        self.state_file.seek(SeekFrom::Start(0))?; // reset the cursor position to the beginning
        self.state_file.write_all(data.as_bytes())?;

        self.position = pos;

        Ok(())
    }
}
