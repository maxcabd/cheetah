//! `/saved` (`views/pages/saved.view`): the places the user bookmarked,
//! newest first, two to a row, filtered by cuisine. Each opens its page over
//! the list.

use crate::nightshade::*;
use serde_json::{json, Value};

/// The grid: two cards a row, the gap between them, the photo's height for
/// its width, and the text under it.
const SAVED_COLUMNS: f32 = 2.0;
const SAVED_GAP: f32 = 12.0;
const SAVED_PHOTO_RATIO: f32 = 0.6;
const SAVED_BODY: f32 = 84.0;

impl NightshadeApp {
    pub(crate) fn saved_model(&self, context: &ViewContext) -> Value {
        let width = context.viewport.width.max(320.0);
        let saved = self.saved_places();
        // The filter's choices: every cuisine saved, in list order.
        let mut cuisines: Vec<&str> = Vec::new();
        for place in saved.iter() {
            if !place.cuisine.is_empty() && !cuisines.contains(&place.cuisine.as_str()) {
                cuisines.push(&place.cuisine);
            }
        }
        // A filter nothing matches any more (its last place un-saved) shows all.
        let filter = self
            .saved_filter
            .as_deref()
            .filter(|cuisine| cuisines.contains(cuisine));
        let card_width = ((width - 40.0 - SAVED_GAP * (SAVED_COLUMNS - 1.0)) / SAVED_COLUMNS).floor();
        let photo_height = (card_width * SAVED_PHOTO_RATIO).round();
        let card_height = photo_height + SAVED_BODY;
        let cards: Vec<Value> = saved
            .iter()
            .filter(|place| filter.map_or(true, |cuisine| place.cuisine == cuisine))
            .map(|place| {
                let mut card = Self::card_model(place);
                card["key"] = format!("saved:open:{}", place.id).into();
                card["detail"] = [place.cuisine.as_str(), card["price"].as_str().unwrap_or("")]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
                    .into();
                // The first photo alone (one page: no dots, no swiping).
                card["photo_props"] = match place.photos.first() {
                    Some(photo) => json!({ "photos": [photo] }),
                    None => json!({ "src": "asset://second-cup-card-full.jpeg", "pages": 1 }),
                }
                .to_string()
                .into();
                card
            })
            .collect();
        let rows: Vec<Value> = cards
            .chunks(SAVED_COLUMNS as usize)
            .map(|pair| Value::Array(pair.to_vec()))
            .collect();
        let count = rows.len() as f32;
        let mut items = vec!["All".to_string()];
        items.extend(cuisines.iter().map(|cuisine| cuisine.to_string()));
        json!({
            "safe_top": self.host.safe_top.max(20.0),
            "has_saved": !saved.is_empty(),
            "filtered": filter.is_some(),
            "filter_props": json!({ "title": "", "items": items }).to_string(),
            "rows": rows,
            "card_width": card_width,
            "photo_height": photo_height,
            "card_height": card_height,
            "list_height": count * card_height + (count - 1.0).max(0.0) * SAVED_GAP + 24.0,
        })
    }

    /// The filter menu's choice: a cuisine, or "All".
    pub(crate) fn filter_saved(&mut self, choice: &str) {
        self.saved_filter = (choice != "All").then(|| choice.to_string());
        self.haptic("light");
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
