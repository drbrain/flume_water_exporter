use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;

use chrono::DateTime;
use chrono_tz::Tz;

use crate::client;

use std::convert::TryFrom;

#[derive(Clone)]
pub struct Sensor {
    pub sensor: client::Sensor,
    pub last_update: DateTime<Tz>,
}

impl Sensor {
    pub fn with_updated_timestamp(&self, last_update: DateTime<Tz>) -> Sensor {
        Sensor {
            sensor: self.sensor.clone(),
            last_update,
        }
    }
}

impl TryFrom<client::Sensor> for Sensor {
    type Error = anyhow::Error;

    fn try_from(sensor: client::Sensor) -> Result<Self> {
        let location = sensor
            .location
            .as_ref()
            .ok_or_else(|| anyhow!("Fetch devices with location"))?;
        let timezone: Tz = match location.tz.parse() {
            Ok(tz) => tz,
            Err(_) => {
                return Err(anyhow!("Unknown sensor timezone {}", location.tz));
            }
        };

        let last_update = DateTime::parse_from_rfc3339(&sensor.last_seen)
            .with_context(|| format!("Unable to parse sensor last seen time {}", sensor.last_seen))?
            .with_timezone(&timezone);

        Ok(Sensor {
            sensor,
            last_update,
        })
    }
}
