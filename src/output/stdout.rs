use crate::output::OutputAdapter;
use async_trait::async_trait;

#[async_trait]
impl OutputAdapter for StdOut {
    async fn send(&self, line: String) {
        println!("got = {}", line);
    }
}

pub struct StdOut {}
