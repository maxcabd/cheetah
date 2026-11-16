//! HTTP over the native hosts (replaces axios / fetch).
//!
//! Requests are queued as `http.request` native calls. The host performs them
//! with `URLSession` (iOS) or `HttpURLConnection` (Android), so TLS, proxies,
//! and certificate trust follow the platform. The response arrives as a
//! `native` event:
//!
//! ```json
//! {"module":"http","method":"response","id":7,"ok":true,"status":200,
//!  "headers":{"content-type":"application/json"},"body":"{...}"}
//! ```
//!
//! `ok` means the transport succeeded (a response was received). Check
//! [`HttpResponse::is_success`] for a 2xx status. Bodies are UTF-8 text.

use crate::native_api::{NativeCall, NativeEvent};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MODULE: &str = "http";
pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRequest {
    pub id: u64,
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub timeout_ms: u32,
}

impl HttpRequest {
    pub fn new(id: u64, method: &str, url: impl Into<String>) -> Self {
        Self {
            id,
            method: method.to_ascii_uppercase(),
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.insert(name.to_ascii_lowercase(), value.into());
        self
    }

    pub fn bearer(self, token: &str) -> Self {
        self.header("authorization", format!("Bearer {token}"))
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Serialize `value` as the body and set `content-type: application/json`.
    pub fn json<T: Serialize>(self, value: &T) -> Self {
        let body = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
        self.header("content-type", "application/json").body(body)
    }

    /// `application/x-www-form-urlencoded` body (OAuth / Supabase token endpoints).
    pub fn form(self, fields: &[(&str, &str)]) -> Self {
        let body = fields
            .iter()
            .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
            .collect::<Vec<_>>()
            .join("&");
        self.header("content-type", "application/x-www-form-urlencoded")
            .body(body)
    }

    pub fn timeout_ms(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn into_call(self) -> NativeCall {
        NativeCall::json(MODULE, "request", self)
    }
}

/// Cancel an in-flight request. The host answers with `ok: false,
/// error: "cancelled"` if it was still running.
pub fn cancel(id: u64) -> NativeCall {
    NativeCall::json(MODULE, "cancel", serde_json::json!({ "id": id }))
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct HttpResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
    /// Transport error: `timeout`, `network`, `invalid_url`, `cancelled`,
    /// or a platform message.
    #[serde(default)]
    pub error: Option<String>,
}

impl HttpResponse {
    /// Returns the response if `event` is an `http.response`.
    pub fn from_event(event: &NativeEvent) -> Option<Self> {
        if !event.is(MODULE, "response") {
            return None;
        }
        event.decode()
    }

    pub fn is_success(&self) -> bool {
        self.ok && (200..300).contains(&self.status)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.body)
    }
}

/// Small axios-style client: base URL, default headers, request ids.
#[derive(Clone, Debug)]
pub struct HttpClient {
    base_url: String,
    headers: BTreeMap<String, String>,
    timeout_ms: u32,
    next_id: u64,
}

impl HttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            headers: BTreeMap::new(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            next_id: 1,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn set_header(&mut self, name: &str, value: impl Into<String>) {
        self.headers.insert(name.to_ascii_lowercase(), value.into());
    }

    pub fn remove_header(&mut self, name: &str) {
        self.headers.remove(&name.to_ascii_lowercase());
    }

    pub fn set_bearer(&mut self, token: Option<&str>) {
        match token {
            Some(token) => self.set_header("authorization", format!("Bearer {token}")),
            None => self.remove_header("authorization"),
        }
    }

    pub fn set_timeout_ms(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms;
    }

    /// Build a request. `path` may be absolute (`https://…`) or relative to
    /// the base URL. Keep the returned `id` to match the response.
    pub fn request(&mut self, method: &str, path: &str) -> HttpRequest {
        let id = self.next_id;
        self.next_id += 1;
        let mut request =
            HttpRequest::new(id, method, self.resolve(path)).timeout_ms(self.timeout_ms);
        request.headers = self.headers.clone();
        request
    }

    pub fn get(&mut self, path: &str) -> HttpRequest {
        self.request("GET", path)
    }

    pub fn post(&mut self, path: &str) -> HttpRequest {
        self.request("POST", path)
    }

    pub fn put(&mut self, path: &str) -> HttpRequest {
        self.request("PUT", path)
    }

    pub fn patch(&mut self, path: &str) -> HttpRequest {
        self.request("PATCH", path)
    }

    pub fn delete(&mut self, path: &str) -> HttpRequest {
        self.request("DELETE", path)
    }

    fn resolve(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") || self.base_url.is_empty() {
            return path.to_string();
        }
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

/// Percent-encode a query or form component (RFC 3986 unreserved set kept).
pub fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Build `path?key=value&…` with encoded components.
pub fn with_query(path: &str, params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return path.to_string();
    }
    let query = params
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    let separator = if path.contains('?') { '&' } else { '?' };
    format!("{path}{separator}{query}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_resolves_paths_and_applies_defaults() {
        let mut client = HttpClient::new("https://api.example.com/v1/");
        client.set_bearer(Some("abc"));
        let first = client.get("/places");
        let second = client.get("https://other.example.com/x");
        assert_eq!(first.url, "https://api.example.com/v1/places");
        assert_eq!(second.url, "https://other.example.com/x");
        assert_eq!(first.headers["authorization"], "Bearer abc");
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn request_serializes_to_the_host_shape() {
        let call = HttpRequest::new(7, "post", "https://x.test")
            .json(&serde_json::json!({"q": "sushi"}))
            .into_call();
        assert_eq!(call.module, "http");
        assert_eq!(call.method, "request");
        let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
        assert_eq!(args["id"], 7);
        assert_eq!(args["method"], "POST");
        assert_eq!(args["timeoutMs"], DEFAULT_TIMEOUT_MS);
        assert_eq!(args["headers"]["content-type"], "application/json");
        assert_eq!(args["body"], r#"{"q":"sushi"}"#);
    }

    #[test]
    fn get_without_body_omits_the_field() {
        let call = HttpRequest::new(1, "GET", "https://x.test").into_call();
        let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
        assert!(args.get("body").is_none());
    }

    #[test]
    fn parses_success_and_transport_errors() {
        let event = NativeEvent::parse(
            r#"{"module":"http","method":"response","id":3,"ok":true,"status":201,
                "headers":{"content-type":"application/json"},"body":"{\"n\":2}"}"#,
        )
        .unwrap();
        let response = HttpResponse::from_event(&event).unwrap();
        assert!(response.is_success());
        assert_eq!(response.header("Content-Type"), Some("application/json"));
        let value: serde_json::Value = response.json().unwrap();
        assert_eq!(value["n"], 2);

        let event = NativeEvent::parse(
            r#"{"module":"http","method":"response","id":4,"ok":false,"error":"timeout"}"#,
        )
        .unwrap();
        let response = HttpResponse::from_event(&event).unwrap();
        assert!(!response.is_success());
        assert_eq!(response.error.as_deref(), Some("timeout"));
    }

    #[test]
    fn encodes_query_and_form() {
        assert_eq!(
            with_query("/search", &[("q", "oat milk & co"), ("n", "5")]),
            "/search?q=oat%20milk%20%26%20co&n=5"
        );
        let request = HttpRequest::new(1, "POST", "https://x.test")
            .form(&[("grant_type", "refresh_token"), ("token", "a+b")]);
        assert_eq!(
            request.body.as_deref(),
            Some("grant_type=refresh_token&token=a%2Bb")
        );
    }
}
