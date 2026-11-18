//! File-based routes, like Next.js / expo-router, over the [`Navigator`]
//! stack.
//!
//! A page is a `.view` file under `views/pages/`; its path is its route and
//! its first component renders it:
//!
//! ```text
//! views/pages/restaurant/[id].view    /restaurant/:id   -> {"id": "java"}
//! views/pages/menu/[...items].view    /menu/a/b         -> {"items": "a/b"}
//! views/pages/about.view              /about
//! ```
//!
//! `/` is the app's root screen (no page). Views open a page with
//! `link="/restaurant/{place.id}"` on any element; a press reaches the app as
//! a `navigate` event, which [`Router::handle_event`] applies. `transition`
//! says how the page appears: "push" (default) or "zoom" (it grows out of the
//! pressed element, the `source`, found by its `test_id`). The host's back
//! gesture pops, as with [`Navigator`].

use crate::app::Event;
use crate::nav::Navigator;
use crate::native_api::{NativeCall, NativeEvent};
use serde::Serialize;
use std::collections::BTreeMap;

/// One open screen: where it is, the page that renders it, and how it opened.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Location {
    pub path: String,
    /// The page component; `None` at the root (`/`).
    pub page: Option<String>,
    pub params: BTreeMap<String, String>,
    /// "push" or "zoom".
    pub transition: String,
    /// `test_id` of the element a "zoom" grows out of.
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct Route {
    /// `["restaurant", "[id]"]`
    segments: Vec<String>,
    page: String,
}

#[derive(Clone, Debug)]
pub struct Router {
    routes: Vec<Route>,
    nav: Navigator<Location>,
}

impl Router {
    /// Routes from the app's views: every `views/pages/**.view` file, rendered
    /// by its first `component`.
    pub fn from_views(views: &[(&str, &str)]) -> Self {
        let routes = views
            .iter()
            .filter_map(|(path, source)| {
                let route = path.strip_prefix("views/pages/")?.strip_suffix(".view")?;
                let page = source.lines().find_map(|line| {
                    line.trim().strip_prefix("component ").map(|name| name.trim().to_string())
                })?;
                let segments = route
                    .split('/')
                    .filter(|segment| !segment.is_empty() && *segment != "index")
                    .map(str::to_string)
                    .collect();
                Some(Route { segments, page })
            })
            .collect();
        Self {
            routes,
            nav: Navigator::new(Location {
                path: "/".into(),
                transition: "push".into(),
                ..Location::default()
            }),
        }
    }

    /// The page and params for `path`, if a page matches. Literal segments
    /// win over `[param]`s, which win over `[...rest]`.
    pub fn resolve(&self, path: &str) -> Option<(String, BTreeMap<String, String>)> {
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        let mut best: Option<(u32, &Route, BTreeMap<String, String>)> = None;
        for route in &self.routes {
            if let Some((score, params)) = match_route(&route.segments, &parts) {
                if best.as_ref().map_or(true, |(top, ..)| score > *top) {
                    best = Some((score, route, params));
                }
            }
        }
        best.map(|(_, route, params)| (route.page.clone(), params))
    }

    pub fn current(&self) -> &Location {
        self.nav.current()
    }

    pub fn stack(&self) -> &[Location] {
        self.nav.stack()
    }

    /// Open `path`; false (nothing happens) if no page matches it.
    pub fn push(&mut self, path: &str, transition: &str, source: Option<String>) -> bool {
        let Some((page, params)) = self.resolve(path) else {
            return false;
        };
        self.nav.push(Location {
            path: path.to_string(),
            page: Some(page),
            params,
            transition: transition.to_string(),
            source,
        });
        true
    }

    /// Close the top page; false at the root.
    pub fn back(&mut self) -> bool {
        self.nav.pop().is_some()
    }

    /// Apply a `navigate` event (a pressed `link`). True if it was one.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        if event.kind != "navigate" {
            return false;
        }
        let Ok(data) = serde_json::from_str::<serde_json::Value>(&event.payload) else {
            return true;
        };
        if let Some(path) = data["path"].as_str() {
            self.push(
                path,
                data["transition"].as_str().unwrap_or("push"),
                data["source"].as_str().map(str::to_string),
            );
        }
        true
    }

    /// The host's back gesture or button (see [`Navigator::handle_native`]).
    pub fn handle_native(&mut self, event: &NativeEvent) -> bool {
        self.nav.handle_native(event)
    }

    /// Keep the host's back affordance in step (see [`Navigator::take_sync`]).
    pub fn take_sync(&mut self) -> Option<NativeCall> {
        self.nav.take_sync()
    }
}

/// A match's specificity (higher wins) and params, or None.
fn match_route(segments: &[String], parts: &[&str]) -> Option<(u32, BTreeMap<String, String>)> {
    let mut params = BTreeMap::new();
    let mut score = 0;
    for (index, segment) in segments.iter().enumerate() {
        if let Some(name) = segment.strip_prefix("[...").and_then(|s| s.strip_suffix(']')) {
            if index >= parts.len() {
                return None;
            }
            params.insert(name.to_string(), parts[index..].join("/"));
            return Some((score, params));
        }
        let part = parts.get(index)?;
        if let Some(name) = segment.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            params.insert(name.to_string(), (*part).to_string());
            score += 1;
        } else if segment == part {
            score += 2;
        } else {
            return None;
        }
    }
    (segments.len() == parts.len()).then_some((score, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> Router {
        Router::from_views(&[
            ("views/theme.view", "const BG = \"#000\""),
            ("views/pages/restaurant/[id].view", "# A place\ncomponent RestaurantPage\n  column"),
            ("views/pages/restaurant/new.view", "component NewRestaurant\n  column"),
            ("views/pages/menu/[...items].view", "component Menu\n  column"),
            ("views/pages/about.view", "component About\n  column"),
        ])
    }

    #[test]
    fn pages_come_from_view_paths() {
        let router = router();
        let (page, params) = router.resolve("/restaurant/java").unwrap();
        assert_eq!(page, "RestaurantPage");
        assert_eq!(params["id"], "java");
        assert_eq!(router.resolve("/about").unwrap().0, "About");
        assert_eq!(router.resolve("/menu/drinks/hot").unwrap().1["items"], "drinks/hot");
        assert!(router.resolve("/nowhere").is_none());
        assert!(router.resolve("/restaurant").is_none());
    }

    #[test]
    fn literal_segments_beat_params() {
        assert_eq!(router().resolve("/restaurant/new").unwrap().0, "NewRestaurant");
    }

    #[test]
    fn navigate_events_push_and_back_pops() {
        let mut router = router();
        assert_eq!(router.current().page, None);
        let event = Event {
            id: 0,
            key: Some("place-java".into()),
            kind: "navigate".into(),
            payload: r#"{"path":"/restaurant/java","transition":"zoom","source":"card-java"}"#.into(),
        };
        assert!(router.handle_event(&event));
        let top = router.current();
        assert_eq!(top.page.as_deref(), Some("RestaurantPage"));
        assert_eq!(top.transition, "zoom");
        assert_eq!(top.source.as_deref(), Some("card-java"));
        assert!(router.take_sync().is_some(), "the host learns it can go back");

        assert!(router.back());
        assert_eq!(router.current().path, "/");
        assert!(!router.back(), "never pops the root");
    }
}
