use crate::app::{App, Event, ViewContext, Viewport};
use crate::element::{button, column, input, list, row, text, Element};
use crate::layout::visible_range_prefix;
use std::cell::RefCell;
use std::collections::HashMap;

const BG: &str = "#313338";
const PANEL: &str = "#2b2d31";
const HEADER: &str = "#1e1f22";
const TEXT: &str = "#f2f3f5";
const MUTED: &str = "#949ba4";
const BODY: &str = "#dbdee1";
const ACCENT: &str = "#5865f2";
const INPUT: &str = "#383a40";
const PINK: &str = "#eb459e";
const PINK_BG: &str = "#3d1f2e";

const HEADER_HEIGHT: f32 = 64.0;
const COMPOSER_HEIGHT: f32 = 68.0;
const MIN_ITEM_HEIGHT: f32 = 56.0;
const ITEM_CHROME_HEIGHT: f32 = 36.0;
const ITEM_CHROME_WIDTH: f32 = 138.0;
const MAX_MESSAGES: usize = 500;

struct Message {
    id: u32,
    author: String,
    body: String,
    time: String,
    color: &'static str,
    likes: u32,
    liked: bool,
}

struct HeightCache {
    width: f32,
    values: HashMap<u32, (f32, f32)>,
}

struct Metrics {
    heights: Vec<f32>,
    text_heights: Vec<f32>,
    prefix: Vec<f32>,
}

pub struct ChatApp {
    messages: Vec<Message>,
    draft: String,
    next_id: u32,
    scroll_offset: f32,
    heights: RefCell<HeightCache>,
    slots: RefCell<Vec<Option<u32>>>,
}

impl ChatApp {
    pub fn new() -> Self {
        let authors = ["mara", "lena", "dev", "alex"];
        let colors = ["#5865f2", "#eb459e", "#23a55a", "#f0b232"];
        let bodies = [
            "the rust ui runtime is alive",
            "no js engine, no bundle, just native views",
            "taffy computes the layout, the host only draws",
            "only the visible rows exist as views",
            "same view tree, different producer",
            "scrolling materializes and drops rows",
            "the diff keeps identity across frames",
            "this list has 200 messages in rust",
        ];

        let messages = (0..200)
            .map(|index| Message {
                id: index as u32 + 1,
                author: authors[index % authors.len()].into(),
                body: format!("{} (#{})", bodies[index % bodies.len()], index + 1),
                time: format!("09:{:02}", index % 60),
                color: colors[index % colors.len()],
                likes: (index % 7) as u32,
                liked: index % 11 == 0,
            })
            .collect();

        Self {
            messages,
            draft: String::new(),
            next_id: 200,
            scroll_offset: f32::MAX,
            heights: RefCell::new(HeightCache {
                width: 0.0,
                values: HashMap::new(),
            }),
            slots: RefCell::new(Vec::new()),
        }
    }

    fn list_height(&self, viewport: Viewport) -> f32 {
        (viewport.height - HEADER_HEIGHT - COMPOSER_HEIGHT).max(0.0)
    }

    fn metrics(&self, context: &ViewContext) -> Metrics {
        let width = context.viewport.width.max(1.0);
        let column_width = (width - ITEM_CHROME_WIDTH).max(80.0);

        let mut cache = self.heights.borrow_mut();
        if (cache.width - width).abs() > 0.5 {
            cache.width = width;
            cache.values.clear();
        }

        let mut heights = Vec::with_capacity(self.messages.len());
        let mut text_heights = Vec::with_capacity(self.messages.len());

        for message in &self.messages {
            let (item_height, text_height) = match cache.values.get(&message.id) {
                Some(value) => *value,
                None => {
                    let (_, measured) =
                        context.measure_text(&message.body, 15.0, false, column_width);
                    let text_height = measured.max(18.0);
                    let item_height = (ITEM_CHROME_HEIGHT + text_height).max(MIN_ITEM_HEIGHT);
                    cache.values.insert(message.id, (item_height, text_height));
                    (item_height, text_height)
                }
            };

            heights.push(item_height);
            text_heights.push(text_height);
        }

        let mut prefix = Vec::with_capacity(heights.len() + 1);
        prefix.push(0.0);
        for height in &heights {
            prefix.push(prefix.last().copied().unwrap_or(0.0) + height);
        }

        Metrics {
            heights,
            text_heights,
            prefix,
        }
    }

