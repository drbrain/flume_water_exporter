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
    budget_interval: Option<u64>,
    device_interval: Option<u64>,
    query_interval: Option<u64>,
    flume_timeout: Option<u64>,
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

    /// Interval between fetching budget data from Flume in seconds.
    ///
    /// Defaults to 60 minutes, the Flume Water API has a rate limit of 120 requests per hour.
    pub fn budget_interval(&self) -> std::time::Duration {
        let interval = self.budget_interval.unwrap_or(3600);

        std::time::Duration::from_secs(interval)
    }

    /// Interval between fetching device data (bridge and sensor connected, battery level) from
    /// Flume in seconds.
    ///
    /// Defaults to 5 minutes, the Flume Water API has a rate limit of 120 requests per hour.
    pub fn device_interval(&self) -> std::time::Duration {
        let interval = self.device_interval.unwrap_or(300);

        std::time::Duration::from_secs(interval)
    }

    /// Interval between querying usage data from Flume in seconds.
    ///
    /// Defaults to 60 seconds, the Flume Water API has a rate limit of 120 requests per hour.
    /// Reduce this interval if you have multiple sensors.
    pub fn query_interval(&self) -> std::time::Duration {
        let interval = self.query_interval.unwrap_or(60);

        std::time::Duration::from_secs(interval)
    }

    /// Timeout to wait for the Flume API to respond in milliseconds.  Defaults to 1s.
    pub fn flume_timeout(&self) -> std::time::Duration {
        let timeout = self.flume_timeout.unwrap_or(1_000);

        std::time::Duration::from_millis(timeout)
    }
}
