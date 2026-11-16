//! Bottom sheet: search bar, suggestions, results list.

use super::*;

impl NightshadeApp {
    pub(super) fn sheet_height(&self, context: &ViewContext) -> f32 {
        if self.expanded {
            let fraction = if self.host.keyboard_inset > 0.0 {
                0.92
            } else {
                0.85
            };
            (context.viewport.height * fraction).max(520.0)
        } else if self.has_results() {
            RESULT_SHEET_HEIGHT
        } else {
            (context.viewport.height * 0.52).max(390.0)
        }
    }

    pub(super) fn search_bar(&self, _context: &ViewContext) -> Element {
        let results = self.has_results();
        let placeholder = if results {
            "Ask a follow up"
        } else {
            "What are you in the mood for?"
        };

        let mut field = row()
            .weight(1.0)
            .height(SEARCH_HEIGHT)
            .bg(CHIP_BG)
            .border(1.0, "#2d2d2d")
            .radius(36.0)
            .align_items("center")
            .child(
                input(placeholder, self.draft.clone())
                    .key("search")
                    .weight(1.0)
                    .height(64.0)
                    .size(16.0)
                    .font("satoshi-medium")
                    .color("#ffffff")
                    .placeholder_color("#737373")
                    .pad_h(32.0)
                    .align("start"),
            );

        if !self.draft.trim().is_empty() {
            field = field.child(Self::action_button(
                "arrow-up",
                "search:send",
                "#dc2626",
                40.0,
            ));
        }

        let mut bar = row()
            .width(-1.0)
            .height(SEARCH_HEIGHT)
            .gap(12.0)
            .align_items("center")
            .child(field);

        if results {
            bar = bar
                .child(Self::action_button("xmark", "search:clear", CHIP_BG, 48.0).label("Clear"));
        }

        column()
            .width(-1.0)
            .height(SEARCH_HEIGHT)
            .pad_h(24.0)
            .justify("end")
            .test_id("cheetah-search-bar")
            .child(bar)
    }

    pub(super) fn empty_body(&self) -> Element {
        if let Some(cached) = self.suggestion_cache.borrow().as_ref() {
            return cached.clone();
        }

        let mut suggestions = row()
            .width(600.0)
            .wrap(true)
            .gap(10.0)
            .align_items("center");
        for (index, suggestion) in SUGGESTIONS.iter().enumerate() {
            suggestions = suggestions.child(Self::chip(suggestion, &format!("suggest:{index}")));
        }

        let grid = column()
            .width(-1.0)
            .weight(1.0)
            .pad_h(24.0)
            .gap(14.0)
            .child(
                text("Ready to discover?")
                    .size(20.0)
                    .font("brice-semibold")
                    .line_height(26.0)
                    .color(TEXT)
                    .height(26.0),
            )
            .child(
                hscroll()
                    .width(-1.0)
                    .height(260.0)
                    .content_width(600.0)
                    .child(column().width(600.0).height(260.0).child(suggestions)),
            );
        *self.suggestion_cache.borrow_mut() = Some(grid.clone());
        grid
    }

    pub(super) fn results_body(&self) -> Element {
        let mut body = column()
            .width(-1.0)
            .weight(1.0)
            .pad_h(24.0)
            .gap(8.0)
            .child(
                text(self.query.clone())
                    .size(20.0)
                    .font("brice-semibold")
                    .line_height(26.0)
                    .color(TEXT)
                    .height(26.0),
            )
            .child(
                text("Alright, here's what I'd check out if I were you.")
                    .size(16.0)
                    .font("satoshi-medium")
                    .line_height(20.0)
                    .color(MUTED)
                    .height(20.0),
            )
            .child(
                row()
                    .key("show-on-map")
                    .pressable(true)
                    .width(154.0)
                    .height(40.0)
                    .pad_h(16.0)
                    .gap(10.0)
                    .radius(18.0)
                    .bg(CHIP_BG)
                    .border(1.0, "#ffffff14")
                    .align_items("center")
                    .child(
                        text("Show on map")
                            .size(14.0)
                            .font("satoshi-medium")
                            .color("#ffffff")
                            .width(96.0)
                            .height(18.0),
                    )
                    .child(Self::icon("chevron-right", 12.0, "#ffffff")),
            );

        if self.expanded {
            if let Some(response) = self.response() {
                let mut followups = row().width(-1.0).wrap(true).gap(10.0);
                for (index, followup) in response.followups.iter().enumerate() {
                    followups = followups.child(Self::chip(followup, &format!("followup:{index}")));
                }
                body = body.child(followups);
            }
        }

        body
    }

    pub(super) fn searching_body(&self) -> Element {
        column()
            .width(-1.0)
            .weight(1.0)
            .pad_h(24.0)
            .gap(10.0)
            .child(
                text(self.query.clone())
                    .size(20.0)
                    .font("brice-semibold")
                    .color("#ffffff")
                    .height(26.0),
            )
            .child(text("Searching...").size(16.0).height(24.0))
    }

    pub(super) fn sheet(&self, context: &ViewContext) -> Element {
        let height = self.sheet_height(context);
        let body_height = (height - 24.0 - SEARCH_HEIGHT - SEARCH_BOTTOM_GAP).max(0.0);
        let body = if self.has_results() && !self.expanded {
            column().width(-1.0).height(body_height)
        } else {
            match &self.search {
                Resource::Ready(_) => self.results_body(),
                Resource::Loading => self.searching_body(),
                _ => self.empty_body(),
            }
        };

        sheet()
            .key("search-panel")
            .absolute()
            .bottom(0.0)
            .left(0.0)
            .width(-1.0)
            .height(height)
            .radius(54.0)
            .clip(true)
            .bg(SHEET_BG)
            .child(
                column()
                    .width(-1.0)
                    .height(height)
                    .child(
                        row()
                            .key("sheet:toggle")
                            .pressable(true)
                            .width(-1.0)
                            .height(24.0)
                            .justify("center")
                            .align_items("center")
                            .child(row().width(30.0).height(6.0).bg("#404040").radius(3.0)),
                    )
                    .child(
                        column()
                            .width(-1.0)
                            .height(body_height)
                            .clip(true)
                            .child(body),
                    )
                    .child(self.search_bar(context))
                    .child(column().width(-1.0).height(SEARCH_BOTTOM_GAP)),
            )
    }
}
