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
    #[error("the file has been rotated, file's position has been reset to 0")]
    FileRotated,
    #[error("the file has been truncated, file's position has been reset to 0")]
    FileTruncated,
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

    /// Reads new lines and return the ones that finishes with line breaker "\n"
    pub fn read(&mut self, file: &File) -> Result<Vec<String>> {
        let mut reader = BufReader::new(file);
        let mut lines = vec![];
        reader.seek(SeekFrom::Start(self.pos))?;

        loop {
            let mut line = String::new();
            let n: u64 = reader.read_line(&mut line)? as u64;

            if n == 0 || !line.ends_with('\n') {
                // EOF or the line doesn't contain a line breaker, therefore shouldn't be added
                break;
            }

            lines.push(line.replace('\n', "")); // line breakers should be removed
            self.pos += n;
        }

        Ok(lines)
    }

    /// Prints new data read on an instance of `staart::TailedFile` to `stdout`
    pub fn follow(&mut self) -> Result<Vec<String>> {
        let fd = File::open(self.path)?;
        self.has_been_rotated(&fd)?;
        self.has_been_truncated(&fd)?;
        let data = self.read(&fd)?;

        Ok(data)
    }

    /// Checks for file rotation by inode comparison in Linux-like systems
    fn has_been_rotated(&mut self, fd: &File) -> Result<()> {
        let meta = fd.metadata()?;
        let inode = meta.st_ino();
        if inode != self.meta.st_ino() {
            self.pos = 0;
            self.meta = meta;

            Err(Error::FileRotated)?; // trigger an error
        }

        Ok(())
    }

    /// Checks for file truncation by length comparison to the previous read position
    fn has_been_truncated(&mut self, fd: &File) -> Result<()> {
        let meta = fd.metadata()?;
        let inode = meta.st_ino();
        let len = meta.len();
        if inode == self.meta.st_ino() && len < self.pos {
            self.pos = 0;

            Err(Error::FileTruncated)?; // trigger an error
        }

        Ok(())
    }

    pub fn pos(&self) -> u64 {
        self.pos
    }

    pub fn set_pos(&mut self, pos: u64) {
        self.pos = pos
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

    /// All lines contains line breakers "\n"
    /// they should be added as they're considered as fully written
    #[test]
    fn test_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = &dir.path().join("test.file");
        let test_data = b"{\"data\":\"coucou1\"}
{\"data\":\"coucou2\"}
{\"data\":\"coucou3\"}
";

        let mut f = File::create(&path).unwrap();
        let mut tailed_file = TailedFile::new(&path).unwrap();
        f.write_all(test_data).unwrap();
        let f = File::open(&path).unwrap();
        let read_data = tailed_file.read(&f).unwrap();

        assert_eq!(read_data.len(), 3);
        assert_eq!(tailed_file.pos, test_data.len() as u64);

        for line in read_data {
            // making sure line breakers have been removed
            assert!(!line.contains('\n'));
        }
    }

    /// The last line doesn't contain a line breaker, thus it should be ignored because the line may
    /// have not been finished to be written.
    #[test]
    fn test_read_with_one_missing_line_breaker() {
        let dir = tempfile::tempdir().unwrap();
        let path = &dir.path().join("test.file");
        let test_data = b"{\"data\":\"coucou1\"}
{\"data\":\"coucou2\"}
{\"data\":\"coucou3\"}";

        let mut f = File::create(&path).unwrap();
        let mut tailed_file = TailedFile::new(&path).unwrap();
        f.write_all(test_data).unwrap();
        let f = File::open(&path).unwrap();
        let read_data = tailed_file.read(&f).unwrap();
        assert_eq!(read_data.len(), 2); // only 2 here
        assert_eq!(tailed_file.pos, 38); // and the position should be before the third line
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

        assert_eq!(
            "Err(FileRotated)",
            format!("{:?}", tailed_file.has_been_rotated(&f))
        );
        assert_eq!(tailed_file.meta.st_ino(), f.metadata().unwrap().st_ino());
        assert_eq!(tailed_file.pos, 0)
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
        assert_eq!(
            "Err(FileTruncated)",
            format!("{:?}", tailed_file.has_been_truncated(&f))
        );
        assert_eq!(tailed_file.pos, 0)
    }
}
