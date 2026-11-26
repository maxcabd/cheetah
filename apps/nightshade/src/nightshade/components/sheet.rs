//! Data for the bottom sheet in `views/sheet.view`.

use crate::nightshade::*;
use serde_json::{json, Value};

impl NightshadeApp {
    pub(crate) fn sheet_height(&self, context: &ViewContext) -> f32 {
        if self.expanded {
            self.expanded_sheet_height(context)
        } else {
            self.resting_sheet_height()
        }
    }

    /// The sheet's two detents: iOS drags between them natively.
    fn expanded_sheet_height(&self, context: &ViewContext) -> f32 {
        let fraction = if self.keyboard_open() { 0.92 } else { 0.85 };
        (context.viewport.height * fraction).max(520.0)
    }

    fn resting_sheet_height(&self) -> f32 {
        if self.has_results() {
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

    /// Space under the search bar. The keyboard doesn't change it: the
    /// native sheet lifts the bar above the keyboard (`keyboardView` in
    /// `views/sheet.view`) with the keyboard's own animation.
    pub(crate) fn search_bottom_gap(&self) -> f32 {
        SEARCH_BOTTOM_GAP
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
        let body = match &self.search {
            Resource::Ready(response) if response.places.is_empty() => "no_results",
            Resource::Ready(_) if !self.expanded => "collapsed",
            Resource::Ready(_) => "results",
            Resource::Loading => "searching",
            Resource::Failed(_) => "failed",
            Resource::Idle => "empty",
        };
        // Searches to try after one that found nothing: the backend's
        // follow-ups, else the usual suggestions.
        let other_searches = match self.response() {
            Some(response) if !response.followups.is_empty() => {
                Self::chips(&response.followups, "followup")
            }
            _ => Self::chips(&self.suggestions(), "suggest"),
        };
        let followups = self
            .response()
            .map(|response| Self::chips(&response.followups, "followup"))
            .unwrap_or_default();
        json!({
            "height": height,
            "rest_height": self.resting_sheet_height(),
            "expanded_height": self.expanded_sheet_height(context),
            "detent": if self.expanded { "expanded" } else { "rest" },
            "body_height": (height - 24.0 - SEARCH_HEIGHT - self.search_bottom_gap()).max(0.0),
            "bottom_gap": self.search_bottom_gap(),
            "body": body,
            "query": self.query,
            "notice": self.response().and_then(|response| response.notice.clone()),
            "searching_emoji": SEARCHING_EMOJIS[self.searching_frame % SEARCHING_EMOJIS.len()],
            "followups": followups,
            "show_followups": self.expanded && self.response().is_some(),
            "suggestions": Self::chips(&self.suggestions(), "suggest"),
            "placeholder": if results { "Ask a follow up" } else { "What are you in the mood for?" },
            "draft": self.draft,
            "field_key": format!("search-field:{}", self.field_mount),
            "can_send": !self.draft.trim().is_empty(),
            // Away while the drawer is open (its native sheet can't slide
            // with the map).
            "hidden": self.drawer_open,
            "results": results,
            // Away from Ottawa, the empty sheet says what searches cover;
            // the chips make room for the line.
            "market_note": self
                .outside_market()
                .then_some("Nightshade only covers Ottawa for now."),
            "chips_height": if self.outside_market() { 166 } else { 200 },
            "other_searches": other_searches,
            "failure": self.search.error().map(failure_message),
            // Clear goes back to the suggestions from results, and from a
            // search that failed or found nothing.
            "can_clear": !matches!(self.search, Resource::Idle | Resource::Loading),
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
                })
            })
            .collect()
    }
}

/// What a failed search tells the user: the typed errors from `parse_search`
/// and the place sources, as sentences.
pub(crate) fn failure_message(error: &str) -> String {
    match error {
        "Search timed out" => "That took too long to answer. Try again in a moment.".into(),
        "Can't reach Nightshade" => {
            "Can't reach Nightshade. Check your connection and try again.".into()
        }
        _ if error.starts_with("Search failed") || error == "Unexpected search response" => {
            "Something went wrong on our end. Try again in a moment.".into()
        }
        _ => format!("{error}. Try again in a moment."),
    }
}
