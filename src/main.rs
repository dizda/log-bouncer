use log_bouncer::parse;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    log_bouncer::run(parse()).await
}
