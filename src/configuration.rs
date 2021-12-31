use anyhow::Context;
use anyhow::Result;

use serde::Deserialize;

use std::fs;
use std::path::Path;

#[derive(Clone, Default, Deserialize)]
pub struct Configuration {
    bind_address: Option<String>,
    client_id: String,
    secret_id: String,
    username: String,
    password: String,
    refresh_interval: Option<u64>,
    refresh_timeout: Option<u64>,
}

impl Configuration {
    /// Load a configuration file from `path`.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let source = fs::read_to_string(path)?;

        toml::from_str(&source).context("Invalid configuration file")
    }

    /// Load configuration from the next argument in the environment.
    pub fn load_from_next_arg() -> Result<Self> {
        let file = match std::env::args().nth(1) {
            None => {
                return Ok(Configuration::default());
            }
            Some(f) => f,
        };

        Configuration::load(&file).with_context(|| format!("Unable to load {}", file))
    }

    /// Bind address for Prometheus metric server
    pub fn bind_address(&self) -> String {
        self.bind_address
            .as_ref()
            .unwrap_or(&"0.0.0.0:9160".to_string())
            .to_string()
    }

    pub fn client_id(&self) -> String {
        self.client_id.clone()
    }

    pub fn secret_id(&self) -> String {
        self.secret_id.clone()
    }

    pub fn username(&self) -> String {
        self.username.clone()
    }

    pub fn password(&self) -> String {
        self.password.clone()
    }

    /// Interval between fetching data from Flume.
    ///
    /// Defaults to 60 seconds, the Flume Water API has a rate limit of 120 requests per hour.
    pub fn refresh_interval(&self) -> std::time::Duration {
        let interval = self.refresh_interval.unwrap_or(60_000);

        std::time::Duration::from_millis(interval)
    }

    /// Timeout to wait for the Flume API to respond.  Defaults to 10s.
    pub fn refresh_timeout(&self) -> std::time::Duration {
        let timeout = self.refresh_timeout.unwrap_or(10_000);

        std::time::Duration::from_millis(timeout)
    }
}
