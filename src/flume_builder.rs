use anyhow::Result;

use crate::client::Client;
use crate::configuration::Configuration;
use crate::flume::Flume;

pub struct FlumeBuilder {
    configuration: Configuration,
}

impl FlumeBuilder {
    pub fn from_configuration(configuration: Configuration) -> Self {
        FlumeBuilder { configuration }
    }

    pub async fn build(self) -> Result<Flume> {
        let mut client = Client::new(&self.configuration);

        let (access_token, refresh_token, token_expires_in, token_fetch_time) = client
            .access_token(
                &self.configuration.username(),
                &self.configuration.password(),
            )
            .await?;

        Ok(Flume {
            client,

            access_token,
            refresh_token,
            token_expires_in,
            token_fetch_time,
        })
    }
}
