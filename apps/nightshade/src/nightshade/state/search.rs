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
        // After a search that found nothing, there's nothing to refine.
        let previous_plan = self
            .response()
            .filter(|response| !response.places.is_empty())
            .and_then(|response| response.plan.clone());
        if previous_plan.is_none() {
            self.remember_search(query);
        }
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
            Ok(response) if response.places.is_empty() => {
                // Nothing found: the sheet says so (`sheet_model`), with
                // other searches to try.
                self.drop_keyboard();
                self.search = Resource::Ready(response);
                self.cards_built.set(None);
                self.selected = 0;
                self.focused = false;
                self.expanded = false;
                self.haptic("warning");
            }
            Ok(mut response) => {
                // Away from Ottawa, results are Ottawa's: say so, unless the
                // backend has something to say.
                if self.outside_market() && response.notice.is_none() {
                    response.notice = Some(MARKET_NOTICE.into());
                }
                // New results start with full cards, the map zoomed in on
                // the first (as if its card was swiped to).
                self.cards_minimized = false;
                self.locating = false;
                // Results collapse the sheet; iOS won't shrink a sheet while
                // the keyboard is up, so drop it (remounting the field).
                if self.expanded {
                    self.field_mount += 1;
                }
                self.search = Resource::Ready(response);
                self.refresh_saved();
                self.cards_built.set(None);
                self.selected = 0;
                self.focused = true;
                self.expanded = false;
                self.haptic("light");
                self.build_ahead();
            }
            Err(error) => {
                // The sheet shows what went wrong, with a way to try again.
                self.drop_keyboard();
                self.search = Resource::Failed(error);
                self.selected = 0;
                self.focused = false;
                self.expanded = false;
                self.haptic("error");
            }
        }
    }

    /// The sheet is about to shrink; iOS won't shrink it while the keyboard
    /// is up, so drop it (remounting the field).
    fn drop_keyboard(&mut self) {
        if self.expanded {
            self.field_mount += 1;
        }
    }

    /// "Try again" after a failed search: the same search.
    pub(crate) fn retry_search(&mut self) {
        if self.search.error().is_some() {
            let query = self.query.clone();
            self.run_search(&query);
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
        // Fresh suggestions for the empty sheet.
        self.load_suggestions();
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
            self.build_ahead();
        }
    }

    pub(crate) fn haptic(&mut self, style: &str) {
        self.calls.push(NativeCall::new("haptics", style, "{}"));
    }

    pub(crate) fn handle_native_event(&mut self, payload: &str) {
        let Some(event) = NativeEvent::parse(payload) else {
            return;
        };
        // The host's back gesture closes the open page.
        if self.router.handle_native(&event) {
            return;
        }
        // Settings, picked when location is off.
        if event.is("dialog", "result") {
            if event.data["id"] == "location:denied" && event.data["index"] == 1 {
                self.open_settings();
            }
            return;
        }

        if let Some(response) = HttpResponse::from_event(&event) {
            if self.suggestions_request == Some(response.id) {
                self.suggestions_request = None;
                if let Ok(data) = response.json::<serde_json::Value>() {
                    let list = |key: &str| -> Vec<String> {
                        data[key]
                            .as_array()
                            .map(|items| items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect())
                            .unwrap_or_default()
                    };
                    self.trending = list("trending");
                    self.time_of_day = list("time_of_day");
                }
                return;
            }
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

        if !event.is("location", "result") {
            return;
        }
        if event.data["ok"] != true {
            // Turned off for Nightshade: the locate button says so. Other
            // failures (no fix yet) ask again on the next tap.
            if event.data["error"] == "permission_denied" {
                self.location_denied = true;
                self.locating = false;
            }
            return;
        }
        self.location_denied = false;
        let latitude = event.data["latitude"].as_f64();
        let longitude = event.data["longitude"].as_f64();
        if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
            self.center = Some((latitude as f32, longitude as f32));
        }
    }
}
