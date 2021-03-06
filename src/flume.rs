use anyhow::Result;

use chrono::offset::Utc;
use chrono::DateTime;
use chrono_tz::Tz;

use crate::client;
use crate::client::Budget;
use crate::client::Client;
use crate::device::Device;
use crate::sensor::Sensor;

use std::time::Duration;
use std::time::Instant;

#[derive(Clone)]
pub struct Flume {
    pub client: Client,

    pub access_token: String,
    pub refresh_token: String,
    pub token_expires_in: u64,
    pub token_fetch_time: Instant,
}

impl Flume {
    pub async fn budgets(&mut self, user_id: i64, sensor: &Sensor) -> Result<Vec<Budget>> {
        self.refresh_token_if_expired().await?;

        let sensor_id = sensor.sensor.id.clone();

        self.client
            .budgets(&self.access_token, user_id, &sensor_id)
            .await
    }

    pub async fn devices(&mut self, user_id: i64) -> Result<Vec<Device>> {
        self.refresh_token_if_expired().await?;

        self.client
            .devices(&self.access_token, user_id)
            .await?
            .iter()
            .map(|d: &client::Device| Device::try_from(d.clone()))
            .collect()
    }

    pub async fn query_sensor(
        &mut self,
        user_id: i64,
        sensor: &Sensor,
    ) -> Result<(f64, DateTime<Tz>)> {
        self.refresh_token_if_expired().await?;

        let last_update = sensor.last_update;
        let timezone = last_update.timezone();
        let since_datetime = last_update.format("%F %H:%M:00").to_string();
        let now = Utc::now().with_timezone(&timezone);
        let until_datetime = Some(now.format("%F %H:%M:00").to_string());

        let query = client::Query {
            request_id: since_datetime.clone(),
            bucket: client::QueryBucket::MIN,
            since_datetime,
            until_datetime,
            operation: Some(client::QueryOperation::SUM),
            units: Some(client::QueryUnits::LITERS),
            ..Default::default()
        };

        let new_usage = self
            .client
            .query_samples(&self.access_token, user_id, &sensor.sensor.id, query)
            .await?;

        Ok((new_usage, now))
    }

    async fn refresh_token_if_expired(&mut self) -> Result<bool> {
        let expiry = Duration::from_secs(self.token_expires_in);

        if Instant::now().duration_since(self.token_fetch_time) < expiry {
            return Ok(false);
        };

        let (token, token_fetch_time) = self.client.refresh_token(&self.refresh_token).await?;

        self.access_token = token.access_token;
        self.refresh_token = token.refresh_token;
        self.token_expires_in = token.expires_in;
        self.token_fetch_time = token_fetch_time;

        Ok(true)
    }

    pub async fn user_id(&mut self) -> Result<i64> {
        self.refresh_token_if_expired().await?;

        self.client.user_id(&self.access_token).await
    }
}
