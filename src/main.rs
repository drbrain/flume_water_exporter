mod bridge;
mod client;
mod configuration;
mod device;
mod downloader;
mod exporter;
mod flume;
mod flume_builder;
mod sensor;

use anyhow::anyhow;
use anyhow::Result;

use log::error;

use configuration::Configuration;
use downloader::Downloader;
use exporter::Exporter;
use flume_builder::FlumeBuilder;

use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let configuration = Configuration::load_from_next_arg()?;

    let (error_tx, error_rx) = mpsc::channel(1);

    let flume = FlumeBuilder::from_configuration(configuration.clone())
        .build()
        .await?;

    Downloader::new(
        flume,
        configuration.budget_interval(),
        configuration.device_interval(),
        configuration.query_interval(),
        error_tx.clone(),
    )
    .start()
    .await;

    Exporter::new(configuration.bind_address())?
        .start(error_tx.clone())
        .await;

    let exit_code = wait_for_error(error_rx).await;

    std::process::exit(exit_code);
}

async fn wait_for_error(mut error_rx: mpsc::Receiver<anyhow::Error>) -> i32 {
    let error = match error_rx.recv().await {
        Some(e) => e,
        None => anyhow!("Error reporting channel closed unexpectedly, bug?"),
    };

    error!("{:#}", error);

    1
}

#[track_caller]
pub(crate) fn spawn_named<T>(
    task: impl std::future::Future<Output = T> + Send + 'static,
    _name: &str,
) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
{
    #[cfg(tokio_unstable)]
    return tokio::task::Builder::new().name(_name).spawn(task);

    #[cfg(not(tokio_unstable))]
    tokio::spawn(task)
}
