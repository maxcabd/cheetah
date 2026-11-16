//! Typed calls for the built-in native modules. Each function returns a
//! [`NativeCall`] to queue from `update` (return it from
//! `take_native_calls`); results come back as `native` events, which
//! [`NativeEvent::parse`](crate::NativeEvent::parse) reads. Every module
//! answers with `{"module":..,"method":..}` plus the fields documented here;
//! failures carry `"error": "<reason>"`.
//!
//! Modules: [`notifications`], [`linking`], [`auth`], [`dialog`], [`picker`],
//! [`media`], [`files`], [`purchases`].

use crate::native_api::NativeCall;
use serde_json::{json, Value};

/// Local and push notifications.
///
/// Events: `permission {granted}`, `token {token}` (push device token),
/// `received {id, title, body, data}` (arrived while the app is open),
/// `opened {id, data}` (the user tapped one; also on cold start).
pub mod notifications {
    use super::*;

    pub fn request_permission() -> NativeCall {
        NativeCall::json("notifications", "requestPermission", json!({}))
    }

    /// Show a notification after `seconds` (0: now). `data` comes back in
    /// `received`/`opened`.
    pub fn schedule(id: &str, title: &str, body: &str, seconds: f64, data: Value) -> NativeCall {
        NativeCall::json(
            "notifications",
            "schedule",
            json!({"id": id, "title": title, "body": body, "seconds": seconds, "data": data}),
        )
    }

    pub fn cancel(id: &str) -> NativeCall {
        NativeCall::json("notifications", "cancel", json!({"id": id}))
    }

    pub fn cancel_all() -> NativeCall {
        NativeCall::json("notifications", "cancelAll", json!({}))
    }

    /// Ask for a push token (APNs on iOS; needs `push = true` in
    /// cheetah.toml). Android push needs Firebase, which isn't bundled: it
    /// answers `token {error: "unsupported"}`.
    pub fn register_push() -> NativeCall {
        NativeCall::json("notifications", "registerPush", json!({}))
    }

    /// Set the app icon badge (iOS; Android launchers decide themselves).
    pub fn set_badge(count: u32) -> NativeCall {
        NativeCall::json("notifications", "setBadge", json!({"count": count}))
    }
}

/// Deep links (`scheme` in cheetah.toml) and universal / app links (`links`).
///
/// Events: `initial {url}` (null when the app wasn't opened by a link),
/// `url {url}` for links that arrive while running.
pub mod linking {
    use super::*;

    pub fn initial_url() -> NativeCall {
        NativeCall::json("linking", "initialUrl", json!({}))
    }

    /// Open a URL in the system (browser, maps, mail, other apps).
    pub fn open(url: &str) -> NativeCall {
        NativeCall::json("linking", "open", json!({"url": url}))
    }

    /// Path and query of a link, for routing: `nightshade://place/42?x=1`
    /// and `https://nightshade.app/place/42?x=1` both give
    /// `("place/42", [("x", "1")])`.
    pub fn route(url: &str) -> (String, Vec<(String, String)>) {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let path = if url.starts_with("http://") || url.starts_with("https://") {
            path.split_once('/').map_or("", |(_, path)| path)
        } else {
            path
        };
        let pairs = query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                (decode(key), decode(value))
            })
            .collect();
        (path.trim_matches('/').to_string(), pairs)
    }

    fn decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'+' => out.push(b' '),
                b'%' if index + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                    match u8::from_str_radix(hex, 16) {
                        Ok(byte) => {
                            out.push(byte);
                            index += 2;
                        }
                        Err(_) => out.push(b'%'),
                    }
                }
                byte => out.push(byte),
            }
            index += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}

/// Sign-in flows.
///
/// Events: `session {url}` (the redirect URL with the code or token),
/// `apple {user, identityToken, authorizationCode, email, name}`,
/// `biometric {ok}`.
pub mod auth {
    use super::*;

    /// OAuth / web sign-in (Google, GitHub, your own server): opens `url` in
    /// a secure browser sheet and finishes when the provider redirects to
    /// `callback_scheme://…` (use the app's `scheme`).
    pub fn session(url: &str, callback_scheme: &str) -> NativeCall {
        NativeCall::json(
            "auth",
            "session",
            json!({"url": url, "callbackScheme": callback_scheme}),
        )
    }

    /// Sign in with Apple (iOS; `apple_sign_in = true` in cheetah.toml).
    /// Android answers `apple {error: "unsupported"}`: use [`session`] with
    /// Apple's web flow there.
    pub fn apple() -> NativeCall {
        NativeCall::json("auth", "apple", json!({}))
    }

    /// Face ID / Touch ID / fingerprint check (`face_id` permission on iOS).
    pub fn biometric(reason: &str) -> NativeCall {
        NativeCall::json("auth", "biometric", json!({"reason": reason}))
    }
}

/// System dialogs. Event: `result {index}` (-1: cancelled), with the `id`
/// passed in.
pub mod dialog {
    use super::*;

