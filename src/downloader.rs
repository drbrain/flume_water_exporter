use anyhow::Error;
use anyhow::Result;

use crate::bridge::Bridge;
use crate::device::Device;
use crate::flume::Flume;
use crate::sensor::Sensor;

use lazy_static::lazy_static;

use log::debug;

use prometheus::register_counter_vec;
use prometheus::register_gauge_vec;
use prometheus::CounterVec;
use prometheus::GaugeVec;

use std::time::Duration;
use std::time::Instant;

use tokio::sync::mpsc;
use tokio::time::sleep;

type Sender = mpsc::Sender<anyhow::Error>;

const BATTERY_HIGH: &str = "high";
const BATTERY_MEDIUM: &str = "medium";
const BATTERY_LOW: &str = "low";

lazy_static! {
    static ref BRIDGE_PRODUCT: GaugeVec = register_gauge_vec!(
        "flume_water_bridge_product_info",
        "Flume bridge product",
        &["location", "product"],
    )
    .unwrap();
    static ref BRIDGE_CONNECTED: GaugeVec = register_gauge_vec!(
        "flume_water_bridge_connected",
        "Flume bridge is connected to Flume",
        &["location"],
    )
    .unwrap();
    static ref SENSOR_PRODUCT: GaugeVec = register_gauge_vec!(
        "flume_water_sensor_product_info",
        "Flume sensor product",
        &["location", "product"],
    )
    .unwrap();
    static ref SENSOR_BATTERY: GaugeVec = register_gauge_vec!(
        "flume_water_sensor_battery_info",
        "Flume sensor battery level",
        &["location"],
    )
    .unwrap();
    static ref SENSOR_CONNECTED: GaugeVec = register_gauge_vec!(
        "flume_water_sensor_connected",
        "Flume sensor is connected to Flume",
        &["location"],
    )
    .unwrap();
    static ref USAGE: CounterVec = register_counter_vec!(
        "flume_water_usage_liters",
        "Water usage in liters",
        &["location"],
    )
    .unwrap();
}

pub struct Downloader {
    error_tx: Sender,
    refresh_interval: Duration,

    flume: Flume,

    user_id: Option<i64>,
    devices_last_update: Instant,
    sensors: Option<Vec<Sensor>>,
}

impl Downloader {
    pub fn new(flume: Flume, refresh_interval: Duration, error_tx: Sender) -> Self {
        let devices_last_update = Instant::now()
            .checked_sub(Duration::from_secs(86400))
            .unwrap();

        Downloader {
            error_tx,
            refresh_interval,

            flume,

            user_id: None,

            devices_last_update,
            sensors: None,
        }
    }

    pub async fn start(mut self) {
        tokio::spawn(async move {
            match self.flume.user_id().await {
                Ok(user_id) => {
                    self.user_id = Some(user_id);
                    debug!("user_id: {:?}", self.user_id)
                }
                Err(e) => self.send_error(e).await,
            }

            loop {
                match self.update().await {
                    Ok(_) => (),
                    Err(e) => self.send_error(e).await,
                };

                sleep(self.refresh_interval).await;
            }
        });
    }

    async fn send_error(&mut self, error: Error) {
        self.error_tx
            .send(error)
            .await
            .expect("Error propagation failed");
    }

    async fn update(&mut self) -> Result<()> {
        self.devices().await?;

        self.query().await?;

        Ok(())
    }

    async fn devices(&mut self) -> Result<bool> {
        let one_hour = Duration::from_secs(3600);

        if Instant::now().duration_since(self.devices_last_update) < one_hour {
            return Ok(false);
        }

        let mut sensors = Vec::new();

        let devices = self.flume.devices(self.user_id.unwrap()).await?;
        debug!("Found {} devices", devices.len());

        for device in devices {
            match device {
                Device::Bridge(b) => update_bridge(&b),
                Device::Sensor(s) => {
                    update_sensor(&s);

                    sensors.push(s);
                }
            };
        }

        self.sensors = Some(sensors);
        self.devices_last_update = Instant::now();

        Ok(true)
    }

    async fn query(&mut self) -> Result<()> {
        if let Some(sensors) = &self.sensors {
            let mut updated_sensors = Vec::with_capacity(sensors.len());

            let user_id = self.user_id.unwrap();

            for sensor in sensors {
                let new_usage = self.flume.query_sensor(user_id, &sensor).await?;

                debug!("Sensor {} used {} liters", sensor.id, new_usage);
                USAGE
                    .with_label_values(&[&sensor.location])
                    .inc_by(new_usage);

                updated_sensors.push(sensor.with_updated_timestamp());
            }
        }

        Ok(())
    }
}

fn update_bridge(bridge: &Bridge) {
    let location = &bridge.location;
    let product = &bridge.product;
    let connected = if bridge.connected { 1.0 } else { 0.0 };

    BRIDGE_PRODUCT
        .with_label_values(&[location, product])
        .set(1.0);
    BRIDGE_CONNECTED
        .with_label_values(&[location])
        .set(connected);
}

fn update_sensor(sensor: &Sensor) {
    let location = &sensor.location;
    let product = &sensor.product;

    let connected = if sensor.connected { 1.0 } else { 0.0 };
    let battery_level = if BATTERY_HIGH == sensor.battery_level {
        1.0
    } else if BATTERY_MEDIUM == sensor.battery_level {
        0.5
    } else if BATTERY_LOW == sensor.battery_level {
        0.25
    } else {
        0.0
    };

    SENSOR_PRODUCT
        .with_label_values(&[location, product])
        .set(1.0);
    SENSOR_BATTERY
        .with_label_values(&[location])
        .set(battery_level);
    SENSOR_CONNECTED
        .with_label_values(&[location])
        .set(connected);
}
