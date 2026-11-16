//! Stack navigation (replaces expo-router / react-navigation stack).
//!
//! The route stack lives in Rust; `view` renders `navigator.current()`. The
//! hosts only own the platform back affordances:
//!
//! - Android: the system Back button/gesture.
//! - iOS: a left-edge swipe.
//!
//! Rust tells the host whether back is available with
//! `navigation.setCanGoBack {enabled}` (from [`Navigator::take_sync`]). When
//! it is, the host sends `{"module":"navigation","method":"back"}` instead of
//! performing its default action, so Android Back at the root still leaves
//! the app.
//!
//! ```ignore
//! enum Route { Welcome, Login, Map }
//!
//! fn update(&mut self, event: Event) {
//!     if event.kind == "native" {
//!         if let Some(native) = NativeEvent::parse(&event.payload) {
//!             if self.nav.handle_native(&native) { return; }
//!         }
//!     }
//! }
//!
//! fn take_native_calls(&mut self) -> Vec<NativeCall> {
//!     let mut calls = std::mem::take(&mut self.calls);
//!     calls.extend(self.nav.take_sync());
//!     calls
//! }
//! ```

use crate::native_api::{NativeCall, NativeEvent};
use serde_json::json;

pub const MODULE: &str = "navigation";

#[derive(Clone, Debug)]
pub struct Navigator<R> {
    stack: Vec<R>,
    synced: Option<bool>,
}

impl<R: Clone> Navigator<R> {
    pub fn new(root: R) -> Self {
        Self {
            stack: vec![root],
            synced: None,
        }
    }

    pub fn current(&self) -> &R {
        self.stack.last().expect("navigator stack is never empty")
    }

    pub fn stack(&self) -> &[R] {
        &self.stack
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn can_go_back(&self) -> bool {
        self.stack.len() > 1
    }

    /// `router.push(route)`
    pub fn push(&mut self, route: R) {
        self.stack.push(route);
    }

    /// `router.back()`. Never pops the root; returns the popped route.
    pub fn pop(&mut self) -> Option<R> {
        if self.can_go_back() {
            self.stack.pop()
        } else {
            None
        }
    }

    /// `router.replace(route)`: swap the top route.
    pub fn replace(&mut self, route: R) {
        *self
            .stack
            .last_mut()
            .expect("navigator stack is never empty") = route;
    }

    /// Clear history and start over at `route` (e.g. after login/logout).
    pub fn reset(&mut self, route: R) {
        self.stack.clear();
        self.stack.push(route);
    }

    /// Pop back to the first route matching `predicate`, if any.
    pub fn pop_to(&mut self, predicate: impl Fn(&R) -> bool) -> bool {
        match self.stack.iter().rposition(predicate) {
            Some(index) => {
                self.stack.truncate(index + 1);
                true
            }
            None => false,
        }
    }

    /// Handle a host back request. Returns true if the event was a
    /// navigation event (consumed), whether or not anything was popped.
    pub fn handle_native(&mut self, event: &NativeEvent) -> bool {
        if event.module != MODULE {
            return false;
        }
        if event.method == "back" {
            self.pop();
        }
        true
    }

    /// The host call needed to bring its back affordance in sync, if the
    /// back state changed since the last call. Append it to the frame's
    /// native calls.
    pub fn take_sync(&mut self) -> Option<NativeCall> {
        let enabled = self.can_go_back();
        if self.synced == Some(enabled) {
            return None;
        }
        self.synced = Some(enabled);
        Some(NativeCall::json(
            MODULE,
            "setCanGoBack",
            json!({ "enabled": enabled }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    enum Route {
        Welcome,
        Login,
        Signup,
        Home,
    }

    fn back() -> NativeEvent {
        NativeEvent::parse(r#"{"module":"navigation","method":"back"}"#).unwrap()
    }

    #[test]
    fn push_pop_and_root_is_kept() {
        let mut nav = Navigator::new(Route::Welcome);
        nav.push(Route::Login);
        nav.push(Route::Signup);
        assert_eq!(nav.current(), &Route::Signup);
        assert_eq!(nav.pop(), Some(Route::Signup));
        assert_eq!(nav.pop(), Some(Route::Login));
        assert_eq!(nav.pop(), None);
        assert_eq!(nav.current(), &Route::Welcome);
    }

    #[test]
    fn replace_reset_and_pop_to() {
        let mut nav = Navigator::new(Route::Welcome);
        nav.push(Route::Login);
        nav.replace(Route::Signup);
        assert_eq!(nav.stack(), &[Route::Welcome, Route::Signup]);
        nav.push(Route::Home);
        assert!(nav.pop_to(|route| *route == Route::Welcome));
        assert_eq!(nav.depth(), 1);
        nav.push(Route::Login);
        nav.reset(Route::Home);
        assert_eq!(nav.stack(), &[Route::Home]);
    }

    #[test]
    fn host_back_pops_and_is_consumed_at_root() {
        let mut nav = Navigator::new(Route::Welcome);
        nav.push(Route::Login);
        assert!(nav.handle_native(&back()));
        assert_eq!(nav.current(), &Route::Welcome);
        assert!(nav.handle_native(&back()));
        assert_eq!(nav.current(), &Route::Welcome);

        let other = NativeEvent::parse(r#"{"module":"http","method":"response"}"#).unwrap();
        assert!(!nav.handle_native(&other));
    }

    #[test]
    fn sync_is_emitted_only_on_change() {
        let mut nav = Navigator::new(Route::Welcome);
        let first = nav.take_sync().expect("initial sync");
        assert_eq!(first.args, r#"{"enabled":false}"#);
        assert!(nav.take_sync().is_none());
        nav.push(Route::Login);
        assert_eq!(nav.take_sync().unwrap().args, r#"{"enabled":true}"#);
        nav.push(Route::Signup);
        assert!(nav.take_sync().is_none());
    }
}
