//! Desktop entry point for the dev server (`devserver` feature).
//!
//! Runs the same actor the device hosts run through FFI, but with a caller
//! supplied [`Host`], so `tools/devserver` can forward ops to a device over
//! the network and answer text measurement from it.

use crate::actor::{spawn, ActorHandle, Command};
use crate::app::Viewport;
use std::sync::Arc;

pub use crate::bridge::Host;
pub use crate::layout::TextRequest;

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
