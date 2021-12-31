use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;

use chrono::DateTime;

use chrono_tz::Tz;

use crate::bridge::Bridge;
use crate::configuration::Configuration;
use crate::device::Device;
use crate::sensor::Sensor;

use lazy_static::lazy_static;

use log::debug;

use prometheus::register_histogram_vec;
use prometheus::register_int_counter_vec;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;

use serde_json::json;

use std::time::Instant;

lazy_static! {
    static ref REQUESTS: IntCounterVec = register_int_counter_vec!(
        "flume_water_http_requests_total",
        "Number of HTTP requests made to the Flume API",
        &["request_name"],
    )
    .unwrap();
    static ref ERRORS: IntCounterVec = register_int_counter_vec!(
        "flume_water_http_request_errors_total",
        "Number of HTTP request errors returned by the Flume API",
        &["request_name", "error_type"],
    )
    .unwrap();
    static ref DURATIONS: HistogramVec = register_histogram_vec!(
        "flume_water_http_request_duration_seconds",
        "Flume API request durations",
        &["request_name"],
    )
    .unwrap();
}

const API_URI: &str = "https://api.flumewater.com";
const BRIDGE_ID: u64 = 1;
const SENSOR_ID: u64 = 2;

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,

    client_id: String,
    client_secret: String,
}

impl Client {
    pub fn new(configuration: &Configuration) -> Self {
        let timeout = configuration.refresh_timeout();

        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            "Accept-Encoding",
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .default_headers(default_headers)
            .build()
            .expect("Could not build HTTP client");

        let client_id = configuration.client_id();
        let client_secret = configuration.secret_id();

        Client {
            client,

            client_id,
            client_secret,
        }
    }

    pub async fn access_token(
        &mut self,
        username: &str,
        password: &str,
    ) -> Result<(String, String, u64, Instant)> {
        let token_fetch_time = Instant::now();

        let body = json!({
            "grant_type": "password",
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "username": username,
            "password": password,
        });

        let json = self
            .post("/oauth/token", None, body, "authenticate")
            .await
            .context("Authentication failed")?;

        let data = &json["data"][0];

        access_token(data, token_fetch_time)
    }

    pub async fn devices(&mut self, access_token: &str, user_id: i64) -> Result<Vec<Device>> {
        let path = format!("/users/{}/devices?location=true", user_id);
        let json = self.get(&path, Some(access_token), "devices").await?;

        json["data"]
            .as_array()
            .context("No devices found")?
            .into_iter()
            .map(device)
            .collect()
    }

    pub async fn query_samples(
        &self,
        access_token: &str,
        user_id: i64,
        sensor: &Sensor,
    ) -> Result<f64> {
        let since_time = sensor.last_update.format("%F %T").to_string();
        debug!("Fetching usage for {} since {}", sensor.id, since_time);

        let body = json!({
            "queries": [{
                "request_id": since_time,
                "bucket": "MIN",
                "since_datetime": since_time,
                "operation": "SUM",
            }]
        });

        let path = format!("/users/{}/devices/{}/query", user_id, sensor.id);

        let json = self.post(&path, Some(access_token), body, "query").await?;
        let query_result = &json["data"][0][since_time];

        if query_result
            .as_array()
            .with_context(|| format!("Query results for {} are missing", sensor.id))?
            .is_empty()
        {
            debug!("Sensor {} did not report any data", sensor.id);

            // TODO this is probably a query error
            Ok(0.0)
        } else {
            let new_usage = query_result[0]["value"]
                .as_f64()
                .with_context(|| format!("missing query value for sensor {}", sensor.id))?;

            debug!(
                "Sensor {} reported usage of {} liters",
                sensor.id, new_usage
            );

            Ok(new_usage)
        }
    }

    pub async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<(String, String, u64, Instant)> {
        let token_fetch_time = Instant::now();

        let body = json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": self.client_id,
            "client_secret": self.client_secret,
        });

        let json = self
            .post("/oauth/token", None, body, "refresh token")
            .await?;

        let data = &json["data"][0];

        access_token(data, token_fetch_time)
    }

    pub async fn user_id(&self, access_token: &str) -> Result<i64> {
        let json = self.get("/me", Some(access_token), "user id").await?;

        let user_id = json["data"][0]["id"].as_i64().context("Missing user_id")?;

        Ok(user_id)
    }

    async fn get(
        &self,
        path: &str,
        access_token: Option<&str>,
        request_name: &str,
    ) -> Result<serde_json::Value> {
        let uri = format!("{}{}", API_URI, path);

        debug!("GET {}", uri);
        REQUESTS.with_label_values(&[&request_name]).inc();
        let timer = DURATIONS.with_label_values(&[&request_name]).start_timer();

        let builder = self.client.get(&uri).header("Accept", "application/json");

        let builder = if let Some(access_token) = access_token {
            builder.header("Authorization", format!("Bearer {}", access_token))
        } else {
            builder
        };

        let response = builder
            .send()
            .await
            .with_context(|| format!("awaiting response from {}", uri));

        timer.observe_duration();

        json_from(response, &uri, "GET", request_name).await
    }

    async fn post(
        &self,
        path: &str,
        access_token: Option<&str>,
        body: serde_json::Value,
        request_name: &str,
    ) -> Result<serde_json::Value> {
        let uri = format!("{}{}", API_URI, path);

        debug!("POST {}", uri);

        REQUESTS.with_label_values(&[&request_name]).inc();
        let timer = DURATIONS.with_label_values(&[&request_name]).start_timer();

        let builder = self
            .client
            .post(&uri)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(body.to_string());

        let builder = if let Some(access_token) = access_token {
            builder.header("Authorization", format!("Bearer {}", access_token))
        } else {
            builder
        };

        let response = builder
            .send()
            .await
            .with_context(|| format!("awaiting response from {}", uri));

        timer.observe_duration();

        json_from(response, &uri, "POST", request_name).await
    }
}

