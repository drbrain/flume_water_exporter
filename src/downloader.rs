use anyhow::Context;

use crate::configuration::Configuration;

use lazy_static::lazy_static;

use log::debug;

use prometheus::register_gauge_vec;
use prometheus::register_histogram_vec;
use prometheus::register_int_counter_vec;
use prometheus::GaugeVec;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;

use reqwest::Client;

use serde_json::json;

use std::time::Duration;
use std::time::Instant;

use tokio::sync::mpsc;
use tokio::time::sleep;

type Sender = mpsc::Sender<anyhow::Error>;
const API_URI: &str = "https://api.flumewater.com";
const BRIDGE_ID: u64 = 1;
const SENSOR_ID: u64 = 2;

lazy_static! {
    static ref REQUESTS: IntCounterVec = register_int_counter_vec!(
        "flume_water_http_requests_total",
        "Number of HTTP requests made to the Flume API",
        &["uri"],
    )
    .unwrap();
    static ref ERRORS: IntCounterVec = register_int_counter_vec!(
        "flume_water_http_request_errors_total",
        "Number of HTTP request errors returned by the Flume API",
        &["uri", "error_type"],
    )
    .unwrap();
    static ref DURATIONS: HistogramVec = register_histogram_vec!(
        "flume_water_http_request_duration_seconds",
        "Flume API request durations",
        &["uri"],
    )
    .unwrap();
    static ref BRIDGE_PRODUCT: GaugeVec = register_gauge_vec!(
        "flume_water_bridge_product_info",
        "Flume bridge product",
        &["name", "product"],
    )
    .unwrap();
    static ref BRIDGE_CONNECTED: GaugeVec = register_gauge_vec!(
        "flume_water_bridge_connected",
        "Flume bridge is connected to Flume",
        &["name"],
    )
    .unwrap();
    static ref SENSOR_PRODUCT: GaugeVec = register_gauge_vec!(
        "flume_water_sensor_product_info",
        "Flume sensor product",
        &["name", "product"],
    )
    .unwrap();
    static ref SENSOR_BATTERY: GaugeVec = register_gauge_vec!(
        "flume_water_sensor_battery_info",
        "Flume sensor battery level",
        &["name"],
    )
    .unwrap();
    static ref SENSOR_CONNECTED: GaugeVec = register_gauge_vec!(
        "flume_water_sensor_connected",
        "Flume sensor is connected to Flume",
        &["name"],
    )
    .unwrap();
}

pub struct Downloader {
    error_tx: Sender,
    client: Client,
    interval: Duration,

    client_id: String,
    secret_id: String,
    username: String,
    password: String,

    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<Instant>,

    user_id: Option<i64>,
    devices_last_update: Instant,
}

impl Downloader {
    pub fn new(configuration: &Configuration, error_tx: Sender) -> Self {
        let timeout = configuration.refresh_timeout();
        let interval = configuration.refresh_interval();

        let client_id = configuration.client_id();
        let secret_id = configuration.secret_id();
        let username = configuration.username();
        let password = configuration.password();

        let devices_last_update = Instant::now()
            .checked_sub(Duration::from_secs(86400))
            .unwrap();

        let client = Client::builder()
            .connect_timeout(timeout)
            .timeout(timeout)
            .build()
            .expect("Could not build HTTP client");

        Downloader {
            error_tx,
            client,
            interval,

            client_id,
            secret_id,
            username,
            password,

            access_token: None,
            refresh_token: None,
            expires_at: None,

            user_id: None,
            devices_last_update,
        }
    }

    pub async fn start(mut self) {
        tokio::spawn(async move {
            loop {
                self.fetch().await;

                sleep(self.interval).await;
            }
        });
    }

    pub async fn fetch(&mut self) {
        self.login().await;

        self.devices().await;
    }

    pub async fn login(&mut self) {
        if self.access_token.is_none() {
            let body = json!({
                "grant_type": "password",
                "client_id": self.client_id,
                "client_secret": self.secret_id,
                "username": self.username,
                "password": self.password,
            });

            let json = self.post("/oauth/token", body, true).await.unwrap();
            let data = &json["data"][0];

            self.access_token = Some(data["access_token"].as_str().unwrap().to_string());
            self.refresh_token = Some(data["refresh_token"].as_str().unwrap().to_string());
            self.expires_at = Some(
                Instant::now()
                    .checked_add(Duration::from_secs(data["expires_in"].as_u64().unwrap()))
                    .unwrap(),
            );
        }

        if Instant::now() >= self.expires_at.unwrap() {
            let body = json!( {
                "grant_type": "refresh_token",
                "refresh_token": self.refresh_token.clone().unwrap(),
                "client_id": self.client_id,
                "secret_id": self.secret_id,
            });

            let json = self.post("/oauth/token", body, true).await.unwrap();
            let data = &json["data"][0];

            self.access_token = Some(data["access_token"].as_str().unwrap().to_string());
            self.refresh_token = Some(data["refresh_token"].as_str().unwrap().to_string());
            self.expires_at = Some(
                Instant::now()
                    .checked_add(Duration::from_secs(data["expires_in"].as_u64().unwrap()))
                    .unwrap(),
            );
        }

        if self.user_id.is_none() {
            let json = self.get("/me", true).await.unwrap();

            self.user_id = json["data"][0]["id"].as_i64();
        }
    }

