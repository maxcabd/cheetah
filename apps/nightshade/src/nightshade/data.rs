//! Places, sample data, and the nightshadex search backend.

use super::*;

/// Backend base URL baked in at build time, e.g.
/// `NIGHTSHADE_API_URL=http://10.0.2.2:8000` for the Android emulator.
/// Unset, Nightshade searches its local sample places.
pub(super) const API_URL: Option<&str> = option_env!("NIGHTSHADE_API_URL");
pub(super) const SEARCH_TIMEOUT_MS: u32 = 60_000;
/// The backend only has Ottawa data for now. The device location is sent as
/// the search center only when it is in the Ottawa area; elsewhere (or on an
/// emulator's default location) the backend's own Ottawa default is used.
pub(super) const MARKET_CENTER: (f32, f32) = (45.4215, -75.6972);
pub(super) const MARKET_RADIUS_KM: f32 = 60.0;

/// Great-circle distance in kilometres.
pub(super) fn distance_km((lat1, lng1): (f32, f32), (lat2, lng2): (f32, f32)) -> f32 {
    let (lat1, lng1, lat2, lng2) = (
        lat1.to_radians(),
        lng1.to_radians(),
        lat2.to_radians(),
        lng2.to_radians(),
    );
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lng2 - lng1) / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().min(1.0).asin()
}

