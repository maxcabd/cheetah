//! Data for the side drawer in `views/drawer.view`: the map's menu button
//! slides the map screen aside to show it, like a chat app's history.

use crate::nightshade::*;
use serde_json::{json, Value};

/// The drawer's width, and how far the map screen slides, as a fraction of
/// the screen.
const DRAWER_FRACTION: f32 = 0.8;
/// The slid-aside map screen's corners.
const DRAWER_RADIUS: f32 = 44.0;
/// How long the map screen takes to slide back (`transition_ms` in
/// `views/map.view`), after which its corners go square again.
const DRAWER_SLIDE_MS: u64 = 450;

impl NightshadeApp {
    pub(crate) fn open_drawer(&mut self) {
        if self.drawer_open {
            return;
        }
        self.drawer_open = true;
        self.drawer_rounded = true;
        // The sheet slides away with the keyboard, if it was up.
        if self.expanded {
            self.expanded = false;
            self.field_mount += 1;
        }
        self.haptic("selection");
    }

    pub(crate) fn close_drawer(&mut self) {
        if !self.drawer_open {
            return;
        }
        self.drawer_open = false;
        self.actions.push(Action::Timer {
            name: "drawer".into(),
            ms: DRAWER_SLIDE_MS,
        });
    }

    /// The map screen is back: square corners, no clipping, no drawer.
    pub(crate) fn drawer_settled(&mut self) {
        if !self.drawer_open {
            self.drawer_rounded = false;
        }
    }

    pub(crate) fn drawer_model(&self, context: &ViewContext) -> Value {
        let width = context.viewport.width.max(320.0) * DRAWER_FRACTION;
        let saved = self.saved_places().len();
        json!({
            "open": self.drawer_open,
            // Built while open and while the screen slides back over it.
            "shown": self.drawer_rounded,
            "width": width,
            "offset": if self.drawer_open { width } else { 0.0 },
            "radius": if self.drawer_rounded { DRAWER_RADIUS } else { 0.0 },
            "safe_top": self.host.safe_top.max(20.0),
            "saved_detail": if saved > 0 { saved.to_string() } else { String::new() },
        })
    }
}
