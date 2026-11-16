//! Place models, sample data, and the offline place source.

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) lat: f32,
    pub(crate) lng: f32,
    pub(crate) distance_m: f32,
    pub(crate) price_tier: u8,
    /// `None` when the backend does not know the hours.
    pub(crate) open: Option<bool>,
    pub(crate) closes: String,
    pub(crate) opens: String,
    /// Empty when there is no rating to show.
    pub(crate) rating: String,
    pub(crate) reviews: String,
    pub(crate) cuisine: String,
    pub(crate) icon: String,
    pub(crate) icon_color: String,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sample(
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

pub(crate) fn local_places() -> Vec<Place> {
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
pub(crate) struct SearchResponse {
    pub(crate) places: Vec<Place>,
    pub(crate) followups: Vec<String>,
}

pub(crate) trait PlaceSource {
    fn search(&self, query: &str) -> Result<SearchResponse, String>;
}

pub(crate) struct LocalPlaceSource;

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
