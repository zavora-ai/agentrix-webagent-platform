//! A small PayPal REST client: OAuth client-credentials with a cached token, JSON requests with
//! `PayPal-Request-Id` idempotency keys, and readable errors. Adapted from the fleet release's
//! `steward-paypal` (same authors, MIT).
//!
//! Sandbox only in Horizon 1: the live API needs `"environment": "live"` **and**
//! `"allow_live": true` in the secret, so a demo can never touch real money by accident.

use std::path::Path;
use std::time::{Duration, Instant};

use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// Sandbox REST endpoint.
pub const SANDBOX_URL: &str = "https://api-m.sandbox.paypal.com";
/// Live REST endpoint.
pub const LIVE_URL: &str = "https://api-m.paypal.com";

/// Errors from PayPal or from talking to it.
#[derive(Debug, thiserror::Error)]
pub enum PayPalError {
    /// Missing or unsafe configuration.
    #[error("PayPal configuration: {0}")]
    Config(String),
    /// Network or TLS failure: the outcome is unknown.
    #[error("PayPal request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The OAuth token request was refused.
    #[error("PayPal authentication failed ({status}): {message}")]
    Auth {
        /// HTTP status.
        status: u16,
        /// PayPal's words.
        message: String,
    },
    /// The API answered with an error document.
    #[error("PayPal {status} {name}: {message}{}", details_suffix(.issues))]
    Api {
        /// HTTP status.
        status: u16,
        /// For example `UNPROCESSABLE_ENTITY`.
        name: String,
        /// PayPal's message.
        message: String,
        /// Issue codes, for example `ORDER_NOT_APPROVED`.
        issues: Vec<String>,
        /// For PayPal support.
        debug_id: Option<String>,
    },
}

impl PayPalError {
    /// Whether PayPal said this issue code.
    pub fn has_issue(&self, issue: &str) -> bool {
        matches!(self, Self::Api { issues, .. } if issues.iter().any(|i| i == issue))
    }

    /// Whether the request may have run (network failure or a 5xx): then the effect's outcome
    /// is unknown and is reconciled by reading the resource back, never by retrying blind.
    pub fn outcome_unknown(&self) -> bool {
        match self {
            Self::Http(_) => true,
            Self::Api { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

fn details_suffix(issues: &[String]) -> String {
    if issues.is_empty() {
        String::new()
    } else {
        format!(" [{}]", issues.join(", "))
    }
}

/// Which PayPal to talk to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    /// `api-m.sandbox.paypal.com`.
    Sandbox,
    /// `api-m.paypal.com` (real money).
    Live,
    /// Another base URL (tests).
    Custom(String),
}

impl Environment {
    fn base_url(&self) -> &str {
        match self {
            Self::Sandbox => SANDBOX_URL,
            Self::Live => LIVE_URL,
            Self::Custom(url) => url.trim_end_matches('/'),
        }
    }

    /// True for real money.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live)
    }
}

/// Credentials. `Debug` never shows the secret.
#[derive(Clone)]
pub struct PayPalConfig {
    /// REST app client id.
    pub client_id: String,
    /// REST app secret.
    pub client_secret: String,
    /// The webhook's id, for verifying PayPal's signatures (optional until webhooks are set up).
    pub webhook_id: Option<String>,
    /// Which PayPal.
    pub environment: Environment,
}

impl std::fmt::Debug for PayPalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayPalConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<withheld>")
            .field("webhook_id", &self.webhook_id)
            .field("environment", &self.environment)
            .finish()
    }
}

#[derive(Deserialize)]
struct SecretFile {
    client_id: String,
    client_secret: String,
    #[serde(default)]
    webhook_id: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    allow_live: bool,
}

