use anyhow::Context;

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

use std::time::Duration;
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
    ) -> (String, String, Instant) {
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
            .unwrap();
        let data = &json["data"][0];

        let access_token = data["access_token"].as_str().unwrap().to_string();
        let refresh_token = data["refresh_token"].as_str().unwrap().to_string();
        let expires_in = data["expires_in"].as_u64().unwrap();
        let token_expires_at = Instant::now()
            .checked_add(Duration::from_secs(expires_in))
            .unwrap();

        (access_token, refresh_token, token_expires_at)
    }

    pub async fn devices(&mut self, access_token: &str, user_id: i64) -> Option<Vec<Device>> {
        let path = format!("/users/{}/devices?location=true", user_id);

        if let Some(json) = self.get(&path, Some(access_token), "devices").await {
            let devices = json["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(device)
                .collect();

            Some(devices)
        } else {
            None
        }
    }

    pub async fn query_samples(
        &self,
        access_token: &str,
        user_id: i64,
        sensor: &Sensor,
    ) -> Option<f64> {
        let since_time = sensor.last_update.format("%F %T").to_string();

        let body = json!({
            "queries": [{
                "request_id": since_time,
                "bucket": "MIN",
                "since_datetime": since_time,
                "operation": "SUM",
            }]
        });

        let path = format!("/users/{}/devices/{}/query", user_id, sensor.id);

        let json = self
            .post(&path, Some(access_token), body, "query")
            .await
            .unwrap();
        let query_result = &json["data"][0][since_time];

        if query_result.as_array().unwrap().is_empty() {
            None
        } else {
            Some(query_result[0]["value"].as_f64().unwrap())
        }
    }
    pub async fn refresh_token(&self, refresh_token: &str) -> (String, String, Instant) {
        let body = json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": self.client_id,
            "client_secret": self.client_secret,
        });

        let json = self
            .post("/oauth/token", None, body, "refresh token")
            .await
            .unwrap();

        let data = &json["data"][0];

        let access_token = data["access_token"].as_str().unwrap().to_string();
        let refresh_token = data["refresh_token"].as_str().unwrap().to_string();
        let expires_in = data["expires_in"].as_u64().unwrap();

        let token_expires_at = Instant::now()
            .checked_add(Duration::from_secs(expires_in))
            .unwrap();

        (access_token, refresh_token, token_expires_at)
    }

    pub async fn user_id(&self, access_token: &str) -> Option<i64> {
        if let Some(json) = self.get("/me", Some(access_token), "user id").await {
            json["data"][0]["id"].as_i64()
        } else {
            None
        }
    }

    async fn get(
        &self,
        path: &str,
        access_token: Option<&str>,
        request_name: &str,
    ) -> Option<serde_json::Value> {
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
    ) -> Option<serde_json::Value> {
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

async fn deserialize(body: &str, uri: &str, request_name: &str) -> Option<serde_json::Value> {
    match serde_json::from_str(body).with_context(|| format!("deserialize response from {}", uri)) {
        Ok(j) => Some(j),
        Err(e) => {
            debug!("JSON deserialize error {:?}", e);
            ERRORS
                .with_label_values(&[&request_name, "deserialize"])
                .inc();

            None
        }
    }
}

fn device(device: &serde_json::Value) -> Device {
    let device_type = device["type"].as_u64().unwrap();

    match device_type {
        BRIDGE_ID => bridge(device),
        SENSOR_ID => sensor(device),
        _ => unreachable!("unknown device type {}", device_type),
    }
}

fn bridge(device: &serde_json::Value) -> Device {
    let location = device["location"]["name"].as_str().unwrap().to_string();
    let connected = device["connected"].as_bool().unwrap();
    let product = device["product"].as_str().unwrap().to_string();

    Device::Bridge(Bridge {
        location,
        connected,
        product,
    })
}

fn sensor(device: &serde_json::Value) -> Device {
    let id = device["id"].as_str().unwrap().to_string();
    let connected = device["connected"].as_bool().unwrap();
    let product = device["product"].as_str().unwrap().to_string();
    let battery_level = device["battery_level"].as_str().unwrap().to_string();
    let location = device["location"]["name"].as_str().unwrap().to_string();

    let last_seen = device["last_seen"].as_str().unwrap().to_string();
    let timezone = device["location"]["tz"].as_str().unwrap().to_string();

    let timezone: Tz = timezone.parse().unwrap();
    let last_update = DateTime::parse_from_rfc3339(&last_seen)
        .unwrap()
        .with_timezone(&timezone);

    Device::Sensor(Sensor {
        id,
        location,
        product,
        connected,
        battery_level,
        last_update,
    })
}
async fn extract_body(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_method: &str,
    request_name: &str,
) -> Option<String> {
    let response = match response {
        Ok(r) => r,
        Err(e) => {
            debug!("{} error {:?}", request_method, e);
            ERRORS.with_label_values(&[&request_name, "request"]).inc();

            return None;
        }
    };

    match response
        .text()
        .await
        .with_context(|| format!("fetching response body for {}", uri))
    {
        Ok(b) => Some(b),
        Err(e) => {
            debug!("{} body fetch error {:?}", request_method, e);
            ERRORS.with_label_values(&[&request_name, "body"]).inc();

            None
        }
    }
}

async fn json_from(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_method: &str,
    request_name: &str,
) -> Option<serde_json::Value> {
    let body = extract_body(response, uri, request_method, request_name)
        .await
        .unwrap();

    deserialize(&body, uri, request_name).await
}
