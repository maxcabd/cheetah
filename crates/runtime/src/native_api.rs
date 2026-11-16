//! Imperative native API calls.
//!
//! Views are declarative and flow through the diff. Some platform APIs are
//! commands instead: haptics, location requests, sharing, clipboard changes.
//! Apps queue those while handling an event and the runtime appends them to
//! the next op frame.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCall {
    pub module: String,
    pub method: String,
    pub args: String,
}

impl NativeCall {
    pub fn new(
        module: impl Into<String>,
        method: impl Into<String>,
        args: impl Into<String>,
    ) -> Self {
        Self {
            module: module.into(),
            method: method.into(),
            args: args.into(),
        }
    }

    /// Build a call whose arguments are serialized as JSON.
    pub fn json<T: Serialize>(
        module: impl Into<String>,
        method: impl Into<String>,
        args: T,
    ) -> Self {
        let args = serde_json::to_string(&args).unwrap_or_else(|_| "{}".into());
        Self::new(module, method, args)
    }
}

/// A parsed `native` event payload sent back by a host module:
/// `{"module": "...", "method": "...", ...fields}`.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeEvent {
    pub module: String,
    pub method: String,
    /// The whole payload, including `module` and `method`.
    pub data: serde_json::Value,
}

impl NativeEvent {
    pub fn parse(payload: &str) -> Option<Self> {
        let data: serde_json::Value = serde_json::from_str(payload).ok()?;
        let module = data.get("module")?.as_str()?.to_owned();
        let method = data.get("method")?.as_str()?.to_owned();
        Some(Self {
            module,
            method,
            data,
        })
    }

    pub fn is(&self, module: &str, method: &str) -> bool {
        self.module == module && self.method == method
    }

    /// Deserialize the payload into a typed result.
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_value(self.data.clone()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_module_and_method() {
        let event = NativeEvent::parse(r#"{"module":"location","method":"result","ok":true}"#)
            .expect("event");
        assert!(event.is("location", "result"));
        assert_eq!(event.data["ok"], true);
    }

    #[test]
    fn rejects_payloads_without_routing_fields() {
        assert!(NativeEvent::parse(r#"{"ok":true}"#).is_none());
        assert!(NativeEvent::parse("not json").is_none());
    }
}
