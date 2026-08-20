use std::{net::SocketAddr, sync::Arc};

use thiserror::Error;
use tokio::net::TcpListener;

use crate::{
    api, config::Config, control::ShutdownSignal, quota::client::QuotaClient,
    windows::WindowsSingleInstance,
};

const MUTEX_NAME: &str = r"Local\Background_Codex_Quota";

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error("failed to create quota client: {0}")]
    Client(String),
    #[error("failed to bind {address}: {source}")]
    Bind {
        address: String,
        source: std::io::Error,
    },
    #[error("HTTP server failed: {0}")]
    Server(String),
    #[error("Windows runtime failed: {0}")]
    Windows(#[from] std::io::Error),
}

pub fn run() -> Result<(), RuntimeError> {
    let instance = match WindowsSingleInstance::acquire(MUTEX_NAME)? {
        Some(instance) => instance,
        None => {
            eprintln!("backend is already running");
            return Ok(());
        }
    };

    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| RuntimeError::Server(error.to_string()))?
        .block_on(run_async());
    drop(instance);
    result
}

async fn run_async() -> Result<(), RuntimeError> {
    let config = Config::from_env()?;
    let client =
        Arc::new(QuotaClient::new().map_err(|error| RuntimeError::Client(error.to_string()))?);
    let signal = Arc::new(ShutdownSignal::default());
    let router = api::router(client, Arc::clone(&signal));
    let address = SocketAddr::new(config.host, config.port);
    let listener = TcpListener::bind(address)
        .await
        .map_err(|source| RuntimeError::Bind {
            address: address.to_string(),
            source,
        })?;
    let server_signal = Arc::clone(&signal);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { server_signal.wait().await })
        .await
        .map_err(|error| RuntimeError::Server(error.to_string()))
}
