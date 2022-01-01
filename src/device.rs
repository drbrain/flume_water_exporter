use anyhow::Result;

use crate::bridge::Bridge;
use crate::client;
use crate::sensor::Sensor;

use std::convert::TryFrom;

pub enum Device {
    Bridge(Bridge),
    Sensor(Sensor),
}

impl TryFrom<client::Device> for Device {
    type Error = anyhow::Error;

    fn try_from(device: client::Device) -> Result<Self> {
        Ok(match device {
            client::Device::Bridge(b) => Device::Bridge(b.try_into()?),
            client::Device::Sensor(s) => Device::Sensor(s.try_into()?),
        })
    }
}