async fn deserialize(body: &str, uri: &str, request_name: &str) -> Result<serde_json::Value> {
    let result =
        serde_json::from_str(body).with_context(|| format!("deserialize response from {}", uri));

    match result {
        Ok(json) => Ok(json),
        Err(e) => {
            debug!("JSON deserialize error {:?}", e);
            ERRORS
                .with_label_values(&[&request_name, "deserialize"])
                .inc();

            Err(e)
        }
    }
}

fn device(device: &serde_json::Value) -> Result<Device> {
    let device_type = device["type"].as_u64().context("Missing device type")?;

    match device_type {
        BRIDGE_ID => bridge(device),
        SENSOR_ID => sensor(device),
        _ => unreachable!("unknown device type {}", device_type),
    }
}

fn bridge(device: &serde_json::Value) -> Result<Device> {
    let location = device["location"]["name"]
        .as_str()
        .context("Missing bridge location name")?
        .to_string();
    let connected = device["connected"]
        .as_bool()
        .context("Missing bridge connected")?;
    let product = device["product"]
        .as_str()
        .context("Missing bridge product")?
        .to_string();

    Ok(Device::Bridge(Bridge {
        location,
        connected,
        product,
    }))
}

fn sensor(device: &serde_json::Value) -> Result<Device> {
    let id = device["id"]
        .as_str()
        .context("Missing sensor id")?
        .to_string();
    let connected = device["connected"]
        .as_bool()
        .context("Missing sensor connected")?;
    let product = device["product"]
        .as_str()
        .context("Missing sensor product")?
        .to_string();
    let battery_level = device["battery_level"]
        .as_str()
        .context("Missing sensor battery_level")?
        .to_string();
    let location = device["location"]["name"]
        .as_str()
        .context("Missing sensor location name")?
        .to_string();

    let last_seen = device["last_seen"]
        .as_str()
        .context("Missing sensor last_seen")?
        .to_string();
    let timezone = device["location"]["tz"]
        .as_str()
        .context("Missing sensor location tz")?
        .to_string();

    let timezone: Tz = match timezone.parse() {
        Ok(tz) => tz,
        Err(_) => {
            return Err(anyhow!("Unknown sensor timezone {}", timezone));
        }
    };

    let last_update = DateTime::parse_from_rfc3339(&last_seen)
        .with_context(|| format!("Unable to parse last seen time {}", last_seen))?
        .with_timezone(&timezone);

    Ok(Device::Sensor(Sensor {
        id,
        location,
        product,
        connected,
        battery_level,
        last_update,
    }))
}
async fn extract_body(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_method: &str,
    request_name: &str,
) -> Result<String> {
    let response = match response {
        Ok(r) => r,
        Err(e) => {
            debug!("{} error {:?}", request_method, e);
            ERRORS.with_label_values(&[&request_name, "request"]).inc();

            return Err(e);
        }
    };

    let result = response
        .text()
        .await
        .with_context(|| format!("fetching response body for {}", uri));

    match result {
        Ok(text) => Ok(text),
        Err(e) => {
            debug!("{} body fetch error {:?}", request_method, e);
            ERRORS.with_label_values(&[&request_name, "body"]).inc();

            Err(e)
        }
    }
}

fn access_token(
    json: &serde_json::Value,
    fetch_time: Instant,
) -> Result<(String, String, u64, Instant)> {
    let access_token = json["access_token"]
        .as_str()
        .context("missing access_token")?
        .to_string();
    let refresh_token = json["refresh_token"]
        .as_str()
        .context("missing refresh_token")?
        .to_string();
    let expires_in = json["expires_in"].as_u64().context("missing expires_in")?;

    Ok((access_token, refresh_token, expires_in, fetch_time))
}

async fn json_from(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_method: &str,
    request_name: &str,
) -> Result<serde_json::Value> {
    let body = extract_body(response, uri, request_method, request_name).await?;

    deserialize(&body, uri, request_name).await
}
