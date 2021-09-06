use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    file_trailer::run().await
}
