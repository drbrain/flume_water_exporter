use crate::client::Client;
use crate::device::Device;
use crate::sensor::Sensor;

use std::time::Instant;

#[derive(Clone)]
pub struct Flume {
    pub client: Client,

    pub access_token: String,
    pub refresh_token: String,
    pub token_expires_at: Instant,
}

impl Flume {
    pub async fn devices(&mut self, user_id: i64) -> Option<Vec<Device>> {
        self.refresh_token_if_expired().await;

        self.client.devices(&self.access_token, user_id).await
    }

    pub async fn query_sensor(&mut self, user_id: i64, sensor: &Sensor) -> Option<f64> {
        self.refresh_token_if_expired().await;

        self.client
            .query_samples(&self.access_token, user_id, sensor)
            .await
    }

    async fn refresh_token_if_expired(&mut self) {
        if Instant::now() < self.token_expires_at {
            return;
        };

        let (access_token, refresh_token, token_expires_at) =
            self.client.refresh_token(&self.refresh_token).await;

        self.access_token = access_token;
        self.refresh_token = refresh_token;
        self.token_expires_at = token_expires_at;
    }

    pub async fn user_id(&mut self) -> Option<i64> {
        self.refresh_token_if_expired().await;

        self.client.user_id(&self.access_token).await
    }
}
