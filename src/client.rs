use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;

use crate::configuration::Configuration;

use lazy_static::lazy_static;

use log::debug;

use prometheus::register_histogram_vec;
use prometheus::register_int_counter_vec;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;

use serde::Deserialize;
use serde::Serialize;

use std::collections::HashMap;
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

#[derive(Clone, Deserialize, Serialize)]
pub struct Response {
    pub success: bool,
    pub code: u64,
    pub message: String,
    pub http_code: u64,
    pub http_message: String,
    pub detailed: serde_json::Value,
    pub data: Vec<Data>,
    pub count: u64,
    pub pagination: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccessToken {
    pub grant_type: String,
    pub client_id: String,
    pub client_secret: String,
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Data {
    Bridge(Bridge),
    Budget(Budget),
    Sensor(Sensor),
    Token(Token),
    User(User),
    QueryResults(HashMap<String, Vec<QueryResult>>),
}

#[derive(Clone, Debug)]
pub enum Device {
    Bridge(Bridge),
    Sensor(Sensor),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Bridge {
    pub id: String,
    pub last_seen: String,
    pub connected: bool,
    pub supports_ap: bool,
    pub product: String,
    pub user: Option<User>,
    pub location: Option<Location>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Budget {
    pub id: u64,
    pub name: String,
    #[serde(rename(deserialize = "type"))]
    pub period: BudgetPeriod,
    /// The budget value is always in gallons even if your preferred units are not gallons
    pub value: u64,
    /// The budget thresholds are always in gallons even if your preferred units are not gallons
    pub thresholds: Vec<u64>,
    /// The actual usage for the budget period always in gallons even if your preferred units are
    //not gallons
    pub actual: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BudgetPeriod {
    DAILY,
    WEEKLY,
    MONTHLY,
}

impl ToString for BudgetPeriod {
    fn to_string(&self) -> String {
        match self {
            BudgetPeriod::DAILY => "daily".to_string(),
            BudgetPeriod::WEEKLY => "weekly".to_string(),
            BudgetPeriod::MONTHLY => "monthly".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Location {
    pub id: u64,
    pub name: String,
    pub primary_location: bool,
    pub address: String,
    pub address_2: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
    pub tz: String,
    pub installation: String,
    pub away_mode: bool,
    pub usage_profile: UsageProfile,
    pub user: Option<User>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QueryResult {
    pub value: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Queries {
    queries: Vec<Query>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Query {
    pub request_id: String,
    pub bucket: QueryBucket,
    pub since_datetime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until_datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_multiplier: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<QueryOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<QuerySortDirection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<QueryUnits>,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            request_id: "".to_string(),
            bucket: QueryBucket::MIN,
            since_datetime: "".to_string(),
            until_datetime: None,
            group_multiplier: None,
            operation: None,
            sort_direction: None,
            units: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum QueryBucket {
    MIN = 3,
    HR = 4,
    DAY = 5,
    MON = 7,
    YR = 8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum QueryOperation {
    SUM = 1,
    AVG = 2,
    MIN = 3,
    MAX = 4,
    CNT = 5,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum QuerySortDirection {
    ASC,
    DESC,
}
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum QueryUnits {
    GALLONS,
    LITERS,
    CUBIC_FEET,
    CUBIC_METERS,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RefreshToken {
    pub grant_type: String,
    pub refresh_token: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Sensor {
    pub id: String,
    pub bridge_id: String,
    pub oriented: bool,
    pub last_seen: String,
    pub connected: bool,
    pub battery_level: String,
    pub product: String,
    pub user: Option<User>,
    pub location: Option<Location>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Token {
    pub token_type: String,
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UsageProfile {
    id: u64,
    score: u64,
    residents: String,
    bathrooms: String,
    irrigation: String,
    irrigation_freq: String,
    irrigation_max_cycle: u64,
    has_pool: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub id: i64,
    email_address: String,
    first_name: String,
    phone: String,
    status: String,
    #[serde(rename(deserialize = "type"))]
    user_type: String,
}

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,

    client_id: String,
    client_secret: String,
}

impl Client {
    pub fn new(configuration: &Configuration) -> Self {
        let timeout = configuration.flume_timeout();

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
    ) -> Result<(Token, Instant)> {
        let token_fetch_time = Instant::now();

        let request = AccessToken {
            grant_type: "password".to_string(),
            client_id: self.client_id.to_string(),
            client_secret: self.client_secret.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        };

        let body = serde_json::to_string(&request)?;

        let response = self
            .post("/oauth/token", None, body, "authenticate")
            .await
            .context("Authentication failed")?;

        let token = match &response.data[0] {
            Data::Token(t) => t.clone(),
            _ => {
                return Err(anyhow!(
                    "Unexpected response type while requesting access token"
                ));
            }
        };

        Ok((token, token_fetch_time))
    }

    pub async fn budgets(
        &self,
        access_token: &str,
        user_id: i64,
        sensor_id: &str,
    ) -> Result<Vec<Budget>> {
        let path = format!("/users/{}/devices/{}/budgets", user_id, sensor_id);

        let response = self.get(&path, Some(access_token), "budgets").await?;

        response.data.iter().map(budget).collect()
    }

    pub async fn devices(&mut self, access_token: &str, user_id: i64) -> Result<Vec<Device>> {
        let path = format!("/users/{}/devices?location=true", user_id);
        let response = self.get(&path, Some(access_token), "devices").await?;

        response.data.iter().map(device).collect()
    }

    pub async fn query_samples(
        &self,
        access_token: &str,
        user_id: i64,
        sensor_id: &str,
        query: Query,
    ) -> Result<f64> {
        let request_id = query.request_id.clone();

        let queries = Queries {
            queries: vec![query],
        };

        let body = serde_json::to_string(&queries)?;

        debug!("query: {}", body);

        let path = format!("/users/{}/devices/{}/query", user_id, sensor_id);

        let response = self.post(&path, Some(access_token), body, "query").await?;
        let query_results = &response.data[0];

        let query_result = match query_results {
            Data::QueryResults(q) => q,
            _ => {
                return Err(anyhow!("Unexpected response type querying sensor"));
            }
        };

        if let Some(results) = query_result.get(&request_id) {
            if let Some(result) = results.get(0) {
                Ok(result.value)
            } else {
                Ok(0.0)
            }
        } else {
            Err(anyhow!("Missing query result {}", request_id))
        }
    }

    pub async fn refresh_token(&self, refresh_token: &str) -> Result<(Token, Instant)> {
        let token_fetch_time = Instant::now();

        let refresh_token = RefreshToken {
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.to_string(),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
        };

        let body = serde_json::to_string(&refresh_token)?;

        let response = self
            .post("/oauth/token", None, body, "refresh token")
            .await?;

        let token = match &response.data[0] {
            Data::Token(t) => t.clone(),
            _ => {
                return Err(anyhow!("Unexpected response type while refreshing token"));
            }
        };

        Ok((token, token_fetch_time))
    }

    pub async fn user_id(&self, access_token: &str) -> Result<i64> {
        let response = self.get("/me", Some(access_token), "user id").await?;

        match &response.data[0] {
            Data::User(u) => Ok(u.id),
            _ => Err(anyhow!("Could not find user in response")),
        }
    }

    async fn get(
        &self,
        path: &str,
        access_token: Option<&str>,
        request_name: &str,
    ) -> Result<Response> {
        let uri = format!("{}{}", API_URI, path);

        debug!("GET {}", uri);
        REQUESTS.with_label_values(&[request_name]).inc();
        let timer = DURATIONS.with_label_values(&[request_name]).start_timer();

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
        body: String,
        request_name: &str,
    ) -> Result<Response> {
        let uri = format!("{}{}", API_URI, path);

        debug!("POST {}", uri);

        REQUESTS.with_label_values(&[request_name]).inc();
        let timer = DURATIONS.with_label_values(&[request_name]).start_timer();

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

fn deserialize(body: &str, uri: &str, request_name: &str) -> Result<Response> {
    let result =
        serde_json::from_str(body).with_context(|| format!("deserialize response from {}", uri));

    match result {
        Ok(json) => Ok(json),
        Err(e) => {
            debug!("JSON deserialize error {:?} for {}", e, body);
            ERRORS
                .with_label_values(&[request_name, "deserialize"])
                .inc();

            Err(e)
        }
    }
}

fn budget(data: &Data) -> Result<Budget> {
    match data {
        Data::Budget(b) => Ok(b.clone()),
        _ => Err(anyhow!("Unable to find budget in response")),
    }
}

fn device(data: &Data) -> Result<Device> {
    match data {
        Data::Bridge(b) => Ok(Device::Bridge(b.clone())),
        Data::Sensor(s) => Ok(Device::Sensor(s.clone())),
        _ => Err(anyhow!("Unable to find device in response")),
    }
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
            ERRORS.with_label_values(&[request_name, "request"]).inc();

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
            ERRORS.with_label_values(&[request_name, "body"]).inc();

            Err(e)
        }
    }
}

async fn json_from(
    response: Result<reqwest::Response, anyhow::Error>,
    uri: &str,
    request_method: &str,
    request_name: &str,
) -> Result<Response> {
    let body = extract_body(response, uri, request_method, request_name).await?;

    let result = deserialize(&body, uri, request_name)?;

    if !result.success {
        Err(anyhow!("request error {}", result.message))
    } else {
        Ok(result)
    }
}
