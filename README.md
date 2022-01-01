Prometheus exporter for the [Flume Smart Home Water Monitor](https://flumewater.com)

## Configuration

For the minimum configuration you will need to provide a Flume Water API key,
secret, username and password.  You can create an API key from your [settings
page](https://portal.flumewater.com/settings).  Your username and password are
from your Flume portal login.

```toml
client_id = "CLIENT_ID_HERE"
secret_id = "CLIENT_SECRET_HERE"
username = "YOUR.EMAIL@EXAMPLE"
password = "YOUR_PASSWORD"
```

You may also configure the prometheus metrics server bind address, the usage
query interval, the device update interval, and the timeout for flume API
requests.  Here are the default values:

```toml
bind_address = "0.0.0.0:9160"
query_interval = 60 # seconds
device_interval = 300 # secodns
flume_timeout = 1000 # milliseconds
```

On each query interval the exporter fetches usage for each sensor in an
account.  On each device interval the exporter fetches bridge and sensor status
for all devices on the account.  If you have two flume sensors and two flume
bridges you will make two queries per query interval and one query per device
interval.

The Flume API has a rate limit of [120 requests per
hour](https://flumetech.readme.io/docs/rate-limiting).

## Metrics

The following metrics contain a `location` label:

`flume_water_bridge_connected` is 1 when the bridge is connected to the internet.

`flume_water_bridge_product_info` contains the bridge product name in the `product` label.

`flume_water_sensor_battery_info` contains the battery level.  1 is "high", 0.5
is "medium", 0.25 is "low".  Flume provides no estimate of how long the
batteries will last at any level.

`flume_water_sensor_connected` is 1 when the sensor is connected to the bridge.

`flume_water_sensor_product_info` contains the bridge product name in the
'product' label.

`flume_water_usage_liters` is a counter for the number of liters the meter has
seen.

The following metrics contain a `request_name` label:

`flume_water_http_request_duration_seconds` is a histogram of response times
for the Flume API by request name.

`flume_water_http_requests_total` contains the total number of Flume API
requests sent.

`flume_water_http_request_errors_total` contains the total number of Flume API
request errors received.
