//! Over-the-air view updates: ship `.view` changes without an app release.
//!
//! `cheetah publish --app <app>` bundles the app's `.view` files into one
//! JSON file; host it anywhere (a CDN, S3, GitHub Pages). The app checks
//! that URL at launch:
//!
//! ```ignore
//! // in new():
//! let mut updates = Updates::new("https://cdn.example.com/views-1.0.0+1.json", "1.0.0+1");
//! calls.extend(updates.start());
//!
//! // in update():
//! if updates.on_event(&event) { return; }
//!
//! // App::take_views:
//! fn take_views(&mut self) -> Option<Vec<(String, String)>> { self.updates.take_views() }
//! ```
//!
//! Safety:
//! - A bundle applies only when its `runtime` matches the app's (by default
//!   the app's `version+build` from cheetah.toml): views can reference model
//!   fields, so they are tied to the Rust code they were written against.
//! - A bundle must parse before it is saved or applied; a broken one is
//!   ignored and the current views stay.
//! - New bundles apply on the next launch (no layout change mid-session)
//!   unless [`Updates::apply_immediately`] is set. The last good bundle is
//!   cached in the app's documents directory and loaded at launch.
//!
//! Only views change over the air. Rust code and native code still ship
//! through the stores; that is also what Apple's rules allow (interpreted
//! content, no new native behavior).

use crate::app::Event;
use crate::http::{HttpRequest, HttpResponse};
use crate::native_api::{NativeCall, NativeEvent};
use crate::platform::files;
use crate::views::Views;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bundle format version this runtime reads.
pub const FORMAT: u32 = 1;
/// HTTP request id for the update check (apart from app requests).
const REQUEST_ID: u64 = u64::MAX - 7;
const CACHE_FILE: &str = "cheetah-views.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: u32,
    /// Which app build these views were written for.
    pub runtime: String,
    /// Content hash; a new value means new views.
    pub version: String,
    pub files: Vec<BundleFile>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BundleFile {
    pub name: String,
    pub source: String,
}

impl Bundle {
    /// Parse and check a bundle for `runtime`; the views must parse.
    pub fn validate(text: &str, runtime: &str) -> Result<Self, String> {
        let bundle: Bundle =
            serde_json::from_str(text).map_err(|error| format!("not a views bundle: {error}"))?;
        if bundle.format != FORMAT {
            return Err(format!(
                "bundle format {} (this app reads {FORMAT})",
                bundle.format
            ));
        }
        if bundle.runtime != runtime {
            return Err(format!(
                "bundle is for runtime {}, this app is {runtime}",
                bundle.runtime
            ));
        }
        Views::parse(&bundle.sources())
            .map_err(|error| format!("bundle views don't parse: {error}"))?;
        Ok(bundle)
    }

    pub fn sources(&self) -> Vec<(&str, &str)> {
        self.files
            .iter()
            .map(|file| (file.name.as_str(), file.source.as_str()))
            .collect()
    }

    fn owned_sources(&self) -> Vec<(String, String)> {
        self.files
            .iter()
            .map(|file| (file.name.clone(), file.source.clone()))
            .collect()
    }
}

pub struct Updates {
    url: String,
    runtime: String,
    apply_immediately: bool,
    documents: Option<PathBuf>,
    /// Version on screen (None: the views built into the app).
    current: Option<String>,
    /// Downloaded before the documents path was known.
    staged: Option<Bundle>,
    pending: Option<Vec<(String, String)>>,
    last_error: Option<String>,
}

