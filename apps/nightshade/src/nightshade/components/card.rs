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

    /// Height of the result cards: full or minimized.
    pub(crate) fn card_height(&self) -> f32 {
        if self.cards_minimized {
            MINI_CARD_HEIGHT
        } else {
            CARD_HEIGHT
        }
    }

    pub(crate) fn carousel_model(&self, context: &ViewContext, places: &[Place]) -> Value {
        let width = context.viewport.width.max(320.0);
        let card_width = width * 0.90;
        json!({
            "width": width,
            // The page fits the card plus room for its scale-down.
            "height": self.card_height() + 20.0,
            "minimized": self.cards_minimized,
            "content_width": width * places.len() as f32,
            "scroll_x": self.selected as f32 * width,
            "card_width": card_width,
            "card_inner": card_width - 40.0,
            "card_controls": card_width - 32.0,
            "places": places
                .iter()
                .map(|place| {
                    let mut card = Self::card_model(place);
                    // The photo's dominant color, once it has loaded.
                    if let Some(color) = self.photo_colors.get(&place.id) {
                        card["color"] = color.as_str().into();
                        card["light"] = is_light(color).into();
                    }
                    card
                })
                .collect::<Vec<_>>(),
        })
    }
}

/// Whether text on `color` ("#rrggbb") reads better dark: its relative
/// luminance (sRGB, WCAG) is above the midpoint between black and white text.
pub(crate) fn is_light(color: &str) -> bool {
    let hex = color.trim_start_matches('#');
    let channel = |index: usize| {
        let value = u8::from_str_radix(hex.get(index..index + 2).unwrap_or("00"), 16).unwrap_or(0);
        let value = value as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
    luminance > 0.179
}
