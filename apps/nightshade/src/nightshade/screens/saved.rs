//! `/saved` (`views/pages/saved.view`): the places the user bookmarked,
//! newest first. Each opens its page over the list.

use crate::nightshade::*;
use serde_json::{json, Value};

/// One saved place's row, and the space between rows.
const SAVED_ROW: f32 = 72.0;
const SAVED_GAP: f32 = 10.0;

impl NightshadeApp {
    pub(crate) fn saved_model(&self, context: &ViewContext) -> Value {
        let width = context.viewport.width.max(320.0);
        let rows: Vec<Value> = self
            .saved_places()
            .iter()
            .map(|place| {
                let price = "$".repeat(place.price_tier.min(4) as usize);
                let detail = [
                    place.address.as_str(),
                    place.cuisine.as_str(),
                    price.as_str(),
                ]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
                json!({
                    "key": format!("saved:open:{}", place.id),
                    "name": place.name,
                    "detail": detail,
                })
            })
            .collect();
        let count = rows.len() as f32;
        json!({
            "safe_top": self.host.safe_top.max(20.0),
            "inner": width - 40.0,
            "rows": rows,
            "list_height": count * SAVED_ROW + (count - 1.0).max(0.0) * SAVED_GAP + 24.0,
        })
    }

    /// The saved list, over the map.
    pub(crate) fn open_saved(&mut self) {
        self.open_page(|router| router.push("/saved", "push", None));
    }

    /// A saved place's page, over the saved list.
    pub(crate) fn open_saved_place(&mut self, id: &str) {
        if self.place(id).is_none() {
            return;
        }
        let path = format!("/restaurant/{}", crate::router::encode_segment(id));
        if self.router.push(&path, "push", None) {
            self.haptic("light");
        }
    }
}
