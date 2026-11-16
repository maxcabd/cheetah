//! The map screen: map with markers, result cards, and the search sheet.

use crate::nightshade::*;

const INITIAL_LAT: f32 = 37.7749;
const INITIAL_LNG: f32 = -122.4194;

impl NightshadeApp {
    /// Full-screen map, result cards over it, and the search sheet.
    pub(crate) fn map_screen(&self, context: &ViewContext) -> Element {
        let (latitude, longitude, latitude_delta, longitude_delta) = self.region();
        let mut root = column().width(-1.0).height(-1.0).bg(BG).child(
            map()
                .key("map")
                .width(-1.0)
                .height(-1.0)
                .bg("#101826")
                .test_id("nightshade-map")
                .map_region(latitude, longitude, latitude_delta, longitude_delta)
                .map_markers(self.markers())
                .map_dark(true)
                .map_shows_user(true),
        );

        if self.has_results() {
            root = root.child(self.carousel(context, self.visible_places()));
        }

        root.child(self.sheet(context))
    }

    pub(crate) fn visible_places(&self) -> &[Place] {
        self.response()
            .map(|response| response.places.as_slice())
            .unwrap_or(&self.sample_places)
    }

    pub(crate) fn region(&self) -> (f32, f32, f32, f32) {
        if !self.has_results() {
            let (latitude, longitude) = self.center.unwrap_or((INITIAL_LAT, INITIAL_LNG));
            return (latitude, longitude, 0.025, 0.025);
        }

        let places = self.visible_places();
        let Some(first) = places.first() else {
            return (INITIAL_LAT, INITIAL_LNG, 0.025, 0.025);
        };

        if self.focused || places.len() == 1 {
            let place = places.get(self.selected).unwrap_or(first);
            let delta = 0.018;
            return (place.lat - delta * 0.40, place.lng, delta, delta);
        }

        let mut min_lat = first.lat;
        let mut max_lat = first.lat;
        let mut min_lng = first.lng;
        let mut max_lng = first.lng;
        for place in places {
            min_lat = min_lat.min(place.lat);
            max_lat = max_lat.max(place.lat);
            min_lng = min_lng.min(place.lng);
            max_lng = max_lng.max(place.lng);
        }

        let latitude = (min_lat + max_lat) / 2.0;
        let longitude = (min_lng + max_lng) / 2.0;
        let latitude_delta = ((max_lat - min_lat) * 1.4).max(0.012);
        let longitude_delta = ((max_lng - min_lng) * 1.4).max(0.012);
        // Keep result pins above the card carousel and search bar.
        let latitude = latitude - latitude_delta * 0.32;
        (latitude, longitude, latitude_delta, longitude_delta)
    }

    pub(crate) fn markers(&self) -> String {
        let results = self.has_results();
        let items = self
            .visible_places()
            .iter()
            .enumerate()
            .map(|(index, place)| {
                serde_json::json!({
                    "id": place.id,
                    "name": place.name,
                    "lat": place.lat,
                    "lng": place.lng,
                    "kind": if results { "result" } else { "static" },
                    "icon": if results { "utensils" } else { place.icon.as_str() },
                    "iconColor": if results { "#ffffff" } else { place.icon_color.as_str() },
                    "selected": results && index == self.selected,
                })
            })
            .collect::<Vec<_>>();
        serde_json::Value::Array(items).to_string()
    }
}
