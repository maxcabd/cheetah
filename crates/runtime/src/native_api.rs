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

