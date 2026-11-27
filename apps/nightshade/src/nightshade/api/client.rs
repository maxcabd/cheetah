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
/// Results shown to someone outside the market.
pub(crate) const MARKET_NOTICE: &str =
    "Nightshade only covers Ottawa for now, so these are Ottawa spots.";

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
    pub(crate) plan: ApiPlan,
    pub(crate) notice: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ApiPlan {
    pub(crate) time_context: ApiTimeContext,
}

/// The search's local time, e.g. `"2026-11-17T10:40:00.27-05:00"`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ApiTimeContext {
    pub(crate) local_datetime_iso: String,
}

/// One opening interval, `"06:00"` to `"20:00"` (an end past midnight may
/// read `"02:00"`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ApiInterval {
    pub(crate) start: String,
    pub(crate) end: String,
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
    /// Google rating, 1-5, when the backend has the place on Google.
    pub(crate) rating: Option<f32>,
    pub(crate) user_rating_count: Option<u32>,
    /// Weekly hours keyed `mon` .. `sun`; days without an entry are closed.
    pub(crate) hours: Option<std::collections::HashMap<String, Vec<ApiInterval>>>,
    /// The card's photos, hero first; paths starting with "/" are on the API.
    pub(crate) photos: Vec<String>,
    pub(crate) amenities: Vec<String>,
    /// Open only through the drive-through (the dining room is closed).
    pub(crate) drive_thru_only: bool,
    /// Who took each photo, lined up with `photos`.
    pub(crate) photo_credits: Vec<PhotoCredit>,
    /// Google says it's closed for now, or for good (a saved place).
    pub(crate) closed_temporarily: bool,
    pub(crate) closed_permanently: bool,
}

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// `"2026-11-17T10:40:00-05:00"` -> (weekday index, Monday = 0; minutes
/// since midnight).
pub(crate) fn local_day_minute(iso: &str) -> Option<(usize, u32)> {
    let (date, time) = iso.split_once('T')?;
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: i32 = parts.next()?.parse().ok()?;
    let day: i32 = parts.next()?.get(..2)?.parse().ok()?;
    let minute = minutes(time)?;
    // Sakamoto's method: 0 = Sunday.
    const OFFSETS: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let sunday_first =
        (y + y / 4 - y / 100 + y / 400 + OFFSETS[(month - 1) as usize] + day).rem_euclid(7);
    Some((((sunday_first + 6) % 7) as usize, minute))
}

/// `"06:30"` (seconds or an offset may follow) -> minutes since midnight.
fn minutes(time: &str) -> Option<u32> {
    let hour: u32 = time.get(..2)?.parse().ok()?;
    let minute: u32 = time.get(3..5)?.parse().ok()?;
    (hour <= 24 && minute < 60).then_some(hour * 60 + minute)
}

impl ApiCard {
    /// When a closed place next opens: "8:00 AM", "tomorrow at 8:00 AM" or "Wed at 9:00 AM" when it
    /// is not today. Empty without hours or a search time.
    fn next_opening(&self, now: Option<(usize, u32)>) -> String {
        let (Some(hours), Some((today, minute))) = (&self.hours, now) else {
            return String::new();
        };
        for ahead in 0..7 {
            let day = (today + ahead) % 7;
            let Some(intervals) = hours.get(DAYS[day]) else {
                continue;
            };
            let mut starts: Vec<u32> = intervals
                .iter()
                .filter_map(|interval| minutes(&interval.start))
                .filter(|&start| ahead > 0 || start > minute)
                .collect();
            starts.sort_unstable();
            if let Some(start) = starts.first() {
                let time = clock_time(&format!("{:02}:{:02}", start / 60, start % 60))
                    .unwrap_or_default();
                return match ahead {
                    0 => time,
                    1 => format!("tomorrow at {time}"),
                    _ => {
                        let name = DAYS[day];
                        format!("{}{} at {time}", name[..1].to_uppercase(), &name[1..])
                    }
                };
            }
        }
        String::new()
    }

