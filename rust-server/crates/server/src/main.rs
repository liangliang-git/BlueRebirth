use blueoath_server::{run, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(ServerConfig::from_args(std::env::args().skip(1))?).await?;
    Ok(())
}