impl Updates {
    /// Check `url` for bundles published for `runtime`.
    pub fn new(url: impl Into<String>, runtime: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            runtime: runtime.into(),
            apply_immediately: false,
            documents: None,
            current: None,
            staged: None,
            pending: None,
            last_error: None,
        }
    }

    /// Swap in a new bundle as soon as it downloads (default: next launch).
    pub fn apply_immediately(mut self, yes: bool) -> Self {
        self.apply_immediately = yes;
        self
    }

    /// Calls to queue at launch: the documents directory (for the cached
    /// bundle) and the update check.
    pub fn start(&mut self) -> Vec<NativeCall> {
        vec![
            files::paths(),
            HttpRequest::new(REQUEST_ID, "GET", self.url.clone())
                .timeout_ms(15_000)
                .into_call(),
        ]
    }

    /// The bundle version on screen, if any.
    pub fn version(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Why the last bundle was rejected, if one was.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Views to swap in, for `App::take_views`.
    pub fn take_views(&mut self) -> Option<Vec<(String, String)>> {
        self.pending.take()
    }

    /// Take this module's events; true when `event` was one of them.
    pub fn on_event(&mut self, event: &Event) -> bool {
        if event.kind != "native" {
            return false;
        }
        let Some(native) = NativeEvent::parse(&event.payload) else {
            return false;
        };
        if native.is("files", "paths") {
            if let Some(path) = native.data["documents"].as_str() {
                self.on_documents(PathBuf::from(path));
            }
            // Other code may want the paths too.
            return false;
        }
        if let Some(response) = HttpResponse::from_event(&native) {
            if response.id != REQUEST_ID {
                return false;
            }
            self.on_response(&response);
            return true;
        }
        false
    }

    fn on_documents(&mut self, documents: PathBuf) {
        // The cached bundle from an earlier download: what this launch shows.
        if let Ok(text) = std::fs::read_to_string(documents.join(CACHE_FILE)) {
            match Bundle::validate(&text, &self.runtime) {
                Ok(bundle) => {
                    self.current = Some(bundle.version.clone());
                    self.pending = Some(bundle.owned_sources());
                }
                // Left from an older build: drop it.
                Err(_) => {
                    let _ = std::fs::remove_file(documents.join(CACHE_FILE));
                }
            }
        }
        self.documents = Some(documents);
        if let Some(bundle) = self.staged.take() {
            self.accept(bundle);
        }
    }

    fn on_response(&mut self, response: &HttpResponse) {
        if !response.ok || !response.is_success() {
            // Offline or nothing published: keep what we have.
            return;
        }
        match Bundle::validate(&response.body, &self.runtime) {
            Ok(bundle) => {
                self.last_error = None;
                if self.documents.is_some() {
                    self.accept(bundle);
                } else {
                    self.staged = Some(bundle);
                }
            }
            Err(error) => self.last_error = Some(error),
        }
    }

    fn accept(&mut self, bundle: Bundle) {
        if self.current.as_deref() == Some(bundle.version.as_str()) {
            return;
        }
        if let Some(documents) = &self.documents {
            if let Ok(text) = serde_json::to_string(&bundle) {
                let _ = std::fs::write(documents.join(CACHE_FILE), text);
            }
        }
        if self.apply_immediately {
            self.current = Some(bundle.version.clone());
            self.pending = Some(bundle.owned_sources());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bundle(runtime: &str, version: &str, source: &str) -> String {
        json!({
            "format": 1,
            "runtime": runtime,
            "version": version,
            "files": [{"name": "a.view", "source": source}],
        })
        .to_string()
    }

    fn native(payload: serde_json::Value) -> Event {
        Event {
            id: 0,
            key: None,
            kind: "native".into(),
            payload: payload.to_string(),
        }
    }

    fn response(body: &str) -> Event {
        native(
            json!({"module": "http", "method": "response", "id": REQUEST_ID, "ok": true, "status": 200, "body": body}),
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cheetah-updates-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const GOOD: &str = "component A\n  text \"v2\"\n";

    #[test]
    fn validates_format_runtime_and_views() {
        assert!(Bundle::validate(&bundle("1.0.0+1", "x", GOOD), "1.0.0+1").is_ok());
        assert!(Bundle::validate(&bundle("1.0.0+2", "x", GOOD), "1.0.0+1")
            .unwrap_err()
            .contains("runtime"));
        assert!(
            Bundle::validate(&bundle("1.0.0+1", "x", "component A\n  txt\n"), "1.0.0+1")
                .unwrap_err()
                .contains("don't parse")
        );
        assert!(Bundle::validate("<html>", "1.0.0+1").is_err());
    }

    #[test]
    fn downloads_apply_next_launch_from_the_cache() {
        let dir = temp_dir("next-launch");
        let paths = native(
            json!({"module": "files", "method": "paths", "documents": dir.to_str().unwrap(), "cache": ""}),
        );

        let mut first = Updates::new("https://x/views.json", "1.0.0+1");
        assert_eq!(first.start().len(), 2);
        assert!(!first.on_event(&paths), "paths stay visible to the app");
        assert!(first.on_event(&response(&bundle("1.0.0+1", "v2", GOOD))));
        assert_eq!(first.take_views(), None, "not mid-session");
        assert!(dir.join(CACHE_FILE).exists());

        let mut second = Updates::new("https://x/views.json", "1.0.0+1");
        second.on_event(&paths);
        assert_eq!(second.version(), Some("v2"));
        assert_eq!(second.take_views().unwrap()[0].1, GOOD);

        // A new app build ignores and removes the old build's cache.
        let mut upgraded = Updates::new("https://x/views.json", "1.1.0+2");
        upgraded.on_event(&paths);
        assert_eq!(upgraded.take_views(), None);
        assert!(!dir.join(CACHE_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_immediately_and_bad_bundles() {
        let dir = temp_dir("immediate");
        let mut updates = Updates::new("https://x/views.json", "1.0.0+1").apply_immediately(true);
        // The response can arrive before the documents path.
        updates.on_event(&response(&bundle("1.0.0+1", "v3", GOOD)));
        assert_eq!(updates.take_views(), None);
        updates.on_event(&native(
            json!({"module": "files", "method": "paths", "documents": dir.to_str().unwrap()}),
        ));
        assert_eq!(updates.version(), Some("v3"));
        assert!(updates.take_views().is_some());

        updates.on_event(&response(&bundle("1.0.0+1", "v4", "component A\n  nope\n")));
        assert!(updates.last_error().unwrap().contains("don't parse"));
        assert_eq!(updates.version(), Some("v3"));
        let other = native(
            json!({"module": "http", "method": "response", "id": 1, "ok": true, "status": 200, "body": ""}),
        );
        assert!(!updates.on_event(&other), "app requests pass through");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
