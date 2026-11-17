//! The map screen (`views/map.view`): region, markers, and the data its
//! carousel and sheet render.

use crate::nightshade::*;

const INITIAL_LAT: f32 = 37.7749;
const INITIAL_LNG: f32 = -122.4194;
/// Height of a map pin above its coordinate.
const PIN_HEIGHT: f32 = 40.0;

impl NightshadeApp {
    /// Full-screen map, result cards over it, and the search sheet.
    pub(crate) fn map_screen(&self, context: &ViewContext) -> Element {
        let (latitude, longitude, latitude_delta, longitude_delta) =
            self.region(context.viewport.width, context.viewport.height);
        let carousel = if self.has_results() {
            self.carousel_model(context, self.visible_places())
        } else {
            serde_json::Value::Null
        };
        context.view(
            "MapScreen",
            &serde_json::json!({
                "region": {
                    "lat": latitude,
                    "lng": longitude,
                    "lat_delta": latitude_delta,
                    "lng_delta": longitude_delta,
                    "satellite": self.satellite,
                    "pins": self.pins(),
                    "safe_top": self.host.safe_top.max(20.0),
                },
                "carousel": carousel,
                "sheet": self.sheet_model(context),
            }),
        )
    }

    pub(crate) fn visible_places(&self) -> &[Place] {
        self.response()
            .map(|response| response.places.as_slice())
            .unwrap_or(&self.sample_places)
    }

    /// Map region as (lat, lng, lat delta, lng delta). With results, the
    /// pins fit the band between the status bar and the cards, so the cards
    /// never cover them.
    pub(crate) fn region(&self, width: f32, height: f32) -> (f32, f32, f32, f32) {
        if !self.has_results() {
            let (latitude, longitude) = self.center.unwrap_or((INITIAL_LAT, INITIAL_LNG));
            return (latitude, longitude, 0.025, 0.025);
        }

        let places = self.visible_places();
        let Some(first) = places.first() else {
            return (INITIAL_LAT, INITIAL_LNG, 0.025, 0.025);
        };
        let shown: Vec<&Place> = if self.focused || places.len() == 1 {
            vec![places.get(self.selected).unwrap_or(first)]
        } else {
            places.iter().collect()
        };
        let min_lat = shown.iter().map(|p| p.lat).fold(f32::MAX, f32::min);
        let max_lat = shown.iter().map(|p| p.lat).fold(f32::MIN, f32::max);
        let min_lng = shown.iter().map(|p| p.lng).fold(f32::MAX, f32::min);
        let max_lng = shown.iter().map(|p| p.lng).fold(f32::MIN, f32::max);

        let (width, height) = (width.max(1.0), height.max(1.0));
        // A pin's balloon stands above its coordinate, so the band's top
        // leaves room for it under the status bar.
        let band_top = self.host.safe_top.max(20.0) + PIN_HEIGHT + 8.0;
        let band_bottom = (height - CAROUSEL_BOTTOM - CARD_HEIGHT - 12.0).max(band_top + 40.0);
        let band = band_bottom - band_top;

        // Mercator: a degree of latitude is 1/cos(lat) times taller on
        // screen than a degree of longitude is wide.
        let mid_lat = (min_lat + max_lat) / 2.0;
        let cos = mid_lat.to_radians().cos().max(0.1);
        // One pin (a focused card) zooms in to its street; several keep a
        // neighbourhood-wide minimum so close pins don't crowd.
        let min_span = if shown.len() == 1 { FOCUS_MIN_SPAN } else { OVERVIEW_MIN_SPAN };
        let lat_span = (max_lat - min_lat).max(min_span);
        let lng_span = (max_lng - min_lng).max(min_span);
        // Degrees of latitude per screen point for the pins to fit the band
        // (with a margin), and for their width to fit the screen.
        let per_point = (lat_span * 1.2 / band).max(lng_span * cos * 1.2 / (width - 48.0));
        let latitude_delta = per_point * height;
        let longitude_delta = latitude_delta / cos * width / height;
        // Put the pins' midpoint at the band's middle, above screen center.
        let band_mid = (band_top + band_bottom) / 2.0;
        let latitude = mid_lat - (height / 2.0 - band_mid) * per_point;
        let longitude = (min_lng + max_lng) / 2.0;
        (latitude, longitude, latitude_delta, longitude_delta)
    }

    /// Map pins as data; `views/map.view` (MapPin) decides how they look.
    /// The selected pin comes last so it draws on top.
    pub(crate) fn pins(&self) -> Vec<serde_json::Value> {
        let results = self.has_results();
        let mut pins: Vec<(bool, serde_json::Value)> = self
            .visible_places()
            .iter()
            .enumerate()
            .map(|(index, place)| {
                let selected = results && index == self.selected;
                let state = match (results, selected) {
                    (false, _) => "static",
                    (true, true) => "selected",
                    (true, false) => "idle",
                };
                (
                    selected,
                    serde_json::json!({
                        "id": place.id,
                        "lat": place.lat,
                        "lng": place.lng,
                        "state": state,
                        "icon": place.icon,
                        "icon_color": place.icon_color,
                    }),
                )
            })
            .collect();
        pins.sort_by_key(|(selected, _)| *selected);
        pins.into_iter().map(|(_, pin)| pin).collect()
    }
}