    fn build_view(&self, context: &ViewContext) -> Element {
        let viewport = context.viewport;
        let list_height = self.list_height(viewport);
        let metrics = self.metrics(context);
        let total_height = metrics.prefix.last().copied().unwrap_or(0.0);
        let content_height = total_height.max(list_height);
        let bottom_offset = (list_height - total_height).max(0.0);
        let max_offset = (total_height - list_height).max(0.0);
        let offset = self.scroll_offset.min(max_offset);
        let (start, end) = visible_range_prefix(offset, list_height, &metrics.prefix);
        let visible_count = end.saturating_sub(start);

        let slot_count = (((list_height / MIN_ITEM_HEIGHT).ceil() as usize) + 4).clamp(8, 48);
        let mut slots = self.slots.borrow_mut();
        slots.resize(slot_count, None);

        let mut items = Vec::with_capacity(slot_count);
        for slot in 0..slot_count {
            if slot < visible_count {
                let index = start + slot;
                if index < self.messages.len() {
                    let message = &self.messages[index];
                    slots[slot] = Some(message.id);
                    items.push(self.slot_row(
                        slot,
                        Some(message),
                        bottom_offset + metrics.prefix[index],
                        metrics.heights[index],
                        metrics.text_heights[index],
                    ));
                    continue;
                }
            }

            slots[slot] = None;
            items.push(self.slot_row(slot, None, 0.0, 0.0, 0.0));
        }

        column()
            .width(-1.0)
            .height(-1.0)
            .bg(BG)
            .child(self.header())
            .child(
                list()
                    .key("messages")
                    .height(list_height)
                    .content_height(content_height)
                    .scroll_to(offset)
                    .children(items),
            )
            .child(self.composer())
    }

    fn header(&self) -> Element {
        row()
            .height(HEADER_HEIGHT)
            .bg(HEADER)
            .pad_h(16.0)
            .gap(8.0)
            .align_items("center")
            .child(
                column()
                    .weight(1.0)
                    .gap(2.0)
                    .child(
                        text("Rust Chat")
                            .size(17.0)
                            .bold(true)
                            .color(TEXT)
                            .height(22.0),
                    )
                    .child(
                        text(format!("#rust-mobile · {} messages", self.messages.len()))
                            .size(11.0)
                            .color(MUTED)
                            .height(15.0),
                    ),
            )
            .child(
                button("Clear")
                    .key("clear")
                    .size(12.0)
                    .color(MUTED)
                    .bg(PANEL)
                    .width(56.0)
                    .height(30.0)
                    .radius(8.0),
            )
    }

