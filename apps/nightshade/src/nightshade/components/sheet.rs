//! Data for the bottom sheet in `views/sheet.view`.

use crate::nightshade::*;
use serde_json::{json, Value};

impl NightshadeApp {
    pub(crate) fn sheet_height(&self, context: &ViewContext) -> f32 {
        if self.expanded {
            let fraction = if self.host.keyboard_inset > 0.0 {
                0.92
            } else {
                0.85
            };
            (context.viewport.height * fraction).max(520.0)
        } else if self.has_results() {
            RESULT_SHEET_HEIGHT
        } else {
            IDLE_SHEET_HEIGHT
        }
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
            "body_height": (height - 24.0 - SEARCH_HEIGHT - SEARCH_BOTTOM_GAP).max(0.0),
            "body": body,
            "query": self.query,
            "followups": followups,
            "show_followups": self.expanded && self.response().is_some(),
            "suggestions": Self::chips(&SUGGESTIONS, "suggest"),
            "placeholder": if results { "Ask a follow up" } else { "What are you in the mood for?" },
            "draft": self.draft,
            "can_send": !self.draft.trim().is_empty(),
            "results": results,
            // iOS 26: let the system's Liquid Glass sheet and search bar show
            // instead of painting over them.
            "glass": self.host.glass,
            "bg": if self.host.glass { "#00000000" } else { "#171717" },
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
