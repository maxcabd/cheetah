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
            // The photo pager's settings: the place's photos, or the
            // placeholder (one bundled photo, five pages) until it has some.
            "photo_props": if place.photos.is_empty() {
                json!({"src": "asset://second-cup-card-full.jpeg", "pages": 5})
            } else {
                json!({"photos": place.photos})
            }
            .to_string(),
            // The directions button's menu: which maps app.
            "directions_menu": json!({ "title": "", "items": MAPS_APPS }).to_string(),
            // The place's page; the id may hold a "/" (osm:node/42).
            "route": format!("/restaurant/{}", crate::router::encode_segment(&place.id)),
            "status": status,
            "closes": place.closes,
            "drive_thru_only": place.drive_thru_only,
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

    /// The cards to build: the selected one, its neighbours, and any built
    /// before for these results, with no gap between. A dozen cards with
    /// their photos and glass at once held up the screen for 150 ms; the rest
    /// are empty pages until `cards_tick` gets to them.
    fn built_cards(&self) -> std::ops::RangeInclusive<usize> {
        let first = self.selected.saturating_sub(CARDS_BESIDE);
        let last = self.selected + CARDS_BESIDE;
        let (first, last) = match self.cards_built.get() {
            Some((from, to)) => (first.min(from), last.max(to)),
            None => (first, last),
        };
        self.cards_built.set(Some((first, last)));
        first..=last
    }

    /// The card to build next, if there's one to: the nearest side first, and
    /// ahead before behind.
    fn next_card(&self) -> Option<(usize, usize)> {
        let count = self.response()?.places.len();
        let built = self.built_cards();
        let (first, last) = (*built.start(), *built.end());
        let ahead = last + 1 < count && last < self.selected + CARDS_AHEAD;
        let behind = first > 0 && first + CARDS_AHEAD > self.selected;
        if ahead && (!behind || last - self.selected.min(last) <= self.selected - first) {
            Some((first, last + 1))
        } else if behind {
            Some((first - 1, last))
        } else {
            None
        }
    }

    /// The cards moved, or are new: build more once they're still.
    pub(crate) fn build_ahead(&mut self) {
        self.cards_moved = true;
        self.schedule_card(CARDS_SETTLE_MS);
    }

    fn schedule_card(&mut self, ms: u64) {
        if self.cards_timer || self.next_card().is_none() {
            return;
        }
        self.cards_timer = true;
        self.actions.push(Action::Timer {
            name: "cards".into(),
            ms,
        });
    }

    /// Build one more card, so no frame has more than one to do.
    pub(crate) fn cards_tick(&mut self) {
        self.cards_timer = false;
        if std::mem::take(&mut self.cards_moved) {
            self.schedule_card(CARDS_SETTLE_MS);
            return;
        }
        if let Some(built) = self.next_card() {
            self.cards_built.set(Some(built));
        }
        self.schedule_card(CARDS_STEP_MS);
    }

    pub(crate) fn carousel_model(&self, context: &ViewContext, places: &[Place]) -> Value {
        let width = context.viewport.width.max(320.0);
        // Cards narrower than the screen, one per page: the carousel is one
        // page wide and centered, and doesn't clip, so the neighbouring
        // cards peek in at the edges to show there are more results.
        let card_width = width * CARD_WIDTH_FRACTION;
        let page = card_width + CARD_GAP;
        let built = self.built_cards();
        json!({
            "width": width,
            "page": page,
            "left": (width - page) / 2.0,
            // The page fits the card plus room for its scale-down.
            "height": self.card_height() + 20.0,
            "minimized": self.cards_minimized,
            "content_width": page * places.len() as f32,
            "scroll_x": self.selected as f32 * page,
            "card_width": card_width,
            "card_inner": card_width - 40.0,
            "card_controls": card_width - 32.0,
            "places": places
                .iter()
                .enumerate()
                .map(|(index, place)| {
                    if !built.contains(&index) {
                        return json!({ "built": false });
                    }
                    let mut card = Self::card_model(place);
                    card["built"] = true.into();
                    card["saved"] = self.is_saved(&place.id).into();
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
