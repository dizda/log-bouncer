use crate::output::OutputAdapter;
use async_trait::async_trait;
use std::error::Error;

#[async_trait]
impl OutputAdapter for StdOut {
    async fn send(&self, line: String) -> Result<(), Box<dyn Error>> {
        println!("got = {}", line);

        // test an error:
        // let not_found = std::io::Error::from(std::io::ErrorKind::NotFound);
        // Err(not_found)?;

        //TODO: make sure amqp gets the ACK before moving to the next message
        Ok(())
    }
}

pub struct StdOut {}