    pub async fn devices(&mut self) {
        let one_hour_ago = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap();

        if one_hour_ago >= self.devices_last_update {
            let path = format!("/users/{}/devices?location=true", self.user_id.unwrap());

            let json = self.get(&path, false).await.unwrap();

            json["data"].as_array().unwrap().iter().for_each(device);

            self.devices_last_update = Instant::now();
        }
    }

    pub async fn post(
        &self,
        path: &str,
        body: serde_json::Value,
        send_error: bool,
    ) -> Option<serde_json::Value> {
        let uri = format!("{}{}", API_URI, path);

        debug!("POST {}", uri);

        REQUESTS.with_label_values(&[&uri]).inc();
        let timer = DURATIONS.with_label_values(&[&uri]).start_timer();

        let builder = self
            .client
            .post(&uri)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .body(body.to_string());

        let builder = if let Some(access_token) = &self.access_token {
            builder.header("Authorization", format!("Bearer {}", access_token))
        } else {
            builder
        };

        let response = builder
            .send()
            .await
            .with_context(|| format!("reqwest GET error for {}", uri));

        timer.observe_duration();

        let error_tx = if send_error {
            Some(&self.error_tx)
        } else {
            None
        };

        json_from(response, &uri, "POST", error_tx).await
    }

    pub async fn get(&self, path: &str, send_error: bool) -> Option<serde_json::Value> {
        let uri = format!("{}{}", API_URI, path);

        debug!("GET {}", uri);
        REQUESTS.with_label_values(&[&uri]).inc();
        let timer = DURATIONS.with_label_values(&[&uri]).start_timer();

        let builder = self.client.get(&uri).header("Accept", "application/json");

        let builder = if let Some(access_token) = &self.access_token {
            builder.header("Authorization", format!("Bearer {}", access_token))
        } else {
            builder
        };

        let response = builder
            .send()
            .await
            .with_context(|| format!("reqwest GET error for {}", uri));

        timer.observe_duration();

        let error_tx = if send_error {
            Some(&self.error_tx)
        } else {
            None
        };

        json_from(response, &uri, "GET", error_tx).await
    }
}

fn device(device: &serde_json::Value) {
    let device_type = device["type"].as_u64().unwrap();

    match device_type {
        BRIDGE_ID => update_bridge(device),
        SENSOR_ID => update_sensor(device),
        _ => unreachable!("unknown device type {}", device_type),
    };
}

fn update_bridge(device: &serde_json::Value) {
    let name = device["location"]["name"].as_str().unwrap().to_string();
    let connected = if device["connected"].as_bool().unwrap() {
        1.0
    } else {
        0.0
    };
    let product = device["product"].as_str().unwrap().to_string();

    BRIDGE_PRODUCT
        .with_label_values(&[&name, &product])
        .set(1.0);
    BRIDGE_CONNECTED.with_label_values(&[&name]).set(connected);
}

fn update_sensor(device: &serde_json::Value) {
    let name = device["location"]["name"].as_str().unwrap().to_string();
    let connected = if device["connected"].as_bool().unwrap() {
        1.0
    } else {
        0.0
    };
    let product = device["product"].as_str().unwrap().to_string();
    let battery_level = device["battery_level"].as_str().unwrap();
    let battery_level = match battery_level {
        "high" => 1.0,
        "medium" => 0.5,
        "low" => 0.25,
        _ => unreachable!("Unknown battery level {:?}", battery_level),
    };

    SENSOR_PRODUCT
        .with_label_values(&[&name, &product])
        .set(1.0);
    SENSOR_BATTERY
        .with_label_values(&[&name])
        .set(battery_level);
    SENSOR_CONNECTED.with_label_values(&[&name]).set(connected);
}

async fn deserialize(
    body: &str,
    uri: &str,
    error_tx: Option<&Sender>,
) -> Option<serde_json::Value> {
    match serde_json::from_str(body).with_context(|| format!("deserialize response from {}", uri)) {
        Ok(j) => Some(j),
        Err(e) => {
            debug!("JSON deserialize error {:?}", e);
            ERRORS.with_label_values(&[&uri, "deserialize"]).inc();

            if let Some(error_tx) = error_tx {
                error_tx
                    .send(e)
                    .await
                    .expect("Error channel failed unexpectedly, bug?");
            };

            None
        }
    }
}

async fn extract_body(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_type: &str,
    error_tx: Option<&Sender>,
) -> Option<String> {
    let response = match response {
        Ok(r) => r,
        Err(e) => {
            debug!("{} error {:?}", request_type, e);
            ERRORS.with_label_values(&[&uri, "request"]).inc();

            if let Some(error_tx) = error_tx {
                error_tx
                    .send(e)
                    .await
                    .expect("Error channel failed unexpectedly, bug?");
            };

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
            debug!("{} body fetch error {:?}", request_type, e);
            ERRORS.with_label_values(&[&uri, "body"]).inc();

            if let Some(error_tx) = error_tx {
                error_tx
                    .send(e)
                    .await
                    .expect("Error channel failed unexpectedly, bug?");
            };

            None
        }
    }
}

async fn json_from(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_type: &str,
    error_tx: Option<&Sender>,
) -> Option<serde_json::Value> {
    let body = extract_body(response, uri, request_type, error_tx)
        .await
        .unwrap();

    deserialize(&body, uri, error_tx).await
}