    fn slot_row(
        &self,
        slot: usize,
        message: Option<&Message>,
        top: f32,
        item_height: f32,
        text_height: f32,
    ) -> Element {
        let (author, time, body, color, initial, likes, liked, label) = match message {
            Some(message) => {
                let initial = message
                    .author
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                (
                    message.author.as_str(),
                    message.time.as_str(),
                    message.body.as_str(),
                    message.color,
                    initial,
                    message.likes,
                    message.liked,
                    message.author.as_str(),
                )
            }
            None => ("", "", "", BG, String::new(), 0, false, ""),
        };

        row()
            .key(format!("slot-{slot}"))
            .absolute()
            .top(top)
            .left(0.0)
            .width(-1.0)
            .height(item_height)
            .pad_h(12.0)
            .pad_v(8.0)
            .gap(10.0)
            .align_items("center")
            .child(
                text(initial)
                    .size(15.0)
                    .bold(true)
                    .color("#ffffff")
                    .bg(color)
                    .width(38.0)
                    .height(38.0)
                    .align("center")
                    .radius(19.0)
                    .label(label),
            )
            .child(
                column()
                    .weight(1.0)
                    .gap(2.0)
                    .child(
                        row()
                            .height(18.0)
                            .gap(8.0)
                            .align_items("center")
                            .child(text(author).size(15.0).bold(true).color(TEXT).height(18.0))
                            .child(text(time).size(11.0).color(MUTED).width(40.0).height(14.0)),
                    )
                    .child(text(body).size(15.0).color(BODY).height(text_height)),
            )
            .child(
                button(if liked {
                    format!("\u{2665} {}", likes)
                } else {
                    format!("\u{2661} {}", likes)
                })
                .key(format!("like-slot-{slot}"))
                .size(12.0)
                .color(if liked { PINK } else { MUTED })
                .bg(if liked { PINK_BG } else { PANEL })
                .width(56.0)
                .height(28.0)
                .radius(10.0)
                .label("Like message"),
            )
    }

    fn composer(&self) -> Element {
        row()
            .height(COMPOSER_HEIGHT)
            .bg(HEADER)
            .pad(12.0)
            .gap(8.0)
            .align_items("center")
            .child(
                input("Message #rust-mobile", self.draft.as_str())
                    .key("composer")
                    .weight(1.0)
                    .height(40.0)
                    .size(15.0)
                    .color(TEXT)
                    .bg(INPUT)
                    .pad_h(12.0)
                    .align("start")
                    .radius(8.0),
            )
            .child(
                button("Send")
                    .key("send")
                    .width(64.0)
                    .height(40.0)
                    .size(14.0)
                    .bold(true)
                    .color("#ffffff")
                    .bg(ACCENT)
                    .radius(8.0),
            )
    }

    fn send(&mut self) {
        let body = self.draft.trim().to_string();
        if body.is_empty() {
            return;
        }

        self.next_id += 1;
        self.messages.push(Message {
            id: self.next_id,
            author: "you".into(),
            body,
            time: "now".into(),
            color: ACCENT,
            likes: 0,
            liked: false,
        });
        self.draft.clear();
        self.scroll_offset = f32::MAX;

        if self.messages.len() > MAX_MESSAGES {
            let removed = self.messages.remove(0);
            self.heights.borrow_mut().values.remove(&removed.id);
        }
    }

    fn handle(&mut self, event: Event) {
        match event.kind.as_str() {
            "change" if event.key.as_deref() == Some("composer") => {
                self.draft = event.payload;
            }
            "submit" if event.key.as_deref() == Some("composer") => {
                self.send();
            }
            "scroll" if event.key.as_deref() == Some("messages") => {
                if let Ok(offset) = event.payload.parse::<f32>() {
                    self.scroll_offset = offset.max(0.0);
                }
            }
            "click" => match event.key.as_deref() {
                Some("send") => self.send(),
                Some("clear") => {
                    self.messages.clear();
                    self.scroll_offset = 0.0;
                    self.heights.borrow_mut().values.clear();
                    self.slots.borrow_mut().clear();
                }
                Some(key) if key.starts_with("like-slot-") => {
                    if let Ok(slot) = key["like-slot-".len()..].parse::<usize>() {
                        let id = {
                            let slots = self.slots.borrow();
                            slots.get(slot).copied().flatten()
                        };
                        if let Some(id) = id {
                            if let Some(message) =
                                self.messages.iter_mut().find(|message| message.id == id)
                            {
                                message.liked = !message.liked;
                                message.likes = if message.liked {
                                    message.likes + 1
                                } else {
                                    message.likes.saturating_sub(1)
                                };
                            }
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl Default for ChatApp {
    fn default() -> Self {
        Self::new()
    }
}

impl App for ChatApp {
    fn view(&self, context: &ViewContext) -> Element {
        self.build_view(context)
    }

    fn update(&mut self, event: Event) {
        self.handle(event);
    }
}
