//! `/restaurant/[id]` (`views/pages/restaurant/[id].view`): the loader that
//! turns the route's `id` into the page's model, and the page host that
//! presents whichever page the router is on.

use crate::nightshade::*;
use crate::router::Location;
use serde_json::{json, Value};

impl NightshadeApp {
    /// The open pages for `views/map.view`'s PageHost, bottom first: each
    /// is a native page over the one before (a place opened from `/saved`
    /// over the saved list). Empty at the root.
    pub(crate) fn pages_model(&self, context: &ViewContext) -> Value {
        self.router
            .stack()
            .iter()
            .filter(|location| location.page.is_some())
            .enumerate()
            .map(|(depth, location)| self.page_model(location, depth, context))
            .collect::<Vec<_>>()
            .into()
    }

    fn page_model(&self, location: &Location, depth: usize, context: &ViewContext) -> Value {
        let page = location.page.as_deref().unwrap_or_default();
        let mut model = json!({
            "page": page,
            // A fresh native page each time one opens from the map; pages
            // over it get their own.
            "key": if depth == 0 {
                format!("page:{}", self.page_opens)
            } else {
                format!("page:{}:{depth}", self.page_opens)
            },
            // How the native page appears (a "zoom" grows out of the card).
            "props": json!({
                "transition": location.transition,
                "source": location.source,
            })
            .to_string(),
        });
        match page {
            "RestaurantPage" => {
                let id = location
                    .params
                    .get("id")
                    .map(String::as_str)
                    .unwrap_or_default();
                model["restaurant"] = self.restaurant_model(id, context);
            }
            "SavedPage" => model["saved"] = self.saved_model(context),
            "SettingsPage" => model["settings"] = self.settings_model(context),
            _ => {}
        }
        model
    }

    /// The loader for `/restaurant/[id]`: the place from the current results.
    pub(crate) fn restaurant_model(&self, id: &str, context: &ViewContext) -> Value {
        let width = context.viewport.width.max(320.0);
        let safe_top = self.host.safe_top.max(20.0);
        let Some(place) = self.place(id) else {
            return json!({"found": false, "id": id, "safe_top": safe_top});
        };
        let mut model = self.card_model(place);
        // A saved place's hours are from when it was found: not shown as
        // if they were today's, until current ones arrive (`photos_arrived`).
        if !self.in_results(id) && !place.hours_current {
            model["status"] = "saved".into();
        }
        let price = "$".repeat(place.price_tier.min(4) as usize);
        let distance = model["distance"].as_str().unwrap_or_default().to_string();
        let summary = [place.cuisine.as_str(), price.as_str(), distance.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        model["found"] = true.into();
        model["saved"] = self.is_saved(&place.id).into();
        model["safe_top"] = safe_top.into();
        model["inner"] = (width - 40.0).into();
        // The Directions button: the row less share and save (48 each) and
        // the two gaps (10), for the menu over it.
        model["directions_width"] = (width - 40.0 - 2.0 * 48.0 - 2.0 * 10.0).into();
        model["summary"] = summary.into();
        model["amenities"] = place.amenities.clone().into();
        model["credit"] = self.photo_credit(place, &format!("page-photo:{}", place.id));
        model
    }
}
