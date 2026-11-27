//! `/settings` (`views/pages/settings.view`): location access, which maps
//! app Directions opens, distance units, clearing what's kept on the phone,
//! where searches work, and this build's version.

use crate::nightshade::*;
use serde_json::{json, Value};

/// The Directions row's choices: asking each time, then `MAPS_APPS`.
const ASK_MAPS: &str = "Ask Every Time";
const UNITS: [&str; 2] = ["Kilometers", "Miles"];

/// The page's rows, headers, footers and the gaps between them, for its
/// scroll height: four groups (Location 1 row; Preferences, Data, About 2).
const ROW: f32 = 52.0;
const HEADER: f32 = 18.0;
const FOOTER: f32 = 34.0;
const GAP: f32 = 8.0;
const GROUPS: f32 = 4.0;
const ROWS: f32 = 7.0;

impl NightshadeApp {
    pub(crate) fn settings_model(&self, context: &ViewContext) -> Value {
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
        // A row's value, and the section's footer, as iOS Settings has them.
        let (coverage, coverage_detail) = if matches!(self.backend, Backend::Local(_)) {
            ("Samples", "Sample places, for trying Nightshade out.")
        } else {
            ("Ottawa", "Ottawa, for now. Searches elsewhere show Ottawa spots.")
        };
        let (version, build) = RUNTIME_VERSION
            .split_once('+')
            .unwrap_or((RUNTIME_VERSION, ""));
        let width = context.viewport.width.max(320.0);
        let maps_items: Vec<&str> = std::iter::once(ASK_MAPS).chain(MAPS_APPS).collect();
        let saved = self.saved_places().len();
        json!({
            "safe_top": self.host.safe_top.max(20.0),
            // The rows' width, for the menus over them.
            "row_width": width - 32.0,
            "content_height": GROUPS * (HEADER + FOOTER) + ROWS * ROW + (GROUPS * 3.0 - 1.0) * GAP + 24.0 + 32.0,
            "maps_app": self.maps_app.map_or(ASK_MAPS, |app| MAPS_APPS[app]),
            "maps_menu": json!({ "title": "", "items": maps_items }).to_string(),
            "units": UNITS[self.miles as usize],
            "units_menu": json!({ "title": "", "items": UNITS }).to_string(),
            "has_recents": !self.recent_searches.is_empty(),
            "has_saved": saved > 0,
            "location_status": status,
            "location_icon": icon,
            "location_detail": detail,
            "location_off": self.location_denied,
            "coverage": coverage,
            "coverage_detail": coverage_detail,
            "version": if build.is_empty() {
                version.to_string()
            } else {
                format!("{version} ({build})")
            },
        })
    }

    /// The Directions row's menu: a maps app, or asking each time.
    pub(crate) fn choose_maps_app(&mut self, choice: &str) {
        self.maps_app = MAPS_APPS.iter().position(|app| *app == choice);
        self.haptic("selection");
    }

    /// The Distance row's menu: kilometres or miles.
    pub(crate) fn choose_units(&mut self, choice: &str) {
        if let Some(index) = UNITS.iter().position(|units| *units == choice) {
            self.miles = index == 1;
            self.haptic("selection");
        }
    }

    /// A Clear row: ask first, as iOS does before removing anything.
    pub(crate) fn confirm_clear(&mut self, key: &str) {
        let saved = self.saved_places().len();
        let (title, message) = match key {
            "settings:clear-recents" if !self.recent_searches.is_empty() => (
                "Clear Recent Searches?",
                "Your recent searches, and the ones you make often, won't be suggested any more."
                    .to_string(),
            ),
            "settings:clear-saved" if saved > 0 => (
                "Clear Saved Places?",
                match saved {
                    1 => "Your saved place will be removed from this iPhone.".to_string(),
                    n => format!("All {n} saved places will be removed from this iPhone."),
                },
            ),
            _ => return,
        };
        self.calls.push(NativeCall::new(
            "dialog",
            "alert",
            &json!({
                "id": key,
                "title": title,
                "message": message,
                "buttons": ["Cancel", "Clear"],
                "destructive": 1,
            })
            .to_string(),
        ));
        self.haptic("light");
    }

    /// Clear confirmed.
    pub(crate) fn clear(&mut self, key: &str) {
        match key {
            "settings:clear-recents" => {
                self.recent_searches.clear();
                self.habits.clear();
            }
            "settings:clear-saved" => {
                self.saved.clear();
                self.saved_unseen.clear();
                self.saved_filter = None;
            }
            _ => return,
        }
        self.haptic("success");
        self.save_state();
    }
}