    /// When an open place closes, from today's hours, for cards whose
    /// `closes_at_local` is missing.
    fn closing_today(&self, now: Option<(usize, u32)>) -> Option<String> {
        let (hours, (today, minute)) = (self.hours.as_ref()?, now?);
        hours.get(DAYS[today])?.iter().find_map(|interval| {
            let (start, end) = (minutes(&interval.start)?, minutes(&interval.end)?);
            // An end at or before the start runs past midnight.
            let open = if end > start {
                (start..end).contains(&minute)
            } else {
                minute >= start
            };
            open.then(|| clock_time(&interval.end)).flatten()
        })
    }

    /// Whether today's hours run midnight to midnight.
    fn open_all_day(&self, now: Option<(usize, u32)>) -> bool {
        let (Some(hours), Some((today, _))) = (self.hours.as_ref(), now) else {
            return false;
        };
        hours.get(DAYS[today]).is_some_and(|intervals| {
            intervals.iter().any(|interval| {
                minutes(&interval.start) == Some(0) && matches!(interval.end.as_str(), "24:00" | "23:59")
            })
        })
    }

    pub(crate) fn into_place(self, now: Option<(usize, u32)>) -> Place {
        let all_day = self.open_now == Some(true) && self.open_all_day(now);
        let opens = if self.open_now == Some(false) {
            self.next_opening(now)
        } else {
            String::new()
        };
        let closes = if all_day {
            String::new()
        } else {
            self.closes_at_local
                .as_deref()
                .and_then(clock_time)
                .or_else(|| self.closing_today(now))
                .unwrap_or_default()
        };
        Place {
            id: self.place_id,
            name: self.name,
            address: self.address.unwrap_or_default(),
            lat: self.lat,
            lng: self.lng,
            distance_m: self.distance_m.unwrap_or(0.0),
            price_tier: self.price_tier.unwrap_or(0),
            open: self.open_now,
            drive_thru_only: self.drive_thru_only && self.open_now == Some(true),
            closes,
            opens,
            rating: self
                .rating
                .map(|rating| format!("{rating:.1}"))
                .unwrap_or_default(),
            reviews: self
                .rating
                .and(self.user_rating_count)
                .map(review_count)
                .unwrap_or_default(),
            cuisine: String::new(),
            icon: "utensils".into(),
            icon_color: "#ffffff".into(),
            photos: self.photos.iter().map(|photo| resolve_url(photo)).collect(),
            amenities: self.amenities,
            photo_credits: self.photo_credits,
            hours_current: false,
            all_day,
            closure: if self.closed_permanently {
                "permanently".into()
            } else if self.closed_temporarily {
                "temporarily".into()
            } else {
                String::new()
            },
        }
    }
}

/// 812 -> "812", 1234 -> "1.2k", 25300 -> "25k".
pub(crate) fn review_count(count: u32) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=9_999 => format!("{:.1}k", count as f32 / 1000.0).replace(".0k", "k"),
        _ => format!("{}k", count / 1000),
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
    let raw: serde_json::Value = response
        .json()
        .map_err(|_| "Unexpected search response".to_string())?;
    let plan = raw.get("plan").filter(|plan| plan.is_object()).cloned();
    let body: ApiSearchResponse =
        serde_json::from_value(raw).map_err(|_| "Unexpected search response".to_string())?;
    let now = local_day_minute(&body.plan.time_context.local_datetime_iso);
    Ok(SearchResponse {
        places: body.cards.into_iter().map(|card| card.into_place(now)).collect(),
        followups: body.followups,
        plan,
        notice: body.notice.filter(|notice| !notice.trim().is_empty()),
    })
}

/// A URL from the API, absolute: "/photos/x.jpg" -> "<API_URL>/photos/x.jpg".
pub(crate) fn resolve_url(url: &str) -> String {
    match API_URL {
        Some(base) if url.starts_with('/') => format!("{}{url}", base.trim_end_matches('/')),
        _ => url.to_string(),
    }
}
