use crate::output::OutputAdapter;
use async_trait::async_trait;
use std::error::Error;

#[async_trait]
impl OutputAdapter for StdOut {
    async fn send(&self, _position: u64, line: String) -> Result<(), Box<dyn Error>> {
        println!("got = {}", line);

        // test an error:
        // let not_found = std::io::Error::from(std::io::ErrorKind::NotFound);
        // Err(not_found)?;
        //tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        Ok(())
    }
}

pub struct StdOut {}
