use crate::bridge::Bridge;
use crate::sensor::Sensor;

pub enum Device {
    Bridge(Bridge),
    Sensor(Sensor),
}
