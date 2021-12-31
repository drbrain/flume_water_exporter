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

    pub async fn build(self) -> Flume {
        let mut client = Client::new(&self.configuration);

        let (access_token, refresh_token, token_expires_at) = client
            .access_token(
                &self.configuration.username(),
                &self.configuration.password(),
            )
            .await;

        Flume {
            client,

            access_token,
            refresh_token,
            token_expires_at,
        }
    }
}
