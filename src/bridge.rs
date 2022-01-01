use anyhow::anyhow;
use anyhow::Result;

use crate::client;

use std::convert::TryFrom;

pub struct Bridge {
    pub location: String,
    pub connected: bool,
    pub product: String,
}

impl TryFrom<client::Bridge> for Bridge {
    type Error = anyhow::Error;

    fn try_from(bridge: client::Bridge) -> Result<Self> {
        let location = bridge
            .location
            .ok_or_else(|| anyhow!("Fetch devices with location"))?;

        Ok(Bridge {
            location: location.name,
            connected: bridge.connected,
            product: bridge.product,
        })
    }
}
