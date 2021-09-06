pub mod stdout;

use async_trait::async_trait;

#[async_trait]
pub trait OutputAdapter {
    async fn send(&self, line: String);
}
