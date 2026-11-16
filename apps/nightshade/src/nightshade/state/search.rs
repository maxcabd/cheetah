//! Running searches and handling native results.

use crate::nightshade::*;

impl NightshadeApp {
    pub(crate) fn run_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }

        self.search = Resource::Loading;
        self.query = query.into();
        self.draft.clear();
        let result = match &mut self.backend {
            Backend::Local(source) => source.search(query),
            Backend::Remote { client, pending } => {
                // A newer search supersedes any in-flight one.
                if let Some(previous) = pending.take() {
                    self.calls.push(crate::http::cancel(previous));
                }
                let request = client
                    .post("/v1/search")
                    .json(&search_body(query, self.center));
                *pending = Some(request.id);
                self.calls.push(request.into_call());
                return;
            }
        };
        self.finish_search(result);
    }

    pub(crate) fn finish_search(&mut self, result: Result<SearchResponse, String>) {
        match result {
            Ok(response) => {
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
