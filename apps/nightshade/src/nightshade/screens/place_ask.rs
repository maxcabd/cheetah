//! A place page's ask bar ("do they have vegan options?"), answered by the
//! backend from what it knows about the place (`POST /v1/search/ask`), and
//! its call and website buttons.

use crate::http::HttpResponse;
use crate::nightshade::*;
use serde_json::{json, Value};

/// A question about a place, and its answer once back.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlaceAsk {
    pub(crate) place_id: String,
    pub(crate) question: String,
    /// `None` while the backend thinks.
    pub(crate) answer: Option<String>,
    pub(crate) failed: bool,
}

impl NightshadeApp {
    /// Ask the open place's question.
    pub(crate) fn send_ask(&mut self) {
        let question = self.ask_draft.trim().to_string();
        let Some(place_id) = self.open_place_id() else { return };
        if question.is_empty() {
            return;
        }
        self.ask_draft.clear();
        // Remounting the field dismisses the keyboard.
        self.ask_mount += 1;
        let Backend::Remote { client, .. } = &mut self.backend else {
            self.ask = Some(PlaceAsk {
                place_id,
                question,
                answer: Some("Asking about a place needs Nightshade's server.".into()),
                failed: true,
            });
            return;
        };
        let request = client.post("/v1/search/ask").json(&json!({ "place_id": place_id, "question": question }));
        self.ask_request = Some(request.id);
        self.calls.push(request.into_call());
        self.ask = Some(PlaceAsk { place_id, question, answer: None, failed: false });
        self.haptic("light");
    }

    /// The backend's answer, or why there isn't one.
    pub(crate) fn ask_answered(&mut self, response: &HttpResponse) {
        let Some(ask) = &mut self.ask else { return };
        let answer = response
            .is_success()
            .then(|| response.json::<Value>().ok())
            .flatten()
            .and_then(|body| body["answer"].as_str().map(str::to_string))
            .filter(|text| !text.trim().is_empty());
        match answer {
            Some(text) => ask.answer = Some(text),
            None => {
                ask.failed = true;
                ask.answer = Some(match response.status {
                    429 => "That's a lot of questions; try again in a few minutes.".into(),
                    _ => "Couldn't answer that right now. Try again in a moment.".into(),
                });
            }
        }
    }

    /// The page's call or website button.
    pub(crate) fn open_contact(&mut self, key: &str) {
        let Some(place) = self.open_place_id().and_then(|id| self.place(&id).cloned()) else { return };
        let url = if key == "page:call" {
            let digits: String = place.phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
            (!digits.is_empty()).then(|| format!("tel:{digits}"))
        } else {
            let site = place.website.trim();
            (!site.is_empty()).then(|| {
                if site.starts_with("http://") || site.starts_with("https://") {
                    site.to_string()
                } else {
                    format!("https://{site}")
                }
            })
        };
        if let Some(url) = url {
            self.calls.push(NativeCall::new("system", "openUrl", &json!({ "url": url }).to_string()));
            self.haptic("light");
        }
    }

    /// The page's ask bar and overview card, for `place`.
    pub(crate) fn ask_model(&self, place: &Place) -> Value {
        let ask = self.ask.as_ref().filter(|ask| ask.place_id == place.id);
        let state = match ask {
            None => "none",
            Some(PlaceAsk { answer: None, .. }) => "thinking",
            Some(PlaceAsk { failed: true, .. }) => "failed",
            Some(_) => "answered",
        };
        json!({
            "state": state,
            "question": ask.map(|ask| ask.question.as_str()).unwrap_or_default(),
            "answer": ask.and_then(|ask| ask.answer.as_deref()).unwrap_or_default(),
            "draft": self.ask_draft,
            "can_send": !self.ask_draft.trim().is_empty(),
            "placeholder": format!("Ask about {}…", place.name),
            "field_key": format!("page-ask:{}", self.ask_mount),
            // Above the keyboard when it's up.
            "bottom": self.host.safe_bottom.max(self.host.keyboard_inset) + 12.0,
        })
    }
}
