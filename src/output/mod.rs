pub mod stdout;

use async_trait::async_trait;
use std::error::Error;

#[async_trait]
pub trait OutputAdapter {
    async fn send(&self, line: String) -> Result<(), Box<dyn Error>>;
}