/// `POST /v1/search` body: the query, plus the device location when usable.
pub(super) fn search_body(query: &str, center: Option<(f32, f32)>) -> serde_json::Value {
    let mut body = serde_json::json!({ "text": query });
    if let Some((lat, lng)) =
        center.filter(|&point| distance_km(point, MARKET_CENTER) <= MARKET_RADIUS_KM)
    {
        body["lat"] = lat.into();
        body["lng"] = lng.into();
    }
    body
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Place {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) address: String,
    pub(super) lat: f32,
    pub(super) lng: f32,
    pub(super) distance_m: f32,
    pub(super) price_tier: u8,
    /// `None` when the backend does not know the hours.
    pub(super) open: Option<bool>,
    pub(super) closes: String,
    pub(super) opens: String,
    /// Empty when there is no rating to show.
    pub(super) rating: String,
    pub(super) reviews: String,
    pub(super) cuisine: String,
    pub(super) icon: String,
    pub(super) icon_color: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sample(
    id: &str,
    name: &str,
    address: &str,
    (lat, lng): (f32, f32),
    distance_m: f32,
    price_tier: u8,
    open: bool,
    (opens, closes): (&str, &str),
    cuisine: &str,
    (icon, icon_color): (&str, &str),
) -> Place {
    Place {
        id: id.into(),
        name: name.into(),
        address: address.into(),
        lat,
        lng,
        distance_m,
        price_tier,
        open: Some(open),
        closes: closes.into(),
        opens: opens.into(),
        rating: "4.3".into(),
        reviews: "50+".into(),
        cuisine: cuisine.into(),
        icon: icon.into(),
        icon_color: icon_color.into(),
    }
}

pub(super) fn local_places() -> Vec<Place> {
    vec![
        sample(
            "java",
            "Java Haven",
            "Cedarwood Drive",
            (37.7793, -122.4192),
            280.0,
            2,
            true,
            ("7:00 AM", "6:00 PM"),
            "Coffee",
            ("coffee", "#7B3F00"),
        ),
        sample(
            "pizza",
            "Pizza Paradise",
            "Market Street",
            (37.7849, -122.4094),
            1_200.0,
            2,
            true,
            ("11:00 AM", "11:00 PM"),
            "Pizza",
            ("pizza-slice", "#FFA500"),
        ),
        sample(
            "sushi",
            "Sushi Zen",
            "Hayes Valley",
            (37.7694, -122.4862),
            700.0,
            3,
            true,
            ("11:30 AM", "9:30 PM"),
            "Sushi",
            ("fish", "#008080"),
        ),
        sample(
            "burger",
            "Burger Joint",
            "Mission Street",
            (37.7599, -122.4148),
            520.0,
            1,
            false,
            ("10:30 AM", "10:00 PM"),
            "Fast Food",
            ("hamburger", "#FF4500"),
        ),
    ]
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SearchResponse {
    pub(super) places: Vec<Place>,
    pub(super) followups: Vec<String>,
}

pub(super) trait PlaceSource {
    fn search(&self, query: &str) -> Result<SearchResponse, String>;
}

pub(super) struct LocalPlaceSource;

impl PlaceSource for LocalPlaceSource {
    fn search(&self, query: &str) -> Result<SearchResponse, String> {
        let lowered = query.to_lowercase();
        if lowered.contains("fail") {
            return Err("Place search is unavailable".into());
        }

        let places = local_places();
        let places = if lowered.contains("sushi") {
            vec![places[2].clone()]
        } else if lowered.contains("pizza") {
            vec![places[1].clone()]
        } else if lowered.contains("coffee")
            || lowered.contains("cafe")
            || lowered.contains("brunch")
        {
            vec![places[0].clone()]
        } else if lowered.contains("burger") || lowered.contains("fast") {
            vec![places[3].clone()]
        } else if lowered.contains("open") || lowered.contains("late") {
            places
                .into_iter()
                .filter(|place| place.open == Some(true))
                .collect()
        } else {
            places
        };

        Ok(SearchResponse {
            places,
            followups: vec![
                "Cafes within 12 min walk".into(),
                "Best dessert spots near me".into(),
                "Nearby spots that open late night".into(),
            ],
        })
    }
}

/// Where searches go. Remote searches are asynchronous: the request is
/// queued as an `http.request` native call and the result arrives as an
/// `http.response` event.
pub(super) enum Backend {
    Local(Box<dyn PlaceSource>),
    Remote {
        client: HttpClient,
        pending: Option<u64>,
    },
}

impl Backend {
    pub(super) fn remote(base_url: &str) -> Self {
        let mut client = HttpClient::new(base_url);
        client.set_timeout_ms(SEARCH_TIMEOUT_MS);
        Backend::Remote {
            client,
            pending: None,
        }
    }
}

/// `POST /v1/search` response (nightshadex backend). Only the fields the
/// map and cards use; everything is lenient because most are nullable.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct ApiSearchResponse {
    pub(super) cards: Vec<ApiCard>,
    pub(super) followups: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct ApiCard {
    pub(super) place_id: String,
    pub(super) name: String,
    pub(super) address: Option<String>,
    pub(super) lat: f32,
    pub(super) lng: f32,
    pub(super) distance_m: Option<f32>,
    pub(super) price_tier: Option<u8>,
    pub(super) open_now: Option<bool>,
    pub(super) closes_at_local: Option<String>,
}

impl ApiCard {
    pub(super) fn into_place(self) -> Place {
        Place {
            id: self.place_id,
            name: self.name,
            address: self.address.unwrap_or_default(),
            lat: self.lat,
            lng: self.lng,
            distance_m: self.distance_m.unwrap_or(0.0),
            price_tier: self.price_tier.unwrap_or(0),
            open: self.open_now,
            closes: self
                .closes_at_local
                .as_deref()
                .and_then(clock_time)
                .unwrap_or_default(),
            opens: String::new(),
            rating: String::new(),
            reviews: String::new(),
            cuisine: String::new(),
            icon: "utensils".into(),
            icon_color: "#ffffff".into(),
        }
    }
}

/// `"2026-11-15T22:30:00"` or `"22:30"` -> `"10:30 PM"`.
pub(super) fn clock_time(value: &str) -> Option<String> {
    let time = value.split('T').nth(1).unwrap_or(value);
    let mut parts = time.split(':');
    let hour: u32 = parts.next()?.trim().parse().ok()?;
    let minute: u32 = parts.next()?.get(..2)?.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let suffix = if hour < 12 { "AM" } else { "PM" };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    Some(format!("{hour}:{minute:02} {suffix}"))
}

pub(super) fn parse_search(response: &HttpResponse) -> Result<SearchResponse, String> {
    if !response.ok {
        return Err(match response.error.as_deref() {
            Some("timeout") => "Search timed out".into(),
            _ => "Can't reach Nightshade".into(),
        });
    }
    if !response.is_success() {
        return Err(format!("Search failed ({})", response.status));
    }
    let body: ApiSearchResponse = response
        .json()
        .map_err(|_| "Unexpected search response".to_string())?;
    Ok(SearchResponse {
        places: body.cards.into_iter().map(ApiCard::into_place).collect(),
        followups: body.followups,
    })
}
