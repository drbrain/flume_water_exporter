use chrono::DateTime;
use chrono::Utc;

use chrono_tz::Tz;

use std::time::SystemTime;

#[derive(Clone)]
pub struct Sensor {
    pub id: String,
    pub location: String,
    pub product: String,
    pub connected: bool,
    pub battery_level: String,
    pub last_update: DateTime<Tz>,
}

impl Sensor {
    pub fn with_updated_timestamp(&self) -> Sensor {
        let id = self.id.clone();
        let location = self.location.clone();
        let connected = self.connected.clone();
        let battery_level = self.battery_level.clone();
        let product = self.product.clone();

        let timezone = self.last_update.timezone();
        let now: DateTime<Utc> = SystemTime::now().into();
        let last_update = now.with_timezone(&timezone);

        Sensor {
            id,
            location,
            product,
            connected,
            battery_level,
            last_update,
        }
    }
}
