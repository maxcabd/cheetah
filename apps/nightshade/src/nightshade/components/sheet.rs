//! Data for the bottom sheet in `views/sheet.view`.

use crate::nightshade::*;
use serde_json::{json, Value};

impl NightshadeApp {
    pub(crate) fn sheet_height(&self, context: &ViewContext) -> f32 {
        if self.expanded {
            let fraction = if self.keyboard_open() { 0.92 } else { 0.85 };
            (context.viewport.height * fraction).max(520.0)
        } else if self.has_results() {
            RESULT_SHEET_HEIGHT
        } else {
            IDLE_SHEET_HEIGHT
        }
    }

    /// The host reports the keyboard as an inset from the screen bottom that
    /// rests at the safe area, so only an inset above it is a keyboard.
    fn keyboard_open(&self) -> bool {
        self.host.keyboard_inset > self.host.safe_bottom + 1.0
    }

    /// Space under the search bar. The native sheet stays put when the
    /// keyboard opens, so the bar is lifted above the keyboard here.
    pub(crate) fn search_bottom_gap(&self) -> f32 {
        if self.keyboard_open() {
            self.host.keyboard_inset + KEYBOARD_GAP
        } else {
            SEARCH_BOTTOM_GAP
        }
    }

    /// Search the typed text and let the keyboard go.
    pub(crate) fn send_draft(&mut self) {
        let query = self.draft.clone();
        self.field_mount += 1;
        self.run_search(&query);
    }

    pub(crate) fn sheet_model(&self, context: &ViewContext) -> Value {
        let height = self.sheet_height(context);
        let results = self.has_results();
        let body = if results && !self.expanded {
            "collapsed"
        } else {
            match &self.search {
                Resource::Ready(_) => "results",
                Resource::Loading => "searching",
                _ => "empty",
            }
        };
        let followups = self
            .response()
            .map(|response| Self::chips(&response.followups, "followup"))
            .unwrap_or_default();
        json!({
            "height": height,
            "body_height": (height - 24.0 - SEARCH_HEIGHT - self.search_bottom_gap()).max(0.0),
            "bottom_gap": self.search_bottom_gap(),
            "body": body,
            "query": self.query,
            "searching_emoji": SEARCHING_EMOJIS[self.searching_frame % SEARCHING_EMOJIS.len()],
            "followups": followups,
            "show_followups": self.expanded && self.response().is_some(),
            "suggestions": Self::chips(&SUGGESTIONS, "suggest"),
            "placeholder": if results { "Ask a follow up" } else { "What are you in the mood for?" },
            "draft": self.draft,
            "field_key": format!("search-field:{}", self.field_mount),
            "can_send": !self.draft.trim().is_empty(),
            "results": results,
            "glass": self.host.glass,
        })
    }

    /// Chip data; keys are `prefix:index`, matched in `update`.
    fn chips(labels: &[impl AsRef<str>], prefix: &str) -> Vec<Value> {
        labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let label = label.as_ref();
                json!({
                    "label": label,
                    "key": format!("{prefix}:{index}"),
                    "width": Self::chip_width(label),
                })
            })
            .collect()
    }
}
