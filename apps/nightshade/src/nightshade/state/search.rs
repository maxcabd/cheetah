//! Running searches and handling native results.

use crate::nightshade::*;

impl NightshadeApp {
    pub(crate) fn run_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }

        // A search typed or tapped over results is a follow-up: it refines
        // the shown results ("with seating") rather than starting over.
        let previous_plan = self.response().and_then(|response| response.plan.clone());
        self.search = Resource::Loading;
        self.query = query.into();
        self.draft.clear();
        self.start_searching_animation();
        let result = match &mut self.backend {
            Backend::Local(source) => source.search(query),
            Backend::Remote { client, pending } => {
                // A newer search supersedes any in-flight one.
                if let Some(previous) = pending.take() {
                    self.calls.push(crate::http::cancel(previous));
                }
                let mut body = search_body(query, self.center);
                if let Some(plan) = previous_plan {
                    body["previous_plan"] = plan;
                }
                let request = client.post("/v1/search").json(&body);
                *pending = Some(request.id);
                self.calls.push(request.into_call());
                return;
            }
        };
        self.finish_search(result);
    }

    pub(crate) fn start_searching_animation(&mut self) {
        self.searching_frame = 0;
        if !self.searching_timer {
            self.searching_timer = true;
            self.actions.push(Action::Timer {
                name: "searching".into(),
                ms: SEARCHING_FRAME_MS,
            });
        }
    }

    /// Next emoji while the search runs; stops once it finishes.
    pub(crate) fn searching_tick(&mut self) {
        if !self.search.is_loading() {
            self.searching_timer = false;
            return;
        }
        self.searching_frame = (self.searching_frame + 1) % SEARCHING_EMOJIS.len();
        self.actions.push(Action::Timer {
            name: "searching".into(),
            ms: SEARCHING_FRAME_MS,
        });
    }

    pub(crate) fn minimize_cards(&mut self, minimized: bool) {
        if self.cards_minimized != minimized {
            self.cards_minimized = minimized;
            self.haptic("selection");
        }
    }

    pub(crate) fn finish_search(&mut self, result: Result<SearchResponse, String>) {
        match result {
            Ok(response) => {
                // New results start with full cards, framed on the map.
                self.cards_minimized = false;
                self.locating = false;
                self.search = Resource::Ready(response);
                self.selected = 0;
                self.focused = false;
                self.expanded = false;
                self.haptic("light");
            }
            Err(error) => {
                self.search = Resource::Failed(error);
                self.selected = 0;
                self.focused = false;
                self.expanded = false;
            }
        }
    }

    pub(crate) fn clear_search(&mut self) {
        if let Backend::Remote { pending, .. } = &mut self.backend {
            if let Some(id) = pending.take() {
                self.calls.push(crate::http::cancel(id));
            }
        }
        self.query.clear();
        self.draft.clear();
        self.search = Resource::Idle;
        self.selected = 0;
        self.focused = false;
        self.expanded = false;
    }

    pub(crate) fn select_place(&mut self, id: &str) {
        let Some(response) = self.response() else {
            return;
        };
        if let Some(index) = response.places.iter().position(|place| place.id == id) {
            self.selected = index;
            self.focused = true;
            self.locating = false;
            self.haptic("selection");
        }
    }

    pub(crate) fn haptic(&mut self, style: &str) {
        self.calls.push(NativeCall::new("haptics", style, "{}"));
    }

    pub(crate) fn handle_native_event(&mut self, payload: &str) {
        let Some(event) = NativeEvent::parse(payload) else {
            return;
        };

        if let Some(response) = HttpResponse::from_event(&event) {
            let Backend::Remote { pending, .. } = &mut self.backend else {
                return;
            };
            // Ignore cancelled or superseded requests.
            if *pending != Some(response.id) {
                return;
            }
            *pending = None;
            self.finish_search(parse_search(&response));
            return;
        }

        if !event.is("location", "result") || event.data["ok"] != true {
            return;
        }
        let latitude = event.data["latitude"].as_f64();
        let longitude = event.data["longitude"].as_f64();
        if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
            self.center = Some((latitude as f32, longitude as f32));
        }
    }
}
