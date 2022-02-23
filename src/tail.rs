//! Credits to https://git.staart.one/ajmartinez/staart/src/branch/main/src/lib.rs
//!
//! Modified version of `staart` is a Rust implementation of a tail-like program
//!
//! The library exposes public methods to allow other programs to follow a file
//! internally. These methods are exposed on a struct [`TailedFile`].
//!
//! # Example
//!
//! ```no_run
//! use std::thread::sleep;
//! use std::time::Duration;
//! use staart::{StaartError, TailedFile};
//!
//! fn main() -> Result<(), StaartError> {
//!     let delay = Duration::from_millis(100);
//!     let args: Vec<String> = std::env::args().collect();
//!     let path = &args[1].as_str();
//!     let mut f = TailedFile::new(path)?;
//!     loop {
//!        f.follow()?;
//!        sleep(delay);
//!     }
//! }
//! ```
use std::fs::{File, Metadata};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[cfg(target_os = "linux")]
use std::os::linux::fs::MetadataExt;

#[cfg(target_os = "macos")]
use std::os::macos::fs::MetadataExt;

type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("i/o: {0}")]
    IO(#[from] std::io::Error),
    #[error("str-utf8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("from-utf8: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    #[error("int-error: {0}")]
    IntError(#[from] std::num::TryFromIntError),
}

/// [`TailedFile`] tracks the state of a file being followed. It offers
/// methods for updating this state, and printing data to `stdout`.
pub struct TailedFile<T> {
    path: T,
    pos: u64,
    meta: Metadata,
}

impl<T> TailedFile<T>
where
    T: AsRef<Path> + Copy,
{
    /// Creates an instance of `std::io::Result<staart::TailedFile>`
    ///
    /// # Example
    /// ```no_run
    /// let mut f = staart::TailedFile::new("/var/log/syslog");
    /// ```
    ///
    /// # Propagates Errors
    /// - If the path provided does not exist, or is not readable by the current user
    /// - If file metadata can not be read
    pub fn new(path: T) -> Result<TailedFile<T>> {
        let f = File::open(path)?;
        let meta = f.metadata()?;
        let pos = meta.len();
        Ok(TailedFile { path, pos, meta })
    }

    /// Reads new data for an instance of `staart::TailedFile` and returns
    /// `Result<Vec<u8>>`
    pub fn read(&mut self, file: &File) -> Result<Vec<u8>> {
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        reader.seek(SeekFrom::Start(self.pos))?;
        let n: u64 = reader.read_line(&mut line)?.try_into()?;
        self.pos += n;
        let data: Vec<u8> = line.collect();
        Ok(data)
    }

    /// Prints new data read on an instance of `staart::TailedFile` to `stdout`
    pub fn follow(&mut self) -> Result<String> {
        let fd = File::open(self.path)?;
        self.check_rotate(&fd)?;
        self.check_truncate(&fd)?;
        let data = self.read(&fd)?;

        Ok(String::from_utf8(data)?)
    }

    /// Prints new data read on an instance of `staart::TailedFile` to `stdout`
    pub fn follow_print_stdout(&mut self) -> Result<()> {
        let fd = File::open(self.path)?;
        self.check_rotate(&fd)?;
        self.check_truncate(&fd)?;
        let data = self.read(&fd)?;
        if let Ok(s) = String::from_utf8(data) {
            print!("{s}")
        }
        Ok(())
    }

    /// Checks for file rotation by inode comparison in Linux-like systems
    fn check_rotate(&mut self, fd: &File) -> Result<()> {
        let meta = fd.metadata()?;
        let inode = meta.st_ino();
        if inode != self.meta.st_ino() {
            self.pos = 0;
            self.meta = meta;
        }
        Ok(())
    }

    /// Checks for file truncation by length comparison to the previous read position
    fn check_truncate(&mut self, fd: &File) -> Result<()> {
        let meta = fd.metadata()?;
        let inode = meta.st_ino();
        let len = meta.len();
        if inode == self.meta.st_ino() && len < self.pos {
            self.pos = 0;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[cfg(target_os = "linux")]
    use std::os::linux::fs::MetadataExt;

    #[cfg(target_os = "macos")]
    use std::os::macos::fs::MetadataExt;

    #[test]
    fn tailed_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = &dir.path().join("test.file");
        let _f = File::create(&path).unwrap();
        let tailed_file = TailedFile::new(&path);
        assert!(tailed_file.is_ok())
    }

    #[test]
    fn test_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = &dir.path().join("test.file");
        let test_data = b"Some data";
        let mut f = File::create(&path).unwrap();
        let mut tailed_file = TailedFile::new(&path).unwrap();
        f.write_all(test_data).unwrap();
        let f = File::open(&path).unwrap();
        let data = tailed_file.read(&f).unwrap();
        assert_eq!(data.len(), test_data.len());
        assert_eq!(tailed_file.pos, 9);
    }

    #[test]
    fn test_check_rotate() {
        let dir = tempfile::tempdir().unwrap();
        let path = &dir.path().join("test.file");
        let path2 = &dir.path().join("test2.file");
        let test_data = b"Some data";
        let more_test_data = b"fun";
        let mut f = File::create(&path).unwrap();
        f.write_all(test_data).unwrap();
        let mut tailed_file = TailedFile::new(&path).unwrap();
        std::fs::rename(&path, &path2).unwrap();
        let mut f = File::create(&path).unwrap();
        f.write_all(more_test_data).unwrap();
        tailed_file.check_rotate(&f).unwrap();

        assert_eq!(tailed_file.meta.st_ino(), f.metadata().unwrap().st_ino());
    }

    #[test]
    fn test_check_truncate() {
        let dir = tempfile::tempdir().unwrap();
        let path = &dir.path().join("test.file");
        let test_data = b"Some data";
        let more_test_data = b"fun";
        let mut f = File::create(&path).unwrap();
        f.write_all(test_data).unwrap();
        let mut tailed_file = TailedFile::new(&path).unwrap();
        let mut f = File::create(&path).unwrap();
        f.write_all(more_test_data).unwrap();
        tailed_file.check_truncate(&f).unwrap();
        assert_eq!(tailed_file.pos, 0)
    }
}