    /// An alert with buttons; `destructive` marks the index shown in red.
    pub fn alert(
        id: &str,
        title: &str,
        message: &str,
        buttons: &[&str],
        destructive: Option<usize>,
    ) -> NativeCall {
        NativeCall::json(
            "dialog",
            "alert",
            json!({"id": id, "title": title, "message": message, "buttons": buttons, "destructive": destructive}),
        )
    }

    /// A list of choices from the bottom (iOS action sheet, Android list
    /// dialog) with a cancel button.
    pub fn action_sheet(
        id: &str,
        title: &str,
        options: &[&str],
        destructive: Option<usize>,
    ) -> NativeCall {
        NativeCall::json(
            "dialog",
            "actionSheet",
            json!({"id": id, "title": title, "options": options, "destructive": destructive}),
        )
    }
}

/// Date and time pickers. Event: `result {id, value}` with an ISO 8601 value
/// (`2026-11-16`, `14:30`, or `2026-11-16T14:30`), or `{id, cancelled: true}`.
pub mod picker {
    use super::*;

    /// `mode`: `date`, `time`, or `datetime`; `value` the initial ISO value.
    pub fn date(id: &str, mode: &str, value: Option<&str>) -> NativeCall {
        NativeCall::json(
            "picker",
            "date",
            json!({"id": id, "mode": mode, "value": value}),
        )
    }
}

/// Camera, photo library, and documents. Event: `result {id, files: [{path,
/// name, mime, size}]}` (empty when cancelled). Files are copied into the
/// app's cache directory, so Rust can read them with `std::fs`.
pub mod media {
    use super::*;

    /// Pick up to `limit` photos (no permission prompt: the system picker).
    pub fn pick_images(id: &str, limit: u32) -> NativeCall {
        NativeCall::json("media", "pickImages", json!({"id": id, "limit": limit}))
    }

    /// Take a photo with the camera (`camera` permission).
    pub fn take_photo(id: &str) -> NativeCall {
        NativeCall::json("media", "takePhoto", json!({"id": id}))
    }

    /// Pick documents; `types` are MIME types like `application/pdf`
    /// (empty: any).
    pub fn pick_documents(id: &str, types: &[&str]) -> NativeCall {
        NativeCall::json("media", "pickDocuments", json!({"id": id, "types": types}))
    }

    /// Save an image file to the photo library (`photos` permission).
    pub fn save_image(id: &str, path: &str) -> NativeCall {
        NativeCall::json("media", "saveImage", json!({"id": id, "path": path}))
    }
}

/// App directories for `std::fs`. Event: `paths {documents, cache}`.
pub mod files {
    use super::*;

    pub fn paths() -> NativeCall {
        NativeCall::json("files", "paths", json!({}))
    }

    /// Share a file through the system share sheet.
    pub fn share(path: &str) -> NativeCall {
        NativeCall::json("files", "share", json!({"path": path}))
    }
}

/// In-app purchases (StoreKit 2, Google Play Billing).
///
/// Events: `products {products: [{id, title, description, price}]}`,
/// `purchase {id, transaction, token}` or `{id, cancelled: true}`,
/// `restore {purchases: [{id, transaction, token}]}`. Verify `token` (iOS:
/// signed transaction; Android: purchase token) on your server before
/// granting anything.
pub mod purchases {
    use super::*;

    pub fn products(ids: &[&str]) -> NativeCall {
        NativeCall::json("purchases", "products", json!({"ids": ids}))
    }

    pub fn purchase(id: &str) -> NativeCall {
        NativeCall::json("purchases", "purchase", json!({"id": id}))
    }

    pub fn restore() -> NativeCall {
        NativeCall::json("purchases", "restore", json!({}))
    }

    /// Mark a consumable as delivered so it can be bought again (Android
    /// requires this within 3 days or the purchase is refunded).
    pub fn finish(transaction: &str, consumable: bool) -> NativeCall {
        NativeCall::json(
            "purchases",
            "finish",
            json!({"transaction": transaction, "consumable": consumable}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_carry_module_method_and_json_args() {
        let call =
            notifications::schedule("n1", "Table ready", "Come in!", 5.0, json!({"place": 42}));
        assert_eq!(
            (call.module.as_str(), call.method.as_str()),
            ("notifications", "schedule")
        );
        let args: Value = serde_json::from_str(&call.args).unwrap();
        assert_eq!(args["seconds"], 5.0);
        assert_eq!(args["data"]["place"], 42);
        let alert = dialog::alert("del", "Delete?", "", &["Cancel", "Delete"], Some(1));
        assert!(alert.args.contains("\"destructive\":1"));
    }

    #[test]
    fn routes_custom_scheme_and_https_links() {
        let expected = (
            "place/42".to_string(),
            vec![("x".to_string(), "a b".to_string())],
        );
        assert_eq!(linking::route("nightshade://place/42?x=a%20b"), expected);
        assert_eq!(
            linking::route("https://nightshade.app/place/42?x=a+b"),
            expected
        );
        assert_eq!(linking::route("nightshade://"), (String::new(), vec![]));
        assert_eq!(linking::route("nightshade://p?bad=%zz").1[0].1, "%zz");
    }
}
