use crate::output::OutputAdapter;
use async_trait::async_trait;
use std::error::Error;

#[derive(thiserror::Error, Debug)]
pub enum StdOutError {
    #[error("corrupted line found")]
    Corrupted,
}

#[async_trait]
impl OutputAdapter for StdOut {
    async fn send(&self, _position: u64, line: String) -> Result<(), Box<dyn Error>> {
        info!("got = {}", line);

        // if line.chars().last().unwrap() != '}' {
        //     Err(StdOutError::Corrupted)?;
        // }

        Ok(())
    }
}

pub struct StdOut {}
