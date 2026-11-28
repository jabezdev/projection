use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct Convex {
    http: reqwest::Client,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply {
    status: String,
    #[serde(default)]
    value: Value,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    error_data: Option<Value>,
}

#[derive(Debug)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

impl Convex {
    pub fn new(url: &str) -> Result<Convex> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(60)).build()?;
        Ok(Convex { http, url: normalize(url)? })
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub async fn query<T: DeserializeOwned>(&self, path: &str, args: Value) -> Result<T> {
        self.call("query", path, args).await
    }

    pub async fn mutation<T: DeserializeOwned>(&self, path: &str, args: Value) -> Result<T> {
        self.call("mutation", path, args).await
    }

    async fn call<T: DeserializeOwned>(&self, kind: &str, path: &str, args: Value) -> Result<T> {
        let response = self
            .http
            .post(format!("{}/api/{kind}", self.url))
            .json(&json!({ "path": path, "args": args, "format": "json" }))
            .send()
            .await
            .with_context(|| format!("reach {}", self.url))?;
        let status = response.status();
        let reply: Reply = response.json().await.with_context(|| format!("read the reply to {path} ({status})"))?;
        if reply.status == "success" {
            return serde_json::from_value(whole_numbers(reply.value)).with_context(|| format!("decode the reply to {path}"));
        }
        let message = reply.error_data.and_then(|d| d.as_str().map(str::to_string)).or(reply.error_message).unwrap_or_else(|| format!("{path} failed"));
        if status.is_client_error() || status.is_success() {
            return Err(Refused(message).into());
        }
        Err(anyhow!(message))
    }
}

pub fn normalize(url: &str) -> Result<String> {
    let raw = url.trim();
    let url = if raw.contains("://") { raw.to_string() } else { format!("https://{raw}") };
    let parsed = reqwest::Url::parse(&url).map_err(|_| Refused("That team address doesn't look right".into()))?;
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(Refused("That team address doesn't look right".into()).into());
    }
    Ok(url.trim_end_matches('/').to_string())
}

fn whole_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 9.0e15 => Value::from(f as i64),
            _ => Value::Number(n),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(whole_numbers).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, whole_numbers(v))).collect()),
        other => other,
    }
}

pub fn is_refusal(error: &anyhow::Error) -> bool {
    error.downcast_ref::<Refused>().is_some()
}

#[cfg(test)]
mod tests {
    #[test]
    fn team_addresses_get_a_scheme_and_lose_trailing_slashes() {
        assert_eq!(super::normalize("happy-cat-123.convex.cloud/").unwrap(), "https://happy-cat-123.convex.cloud");
        assert_eq!(super::normalize(" http://127.0.0.1:3210 ").unwrap(), "http://127.0.0.1:3210");
        assert!(super::normalize("https://").is_err());
    }

    #[test]
    fn whole_floats_from_convex_become_integers() {
        let value = super::whole_numbers(serde_json::json!({ "number": 1.0, "volume": [0.5, 100.0] }));
        assert_eq!(value, serde_json::json!({ "number": 1, "volume": [0.5, 100] }));
    }
}
