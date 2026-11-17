//! Data for the place cards and carousel in `views/card.view`.

use crate::nightshade::*;
use serde_json::{json, Value};

impl NightshadeApp {
    /// One card's display values: text is formatted here, layout lives in
    /// the view.
    pub(crate) fn card_model(place: &Place) -> Value {
        let drive_minutes = ((place.distance_m / 1000.0) / 30.0 * 60.0).round().max(1.0) as u32;
        let distance = if place.distance_m < 1000.0 {
            format!("{}m", place.distance_m.round() as u32)
        } else {
            format!("{:.1}km", place.distance_m / 1000.0)
        };
        let status = match place.open {
            None => "unknown",
            Some(true) => "open",
            Some(false) => "closed",
        };
        json!({
            "id": place.id,
            "name": place.name,
            "address": place.address,
            "distance": distance,
            "drive_minutes": drive_minutes,
            "price": "$".repeat(place.price_tier.min(4) as usize),
            "status": status,
            "closes": place.closes,
            "opens": place.opens,
            // "8:00 AM" is today; otherwise "tomorrow at …" / "Wed at …".
            "opens_today": place.opens.starts_with(|c: char| c.is_ascii_digit()),
            "has_rating": !place.rating.is_empty(),
            "rating": if place.reviews.is_empty() {
                place.rating.clone()
            } else {
                format!("{} ({})", place.rating, place.reviews)
            },
        })
    }

    pub(crate) fn carousel_model(&self, context: &ViewContext, places: &[Place]) -> Value {
        let width = context.viewport.width.max(320.0);
        let card_width = width * 0.90;
        json!({
            "width": width,
            "content_width": width * places.len() as f32,
            "scroll_x": self.selected as f32 * width,
            "card_width": card_width,
            "card_inner": card_width - 40.0,
            "card_controls": card_width - 32.0,
            "places": places.iter().map(Self::card_model).collect::<Vec<_>>(),
        })
    }
}
