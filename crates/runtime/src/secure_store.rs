//! Encrypted key/value storage for secrets (replaces expo-secure-store).
//!
//! iOS stores values in the Keychain; Android encrypts them with an AES-GCM
//! key held in the Android Keystore. Every call answers with a `native`
//! event:
//!
//! ```json
//! {"module":"secureStore","method":"result","op":"get","key":"token",
//!  "ok":true,"value":"…"}
//! ```
//!
//! A missing key is `ok: true, value: null`. Use this for auth tokens only;
//! use [`crate::kv::KvStore`] for ordinary preferences.

use crate::native_api::{NativeCall, NativeEvent};
use serde::Deserialize;
use serde_json::json;

pub const MODULE: &str = "secureStore";

pub fn get(key: &str) -> NativeCall {
    NativeCall::json(MODULE, "get", json!({ "key": key }))
}

pub fn set(key: &str, value: &str) -> NativeCall {
    NativeCall::json(MODULE, "set", json!({ "key": key, "value": value }))
}

pub fn delete(key: &str) -> NativeCall {
    NativeCall::json(MODULE, "delete", json!({ "key": key }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecureOp {
    Get,
    Set,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SecureStoreResult {
    pub op: SecureOp,
    pub key: String,
    pub ok: bool,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl SecureStoreResult {
    pub fn from_event(event: &NativeEvent) -> Option<Self> {
        if !event.is(MODULE, "result") {
            return None;
        }
        event.decode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_carry_key_and_value() {
        let call = set("access_token", "abc");
        assert_eq!(
            (call.module.as_str(), call.method.as_str()),
            ("secureStore", "set")
        );
        let args: serde_json::Value = serde_json::from_str(&call.args).unwrap();
        assert_eq!(args, json!({"key": "access_token", "value": "abc"}));
    }

    #[test]
    fn parses_hits_misses_and_errors() {
        let hit = NativeEvent::parse(
            r#"{"module":"secureStore","method":"result","op":"get","key":"t","ok":true,"value":"v"}"#,
        )
        .unwrap();
        let hit = SecureStoreResult::from_event(&hit).unwrap();
        assert_eq!(hit.op, SecureOp::Get);
        assert_eq!(hit.value.as_deref(), Some("v"));

        let miss = NativeEvent::parse(
            r#"{"module":"secureStore","method":"result","op":"get","key":"t","ok":true,"value":null}"#,
        )
        .unwrap();
        assert_eq!(SecureStoreResult::from_event(&miss).unwrap().value, None);

        let failed = NativeEvent::parse(
            r#"{"module":"secureStore","method":"result","op":"set","key":"t","ok":false,"error":"keystore"}"#,
        )
        .unwrap();
        let failed = SecureStoreResult::from_event(&failed).unwrap();
        assert!(!failed.ok);
        assert_eq!(failed.error.as_deref(), Some("keystore"));
    }
}
