//! `/settings` (`views/pages/settings.view`): location access, where
//! searches work, and this build's version.

use crate::nightshade::*;
use serde_json::{json, Value};

impl NightshadeApp {
    pub(crate) fn settings_model(&self) -> Value {
        let (status, icon, detail) = if self.location_denied {
            (
                "Off",
                "location.slash",
                "Turn it on to see places near you and how far they are.",
            )
        } else if self.center.is_some() {
            (
                "On",
                "location.fill",
                "Nightshade uses where you are to find places near you.",
            )
        } else {
            (
                "Not yet",
                "location",
                "Nightshade asks for your location to find places near you.",
            )
        };
        let coverage = if matches!(self.backend, Backend::Local(_)) {
            "Sample places, for trying Nightshade out."
        } else {
            "Ottawa, for now. Searches elsewhere show Ottawa spots."
        };
        let (version, build) = RUNTIME_VERSION
            .split_once('+')
            .unwrap_or((RUNTIME_VERSION, ""));
        json!({
            "safe_top": self.host.safe_top.max(20.0),
            "location_status": status,
            "location_icon": icon,
            "location_detail": detail,
            "location_off": self.location_denied,
            "coverage": coverage,
            "version": if build.is_empty() {
                format!("Nightshade {version}")
            } else {
                format!("Nightshade {version} ({build})")
            },
        })
    }
}
