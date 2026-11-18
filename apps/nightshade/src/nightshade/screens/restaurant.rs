//! `/restaurant/[id]` (`views/pages/restaurant/[id].view`): the loader that
//! turns the route's `id` into the page's model, and the page host that
//! presents whichever page the router is on.

use crate::nightshade::*;
use serde_json::{json, Value};

impl NightshadeApp {
    /// The open page for `views/map.view`'s PageHost, or null at the root.
    pub(crate) fn page_model(&self, context: &ViewContext) -> Value {
        let location = self.router.current();
        let Some(page) = location.page.as_deref() else {
            return Value::Null;
        };
        let mut model = json!({
            "page": page,
            // How the native page appears (a "zoom" grows out of the card).
            "props": json!({
                "transition": location.transition,
                "source": location.source,
            })
            .to_string(),
        });
        if page == "RestaurantPage" {
            let id = location.params.get("id").map(String::as_str).unwrap_or_default();
            model["restaurant"] = self.restaurant_model(id, context);
        }
        model
    }

    /// The loader for `/restaurant/[id]`: the place from the current results.
    pub(crate) fn restaurant_model(&self, id: &str, context: &ViewContext) -> Value {
        let width = context.viewport.width.max(320.0);
        let safe_top = self.host.safe_top.max(20.0);
        let Some(place) = self.visible_places().iter().find(|place| place.id == id) else {
            return json!({"found": false, "id": id, "safe_top": safe_top});
        };
        let mut model = Self::card_model(place);
        let price = "$".repeat(place.price_tier.min(4) as usize);
        let distance = model["distance"].as_str().unwrap_or_default().to_string();
        let summary = [place.cuisine.as_str(), price.as_str(), distance.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        model["found"] = true.into();
        model["safe_top"] = safe_top.into();
        model["inner"] = (width - 40.0).into();
        model["summary"] = summary.into();
        model["amenities"] = place.amenities.clone().into();
        model
    }
}
