//! Desktop entry point for the dev server (`devserver` feature).
//!
//! Runs the same actor the device hosts run through FFI, but with a caller
//! supplied [`Host`], so `tools/devserver` can forward ops to a device over
//! the network and answer text measurement from it.

use crate::actor::{spawn, spawn_with, ActorHandle, Command};
use crate::app::{Runtime, Viewport};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub use crate::bridge::Host;
pub use crate::layout::TextRequest;
pub use crate::live::{LiteralEdit, LiteralOverrides, LiteralValue};
pub use crate::views::{ViewError, Views};

/// Font-free text size estimate, for when the device can't answer in time.
pub fn rough_measure(text: &str, font_size: f32, max_width: f32) -> (f32, f32) {
    crate::layout::rough_measure(text, font_size, max_width)
}

/// A running app. Dropping it shuts the actor down.
pub struct DevRuntime {
    actor: Option<ActorHandle>,
}

impl DevRuntime {
    /// Start `app` ("nightshade", "nightshade-demo", "chat") against `host`.
    pub fn spawn(
        host: Arc<dyn Host>,
        store_path: impl Into<String>,
        gateway_url: impl Into<String>,
        app: impl Into<String>,
    ) -> Self {
        let actor = spawn(host, store_path.into(), gateway_url.into(), app.into());
        Self { actor: Some(actor) }
    }

    /// Start a runtime built by `make` (on the actor thread). App crates use
    /// this to run their own app type, e.g. wrapped for hot patching.
    pub fn spawn_with(
        host: Arc<dyn Host>,
        gateway_url: impl Into<String>,
        make: impl FnOnce() -> Runtime + Send + 'static,
    ) -> Self {
        let actor = spawn_with(host, gateway_url.into(), make);
        Self { actor: Some(actor) }
    }

    /// Ask the app to re-render with its current (possibly patched) code.
    pub fn rerender(&self) {
        self.send(Command::Rerender);
    }

    /// Apply live literal edits (replacing any previous set) and re-render.
    pub fn set_literals(&self, literals: LiteralOverrides) {
        self.send(Command::Literals(literals));
    }

    /// Replace the app's `.view` templates and re-render.
    pub fn set_views(&self, views: Views) {
        self.send(Command::Views(views));
    }

    /// A handle that can trigger re-renders from other threads, e.g. a
    /// hot-patch callback.
    pub fn rerender_handle(&self) -> Option<RerenderHandle> {
        self.actor
            .as_ref()
            .map(|actor| RerenderHandle(actor.sender.clone()))
    }

    fn send(&self, command: Command) {
        if let Some(actor) = &self.actor {
            let _ = actor.sender.send(command);
        }
    }

    pub fn resize(&self, width: f32, height: f32) {
        self.send(Command::Resize(Viewport { width, height }));
    }

    /// `kind` is an event name ("click", "native", ...).
    pub fn event(&self, id: u32, kind: impl Into<String>, payload: impl Into<String>) {
        self.send(Command::Event {
            id,
            kind: kind.into(),
            payload: payload.into(),
        });
    }

    /// Event by the numeric code the hosts use over FFI.
    pub fn event_code(&self, id: u32, code: u32, payload: impl Into<String>) {
        if let Some(kind) = crate::app::event_kind_from_code(code) {
            self.event(id, kind, payload);
        }
    }

    pub fn lifecycle(&self, active: bool) {
        self.send(Command::Lifecycle(active));
    }
}

#[derive(Clone)]
pub struct RerenderHandle(UnboundedSender<Command>);

impl RerenderHandle {
    pub fn rerender(&self) {
        let _ = self.0.send(Command::Rerender);
    }

    pub fn set_literals(&self, literals: LiteralOverrides) {
        let _ = self.0.send(Command::Literals(literals));
    }

    pub fn set_views(&self, views: Views) {
        let _ = self.0.send(Command::Views(views));
    }
}

impl Drop for DevRuntime {
    fn drop(&mut self) {
        if let Some(mut actor) = self.actor.take() {
            let _ = actor.sender.send(Command::Shutdown);
            if let Some(join) = actor.join.take() {
                let _ = join.join();
            }
        }
    }
}
