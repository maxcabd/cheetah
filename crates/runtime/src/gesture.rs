//! Gestures: an element with `.gestures("pan,pinch")` reports them as
//! `gesture` events (keyed like clicks) whose payload [`Gesture::parse`]
//! reads. Pair with `translate_x`/`scale`/`rotate` and `animate` for drag,
//! swipe-to-dismiss, and pinch-to-zoom:
//!
//! ```ignore
//! if let Some(g) = Gesture::parse(&event) {
//!     match g.kind.as_str() {
//!         "pan" if g.is_active() => self.drag = (g.dx, g.dy),
//!         "pan" => self.drag = (0.0, 0.0), // released: animate back
//!         "swipe" if g.direction == "left" => self.dismiss(),
//!         _ => {}
//!     }
//! }
//! ```
//!
//! Payload: `{"type":"pan","state":"changed","dx":12,"dy":0,"vx":340,"vy":0}`.
//! Distances are points from where the gesture began; velocities are points
//! per second.

use crate::app::Event;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Gesture {
    /// `pan`, `swipe`, `long_press`, `pinch`, or `double_tap`.
    #[serde(rename = "type")]
    pub kind: String,
    /// `began`, `changed`, `ended`, or `cancelled` (discrete gestures:
    /// `ended`).
    pub state: String,
    pub dx: f32,
    pub dy: f32,
    pub vx: f32,
    pub vy: f32,
    /// Pinch scale (1 = unchanged).
    pub scale: f32,
    /// Swipe direction: `left`, `right`, `up`, `down`.
    pub direction: String,
}

impl Gesture {
    pub fn parse(event: &Event) -> Option<Self> {
        if event.kind != "gesture" {
            return None;
        }
        serde_json::from_str(&event.payload).ok()
    }

    /// Still in progress (began or changed).
    pub fn is_active(&self) -> bool {
        self.state == "began" || self.state == "changed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gesture_events_only() {
        let event = Event {
            id: 3,
            key: Some("card".into()),
            kind: "gesture".into(),
            payload: r#"{"type":"pan","state":"changed","dx":12.5,"dy":-3,"vx":300,"vy":0}"#.into(),
        };
        let gesture = Gesture::parse(&event).unwrap();
        assert_eq!(gesture.kind, "pan");
        assert!(gesture.is_active());
        assert_eq!((gesture.dx, gesture.dy), (12.5, -3.0));
        let click = Event {
            kind: "click".into(),
            ..event
        };
        assert_eq!(Gesture::parse(&click), None);
    }
}
