use crate::ServerConfig;
use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

pub fn init_logging(config: &ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let log_path = default_log_file();
    if let Some(parent) = log_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let filter = config.log_level.filter();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(io::stderr)
                .with_ansi(false)
                .with_filter(filter),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file)
                .with_ansi(false)
                .with_filter(filter),
        )
        .try_init()?;
    Ok(())
}

fn default_log_file() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("rust-server.log")))
        .unwrap_or_else(|| PathBuf::from("rust-server.log"))
}
