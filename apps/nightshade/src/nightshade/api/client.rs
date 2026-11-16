//! The nightshadex search backend: requests and response parsing.

use crate::nightshade::*;

/// Backend base URL baked in at build time, e.g.
/// `NIGHTSHADE_API_URL=http://10.0.2.2:8000` for the Android emulator.
/// Unset, Nightshade searches its local sample places.
pub(crate) const API_URL: Option<&str> = option_env!("NIGHTSHADE_API_URL");
pub(crate) const SEARCH_TIMEOUT_MS: u32 = 60_000;
/// The backend only has Ottawa data for now. The device location is sent as
/// the search center only when it is in the Ottawa area; elsewhere (or on an
/// emulator's default location) the backend's own Ottawa default is used.
pub(crate) const MARKET_CENTER: (f32, f32) = (45.4215, -75.6972);
pub(crate) const MARKET_RADIUS_KM: f32 = 60.0;

/// Great-circle distance in kilometres.
pub(crate) fn distance_km((lat1, lng1): (f32, f32), (lat2, lng2): (f32, f32)) -> f32 {
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
pub(crate) fn search_body(query: &str, center: Option<(f32, f32)>) -> serde_json::Value {
    let mut body = serde_json::json!({ "text": query });
    if let Some((lat, lng)) =
        center.filter(|&point| distance_km(point, MARKET_CENTER) <= MARKET_RADIUS_KM)
    {
        body["lat"] = lat.into();
        body["lng"] = lng.into();
    }
    body
}

/// Where searches go. Remote searches are asynchronous: the request is
/// queued as an `http.request` native call and the result arrives as an
/// `http.response` event.
pub(crate) enum Backend {
    Local(Box<dyn PlaceSource>),
    Remote {
        client: HttpClient,
        pending: Option<u64>,
    },
}

impl Backend {
    pub(crate) fn remote(base_url: &str) -> Self {
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
pub(crate) struct ApiSearchResponse {
    pub(crate) cards: Vec<ApiCard>,
    pub(crate) followups: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ApiCard {
    pub(crate) place_id: String,
    pub(crate) name: String,
    pub(crate) address: Option<String>,
    pub(crate) lat: f32,
    pub(crate) lng: f32,
    pub(crate) distance_m: Option<f32>,
    pub(crate) price_tier: Option<u8>,
    pub(crate) open_now: Option<bool>,
    pub(crate) closes_at_local: Option<String>,
}

impl ApiCard {
    pub(crate) fn into_place(self) -> Place {
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
pub(crate) fn clock_time(value: &str) -> Option<String> {
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

pub(crate) fn parse_search(response: &HttpResponse) -> Result<SearchResponse, String> {
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
