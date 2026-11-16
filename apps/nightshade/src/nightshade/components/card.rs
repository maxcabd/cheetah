//! Place card and the paged card carousel.

use crate::nightshade::*;

impl NightshadeApp {
    pub(crate) fn card(&self, width: f32, place: &Place, _selected: bool) -> Element {
        let cache_key = (place.id.clone(), width.to_bits());
        if let Some(cached) = self.card_cache.borrow().get(&cache_key) {
            return cached.clone();
        }
        let drive_minutes = ((place.distance_m / 1000.0) / 30.0 * 60.0).round().max(1.0) as u32;
        let distance = if place.distance_m < 1000.0 {
            format!("{}m", place.distance_m.round() as u32)
        } else {
            format!("{:.1}km", place.distance_m / 1000.0)
        };
        let price = "$".repeat(place.price_tier.min(4) as usize);
        let status = if place.open.is_none() {
            row().width(width - 40.0).height(18.0).child(
                text("Hours unavailable")
                    .size(14.0)
                    .font("satoshi-medium")
                    .color("#ffffffb3")
                    .width(160.0)
                    .height(18.0),
            )
        } else if place.open == Some(true) {
            row()
                .width(width - 40.0)
                .height(18.0)
                .gap(4.0)
                .align_items("center")
                .child(
                    text("Open")
                        .size(14.0)
                        .font("satoshi-bold")
                        .color(GREEN)
                        .width(38.0)
                        .height(18.0),
                )
                .child(
                    text("\u{00B7}")
                        .size(14.0)
                        .font("satoshi-medium")
                        .color("#ffffff")
                        .width(10.0)
                        .height(18.0),
                )
                .child(
                    text(if place.closes.is_empty() {
                        String::new()
                    } else {
                        format!("Closes at {}", place.closes)
                    })
                    .size(14.0)
                    .font("satoshi-medium")
                    .color("#ffffff")
                    .width(130.0)
                    .height(18.0),
                )
        } else {
            row()
                .width(width - 40.0)
                .height(18.0)
                .gap(4.0)
                .align_items("center")
                .child(
                    text("Closed")
                        .size(14.0)
                        .font("satoshi-bold")
                        .color(RED)
                        .width(48.0)
                        .height(18.0),
                )
                .child(
                    text("\u{00B7}")
                        .size(14.0)
                        .font("satoshi-medium")
                        .color("#ffffff")
                        .width(10.0)
                        .height(18.0),
                )
                .child(
                    text(if place.opens.is_empty() {
                        String::new()
                    } else {
                        format!("Opens at {}", place.opens)
                    })
                    .size(14.0)
                    .font("satoshi-medium")
                    .color("#ffffffb3")
                    .width(130.0)
                    .height(18.0),
                )
        };

        let top_controls = row()
            .absolute()
            .top(16.0)
            .left(16.0)
            .width(width - 32.0)
            .height(38.0)
            .justify("space-between")
            .align_items("center")
            .child(Self::glass_icon("chevron-up", "card:up"))
            .child(
                row()
                    .height(38.0)
                    .gap(8.0)
                    .align_items("center")
                    .child(Self::glass_icon("bookmark", "card:bookmark"))
                    .child(Self::glass_icon("share", "card:share"))
                    .child(Self::glass_icon("plus", "card:plus")),
            );

        let photo = native("photo")
            .absolute()
            .top(0.0)
            .left(0.0)
            .width(width)
            .height(CARD_HEADER)
            .native_props(r##"{"src":"asset://second-cup-card-full.jpeg","pages":5}"##);

        let card = column()
            .key(format!("place-{}", place.id))
            .pressable(true)
            .animate(true)
            .width(width)
            .height(CARD_HEIGHT)
            .bg(CARD_BG)
            .radius(28.0)
            .clip(true)
            .border(2.0, "#ffffff08")
            .child(
                column()
                    .width(width)
                    .height(CARD_HEADER)
                    .justify("end")
                    .pad_bottom(12.0)
                    .child(photo)
                    .child(top_controls),
            )
            .child(
                column()
                    .width(width)
                    .height(CARD_HEIGHT - CARD_HEADER)
                    .bg(CARD_BG)
                    .pad_h(20.0)
                    .pad_v(12.0)
                    .pad_bottom(20.0)
                    .gap(6.0)
                    .child(
                        text(place.name.clone())
                            .size(18.0)
                            .font("brice-bold")
                            .color("#ffffff")
                            .lines(1)
                            .height(22.0),
                    )
                    .child(
                        row()
                            .width(width - 40.0)
                            .height(18.0)
                            .gap(6.0)
                            .align_items("center")
                            .child(
                                text(place.address.clone())
                                    .size(14.0)
                                    .font("satoshi-medium")
                                    .color("#fffffff2")
                                    .width(120.0)
                                    .lines(1)
                                    .height(18.0),
                            )
                            .child(Self::icon("car", 14.0, "#ffffff"))
                            .child(
                                text(format!("{distance} • {drive_minutes} min • {price}"))
                                    .size(14.0)
                                    .font("satoshi-medium")
                                    .color("#fffffff2")
                                    .height(18.0),
                            ),
                    )
                    .child(status)
                    .child(
                        row()
                            .width(150.0)
                            .height(30.0)
                            .pad_h(12.0)
                            .gap(8.0)
                            .radius(15.0)
                            .border(1.0, "#404040")
                            .align_items("center")
                            .child(
                                row()
                                    .width(16.0)
                                    .height(16.0)
                                    .radius(8.0)
                                    .bg("#ffffff")
                                    .align_items("center")
                                    .justify("center")
                                    .child(Self::icon("google", 9.0, "#4285F4")),
                            )
                            .child(
                                text(format!("{} ({})", place.rating, place.reviews))
                                    .size(12.0)
                                    .font("satoshi-medium")
                                    .color("#ffffff")
                                    .width(80.0)
                                    .height(16.0),
                            ),
                    ),
            );
        self.card_cache.borrow_mut().insert(cache_key, card.clone());
        card
    }

    pub(crate) fn carousel(&self, context: &ViewContext, places: &[Place]) -> Element {
        let width = context.viewport.width.max(320.0);
        let card_width = width * 0.90;
        let content_width = width * places.len() as f32;

        let mut strip = hscroll()
            .key("places")
            .absolute()
            .bottom(CAROUSEL_BOTTOM)
            .left(0.0)
            .width(width)
            .height(340.0)
            .paged(true)
            .scroll_x(self.selected as f32 * width)
            .content_width(content_width);

        for (index, place) in places.iter().enumerate() {
            strip = strip.child(
                column()
                    .width(width)
                    .height(340.0)
                    .align_items("center")
                    .justify("end")
                    .child(self.card(card_width, place, index == self.selected)),
            );
        }
        strip
    }
}
