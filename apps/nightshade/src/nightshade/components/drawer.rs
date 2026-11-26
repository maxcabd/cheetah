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
/// A flick this fast (points a second) decides by its direction, wherever
/// the finger lets go; slower, the nearer side wins.
const DRAWER_FLICK: f32 = 400.0;

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
        if !self.drawer_open && self.drawer_drag.is_none() {
            self.drawer_rounded = false;
        }
    }

    /// A finger dragging the map screen: from the open drawer, or from the
    /// map's left edge. It follows the finger, then settles open or shut.
    pub(crate) fn drag_drawer(&mut self, gesture: &crate::gesture::Gesture, from_open: bool) {
        let width = self.drawer_width.get();
        let start = if from_open { width } else { 0.0 };
        let offset = (start + gesture.dx).clamp(0.0, width);
        match gesture.state.as_str() {
            "began" | "changed" => {
                if self.drawer_drag.is_none() && self.expanded {
                    // The sheet goes while the map is aside, keyboard and all.
                    self.expanded = false;
                    self.field_mount += 1;
                }
                self.drawer_drag = Some(offset);
                self.drawer_rounded = true;
            }
            _ => {
                let offset = self.drawer_drag.take().unwrap_or(offset);
                let open = if gesture.vx.abs() >= DRAWER_FLICK {
                    gesture.vx > 0.0
                } else {
                    offset > width / 2.0
                };
                if open {
                    if !self.drawer_open {
                        self.haptic("selection");
                    }
                    self.drawer_open = true;
                } else {
                    self.drawer_open = true;
                    self.close_drawer();
                }
            }
        }
    }

    pub(crate) fn drawer_model(&self, context: &ViewContext) -> Value {
        let screen = context.viewport.width.max(320.0);
        let width = screen * DRAWER_FRACTION;
        self.drawer_width.set(width);
        let saved = self.saved_places().len();
        let offset = match self.drawer_drag {
            Some(offset) => offset,
            None if self.drawer_open => width,
            None => 0.0,
        };
        json!({
            "open": self.drawer_open,
            // Built while open, dragged, or sliding back over it.
            "shown": self.drawer_rounded || self.drawer_drag.is_some(),
            "width": width,
            // The slid-aside map, right of the drawer: tap or drag it back.
            "rest": screen - width,
            "offset": offset,
            // Under a finger, it follows the finger; let go, it springs.
            "animate": self.drawer_drag.is_none(),
            "radius": if self.drawer_rounded { DRAWER_RADIUS } else { 0.0 },
            "safe_top": self.host.safe_top.max(20.0),
            "saved_detail": if saved > 0 { saved.to_string() } else { String::new() },
        })
    }
}