impl PayPalConfig {
    /// Reads the secret file `{client_id, client_secret, webhook_id?, environment?}`. An empty
    /// or missing file means payments are off (`Ok(None)`), as with the first release's objects.json.
    pub fn from_file(path: &Path) -> Result<Option<Self>, PayPalError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(PayPalError::Config(format!("{}: {e}", path.display()))),
        };
        if text.trim().is_empty() {
            return Ok(None);
        }
        let file: SecretFile = serde_json::from_str(&text).map_err(|e| {
            PayPalError::Config(format!("{} is not the expected JSON: {e}", path.display()))
        })?;
        let environment = match file.environment.as_deref().unwrap_or("sandbox") {
            "sandbox" => Environment::Sandbox,
            "live" if file.allow_live => Environment::Live,
            "live" => {
                return Err(PayPalError::Config(
                    "live moves real money; set allow_live too".into(),
                ));
            }
            other => {
                return Err(PayPalError::Config(format!(
                    "unknown environment '{other}'"
                )));
            }
        };
        if file.client_id.trim().is_empty() || file.client_secret.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            client_id: file.client_id,
            client_secret: file.client_secret,
            webhook_id: file.webhook_id,
            environment,
        }))
    }
}

struct Token {
    value: String,
    refresh_at: Instant,
}

/// The PayPal REST client. Share it behind an `Arc`.
pub struct PayPalClient {
    http: reqwest::Client,
    config: PayPalConfig,
    token: Mutex<Option<Token>>,
}

impl std::fmt::Debug for PayPalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayPalClient")
            .field("config", &self.config)
            .finish()
    }
}

impl PayPalClient {
    /// A client over `http`.
    pub fn new(config: PayPalConfig, http: reqwest::Client) -> Self {
        Self {
            http,
            config,
            token: Mutex::new(None),
        }
    }

    /// The environment.
    pub fn environment(&self) -> &Environment {
        &self.config.environment
    }

    /// The webhook id, if set.
    pub fn webhook_id(&self) -> Option<&str> {
        self.config.webhook_id.as_deref()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.environment.base_url(), path)
    }

    async fn access_token(&self, refresh: bool) -> Result<String, PayPalError> {
        let mut cached = self.token.lock().await;
        if !refresh {
            if let Some(token) = cached.as_ref() {
                if Instant::now() < token.refresh_at {
                    return Ok(token.value.clone());
                }
            }
        }
        let response = self
            .http
            .post(self.url("/v1/oauth2/token"))
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let message = body["error_description"]
                .as_str()
                .or(body["error"].as_str())
                .unwrap_or("no detail");
            return Err(PayPalError::Auth {
                status: status.as_u16(),
                message: message.to_owned(),
            });
        }
        let value = body["access_token"]
            .as_str()
            .ok_or_else(|| PayPalError::Auth {
                status: status.as_u16(),
                message: "no access_token".into(),
            })?
            .to_owned();
        let lifetime = body["expires_in"].as_u64().unwrap_or(300);
        let refresh_at = Instant::now() + Duration::from_secs(lifetime.saturating_sub(60).max(1));
        *cached = Some(Token {
            value: value.clone(),
            refresh_at,
        });
        Ok(value)
    }

    /// Sends a JSON request; `idempotency_key` becomes `PayPal-Request-Id`, so PayPal runs a
    /// repeated request once and answers with the first result.
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, PayPalError> {
        for attempt in 0..2 {
            let token = self.access_token(attempt > 0).await?;
            let mut builder = self
                .http
                .request(method.clone(), self.url(path))
                .bearer_auth(token)
                .header("Prefer", "return=representation")
                .timeout(Duration::from_secs(30));
            if let Some(key) = idempotency_key {
                builder = builder.header("PayPal-Request-Id", key);
            }
            builder = match body {
                Some(body) => builder.json(body),
                None if method == Method::POST => builder.json(&json!({})),
                None => builder,
            };
            let response = builder.send().await?;
            let status = response.status();
            if status == StatusCode::UNAUTHORIZED && attempt == 0 {
                continue;
            }
            let text = response.text().await?;
            let parsed: Value = if text.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text).unwrap_or(Value::String(text))
            };
            if status.is_success() {
                return Ok(parsed);
            }
            return Err(api_error(status, &parsed));
        }
        Err(PayPalError::Auth {
            status: 401,
            message: "token refused twice".into(),
        })
    }
}

