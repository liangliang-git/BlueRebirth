use blueoath_server::{run, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_args(std::env::args().skip(1))?;
    blueoath_server::init_logging(&config)?;
    run(config).await?;
    Ok(())
}