fn api_error(status: StatusCode, body: &Value) -> PayPalError {
    let issues = body["details"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|d| d["issue"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    PayPalError::Api {
        status: status.as_u16(),
        name: body["name"].as_str().unwrap_or("ERROR").to_owned(),
        message: body["message"]
            .as_str()
            .or(body.as_str())
            .unwrap_or("no message")
            .to_owned(),
        issues,
        debug_id: body["debug_id"].as_str().map(str::to_owned),
    }
}

/// Currencies PayPal takes in whole units only.
const ZERO_DECIMAL: [&str; 3] = ["HUF", "JPY", "TWD"];

/// An amount in minor units as PayPal's `{currency_code, value}`.
pub fn amount(currency: &str, minor: i64) -> Value {
    let currency = currency.to_ascii_uppercase();
    let value = if ZERO_DECIMAL.contains(&currency.as_str()) {
        minor.to_string()
    } else {
        format!("{}.{:02}", minor / 100, (minor % 100).abs())
    };
    json!({ "currency_code": currency, "value": value })
}

/// PayPal's `{currency_code, value}` back to minor units.
pub fn minor_of(amount: &Value) -> Option<(String, i64)> {
    let currency = amount["currency_code"].as_str()?.to_owned();
    let value = amount["value"].as_str()?;
    let minor = if ZERO_DECIMAL.contains(&currency.as_str()) {
        value.parse().ok()?
    } else {
        let (whole, cents) = value.split_once('.').unwrap_or((value, "0"));
        let cents = format!("{cents:0<2}");
        whole.parse::<i64>().ok()? * 100 + cents.get(..2)?.parse::<i64>().ok()?
    };
    Some((currency, minor))
}

/// The first link with one of `rels` (`approve`, `payer-action`).
pub fn link(body: &Value, rels: &[&str]) -> Option<String> {
    body["links"]
        .as_array()?
        .iter()
        .find(|l| rels.contains(&l["rel"].as_str().unwrap_or("")))
        .and_then(|l| l["href"].as_str().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_round_trips() {
        assert_eq!(
            amount("usd", 1850),
            json!({"currency_code": "USD", "value": "18.50"})
        );
        assert_eq!(
            amount("USD", 5),
            json!({"currency_code": "USD", "value": "0.05"})
        );
        assert_eq!(
            amount("JPY", 1500),
            json!({"currency_code": "JPY", "value": "1500"})
        );
        assert_eq!(
            minor_of(&json!({"currency_code": "USD", "value": "18.5"})),
            Some(("USD".into(), 1850))
        );
        assert_eq!(
            minor_of(&json!({"currency_code": "USD", "value": "100"})),
            Some(("USD".into(), 10000))
        );
    }

    #[test]
    fn errors_read_well_and_say_if_unknown() {
        let body = json!({"name": "UNPROCESSABLE_ENTITY", "message": "The requested action could not be performed.", "details": [{"issue": "ORDER_NOT_APPROVED"}]});
        let e = api_error(StatusCode::UNPROCESSABLE_ENTITY, &body);
        assert!(e.has_issue("ORDER_NOT_APPROVED"));
        assert!(!e.outcome_unknown());
        assert!(api_error(StatusCode::BAD_GATEWAY, &json!({})).outcome_unknown());
    }

    #[test]
    fn secret_file_rules() {
        let dir = std::env::temp_dir().join(format!("w6-paypal-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("paypal.json");
        assert!(
            PayPalConfig::from_file(&path).unwrap().is_none(),
            "missing: off"
        );
        std::fs::write(&path, "").unwrap();
        assert!(
            PayPalConfig::from_file(&path).unwrap().is_none(),
            "empty: off"
        );
        std::fs::write(&path, r#"{"client_id":"id","client_secret":"s3cret"}"#).unwrap();
        let config = PayPalConfig::from_file(&path).unwrap().unwrap();
        assert_eq!(config.environment, Environment::Sandbox);
        assert!(!format!("{config:?}").contains("s3cret"));
        std::fs::write(
            &path,
            r#"{"client_id":"id","client_secret":"s","environment":"live"}"#,
        )
        .unwrap();
        assert!(
            PayPalConfig::from_file(&path).is_err(),
            "live needs allow_live"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
