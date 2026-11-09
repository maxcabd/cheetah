use crate::app::{Action, App, Event, HostInfo, ViewContext, Viewport};
use crate::element::{button, column, glass, input, list, row, text, Element};
use crate::layout::{rough_measure, visible_range_prefix};
use crate::model::{Message, MessageStatus};
use crate::store::Store;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;

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
const MAX_ATTEMPTS: u32 = 3;
const HEADER_TINT: &str = "#1e1f2280";
const SEND_TINT: &str = "#5865f299";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConnectionStatus {
    Connecting,
    Online,
    Offline,
}

impl ConnectionStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Online => "online",
            Self::Offline => "offline",
        }
    }
}

fn seed_messages() -> Vec<Message> {
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

    (0..200)
        .map(|index| Message {
            id: index as u32 + 1,
            author: authors[index % authors.len()].into(),
            body: format!("{} (#{})", bodies[index % bodies.len()], index + 1),
            time: format!("09:{:02}", index % 60),
            color: colors[index % colors.len()].to_string(),
            likes: (index % 7) as u32,
            liked: index % 11 == 0,
            status: MessageStatus::Sent,
            attempts: 0,
        })
        .collect()
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

fn prefix_for(heights: &[f32]) -> Vec<f32> {
    let mut prefix = Vec::with_capacity(heights.len() + 1);
    prefix.push(0.0);
    for height in heights {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + height);
    }
    prefix
}

fn estimate_item(message: &Message, column_width: f32) -> (f32, f32) {
    let (_, measured) = rough_measure(&message.body, 15.0, column_width);
    let text_height = measured.max(18.0);
    (
        (ITEM_CHROME_HEIGHT + text_height).max(MIN_ITEM_HEIGHT),
        text_height,
    )
}

fn measure_item(context: &ViewContext, message: &Message, column_width: f32) -> (f32, f32) {
    let (_, measured) = context.measure_text(&message.body, 15.0, false, column_width);
    let text_height = measured.max(18.0);
    (
        (ITEM_CHROME_HEIGHT + text_height).max(MIN_ITEM_HEIGHT),
        text_height,
    )
}

pub struct ChatApp {
    messages: Vec<Message>,
    store: Store,
    connection: ConnectionStatus,
    draft: String,
    next_id: u32,
    scroll_offset: f32,
    host: HostInfo,
    heights: RefCell<HeightCache>,
    slots: RefCell<Vec<Option<u32>>>,
    actions: Vec<Action>,
    stick_to_bottom: Cell<bool>,
    last_max_offset: Cell<f32>,
}

impl ChatApp {
    pub fn new(store_path: &str) -> Self {
        let store = Store::open(Path::new(store_path)).expect("open cheetah store");
        let mut messages = store.load();
        let mut healed = false;

        for message in &mut messages {
            if message.status == MessageStatus::Sending {
                message.status = MessageStatus::Queued;
                message.time = "queued".into();
                healed = true;
            }
        }

        if healed {
            store.save_all(&messages);
        }

        if messages.is_empty() && !store.initialized() {
            messages = seed_messages();
            store.save_all(&messages);
            store.mark_initialized();
        } else if !store.initialized() {
            store.mark_initialized();
        }

        let next_id = messages.iter().map(|message| message.id).max().unwrap_or(0);

        Self {
            messages,
            store,
            connection: ConnectionStatus::Connecting,
            draft: String::new(),
            next_id,
            scroll_offset: f32::MAX,
            host: HostInfo::default(),
            heights: RefCell::new(HeightCache {
                width: 0.0,
                values: HashMap::new(),
            }),
            slots: RefCell::new(Vec::new()),
            actions: Vec::new(),
            stick_to_bottom: Cell::new(true),
            last_max_offset: Cell::new(0.0),
        }
    }

    fn overlay_chrome(&self) -> bool {
        self.host.overlay_chrome
    }

    fn header_inset(&self) -> f32 {
        if self.overlay_chrome() {
            self.host.safe_top.max(0.0) + HEADER_HEIGHT
        } else {
            0.0
        }
    }

    fn composer_padding(&self) -> f32 {
        if self.overlay_chrome() && self.host.keyboard_inset <= 0.0 {
            self.host.safe_bottom.max(0.0)
        } else {
            0.0
        }
    }

    fn composer_inset(&self) -> f32 {
        if self.overlay_chrome() {
            self.host.keyboard_inset.max(0.0) + COMPOSER_HEIGHT + self.composer_padding()
        } else {
            0.0
        }
    }

    fn scroll_height(&self, viewport: Viewport) -> f32 {
        if self.overlay_chrome() {
            (viewport.height - self.header_inset() - self.composer_inset()).max(0.0)
        } else {
            (viewport.height - HEADER_HEIGHT - COMPOSER_HEIGHT).max(0.0)
        }
    }

    fn visible_height(&self, viewport: Viewport) -> f32 {
        if self.overlay_chrome() {
            viewport.height.max(1.0)
        } else {
            self.scroll_height(viewport)
        }
    }

    fn metrics(&self, context: &ViewContext) -> Metrics {
        let width = context.viewport.width.max(1.0);
        let column_width = (width - ITEM_CHROME_WIDTH).max(80.0);
        let scroll_height = self.scroll_height(context.viewport);
        let visible_height = self.visible_height(context.viewport);

        let mut cache = self.heights.borrow_mut();
        if (cache.width - width).abs() > 0.5 {
            cache.width = width;
            cache.values.clear();
        }

        let mut heights = Vec::with_capacity(self.messages.len());
        let mut text_heights = Vec::with_capacity(self.messages.len());
        for message in &self.messages {
            match cache.values.get(&message.id) {
                Some((item_height, text_height)) => {
                    heights.push(*item_height);
                    text_heights.push(*text_height);
                }
                None => {
                    let (item_height, text_height) = estimate_item(message, column_width);
                    heights.push(item_height);
                    text_heights.push(text_height);
                }
            }
        }

        let mut prefix = prefix_for(&heights);
        let mut offset = self
            .scroll_offset
            .min((prefix.last().copied().unwrap_or(0.0) - scroll_height).max(0.0));
        let (mut start, mut end) = visible_range_prefix(offset, visible_height, &prefix);

        // Measure only what is about to be rendered, plus neighbors that the
        // corrected heights pull into view.
        for _ in 0..4 {
            let mut changed = false;
            for index in start..end {
                let message = &self.messages[index];
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    cache.values.entry(message.id)
                {
                    let value = measure_item(context, message, column_width);
                    entry.insert(value);
                    heights[index] = value.0;
                    text_heights[index] = value.1;
                    changed = true;
                }
            }

            if !changed {
                break;
            }

            prefix = prefix_for(&heights);
            offset = self
                .scroll_offset
                .min((prefix.last().copied().unwrap_or(0.0) - scroll_height).max(0.0));
            let (new_start, new_end) = visible_range_prefix(offset, visible_height, &prefix);
            start = new_start;
            end = new_end;
        }

        Metrics {
            heights,
            text_heights,
            prefix,
        }
    }

    fn build_view(&self, context: &ViewContext) -> Element {
        let viewport = context.viewport;
        let scroll_height = self.scroll_height(viewport);
        let visible_height = self.visible_height(viewport);
        let metrics = self.metrics(context);
        let total_height = metrics.prefix.last().copied().unwrap_or(0.0);
        let content_height = total_height.max(scroll_height);
        let bottom_offset = (scroll_height - total_height).max(0.0);
        let max_offset = (total_height - scroll_height).max(0.0);
        self.last_max_offset.set(max_offset);
        let offset = self.scroll_offset.min(max_offset);
        let (start, end) = visible_range_prefix(offset, visible_height, &metrics.prefix);
        let visible_count = end.saturating_sub(start);

        let slot_count = (((visible_height / MIN_ITEM_HEIGHT).ceil() as usize) + 4).clamp(8, 48);
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

        let mut messages = list()
            .key("messages")
            .height(if self.overlay_chrome() {
                -1.0
            } else {
                scroll_height
            })
            .content_height(content_height)
            .scroll_to(offset)
            .children(items);

        if self.overlay_chrome() {
            messages = messages
                .content_inset_top(self.header_inset())
                .content_inset_bottom(self.composer_inset());
        }

        let mut root = column().width(-1.0).height(-1.0).bg(BG);
        if self.overlay_chrome() {
            root = root
                .child(messages)
                .child(self.header())
                .child(self.composer());
        } else {
            root = root
                .child(self.header())
                .child(messages)
                .child(self.composer());
        }
        root
    }

    fn header(&self) -> Element {
        if self.overlay_chrome() {
            let safe_top = self.host.safe_top.max(0.0);
            glass()
                .key("header")
                .glass_style("regular")
                .glass_tint(HEADER_TINT)
                .absolute()
                .top(0.0)
                .left(0.0)
                .width(-1.0)
                .height(safe_top + HEADER_HEIGHT)
                .child(
                    column()
                        .width(-1.0)
                        .height(-1.0)
                        .child(row().width(-1.0).height(safe_top))
                        .child(self.header_content(true)),
                )
        } else {
            self.header_content(false)
        }
    }

    fn header_content(&self, glass_mode: bool) -> Element {
        let clear = if glass_mode {
            self.glass_button("Clear", "clear", 56.0, 30.0, PANEL)
        } else {
            button("Clear")
                .key("clear")
                .test_id("cheetah-clear")
                .size(12.0)
                .color(MUTED)
                .bg(PANEL)
                .width(56.0)
                .height(30.0)
                .radius(8.0)
        };

        row()
            .width(-1.0)
            .height(HEADER_HEIGHT)
            .bg(if glass_mode { "#00000000" } else { HEADER })
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
                        text(format!(
                            "#rust-mobile · {} messages · {}",
                            self.messages.len(),
                            self.connection.label()
                        ))
                        .size(11.0)
                        .color(MUTED)
                        .height(15.0),
                    ),
            )
            .child(clear)
    }

    fn glass_button(&self, label: &str, key: &str, width: f32, height: f32, tint: &str) -> Element {
        let primary = label == "Send";
        let color = if primary { "#ffffff" } else { MUTED };
        let size = if primary { 14.0 } else { 12.0 };

        glass()
            .key(format!("{key}-glass"))
            .glass_style("regular")
            .glass_tint(tint)
            .glass_interactive(true)
            .width(width)
            .height(height)
            .radius(8.0)
            .child(
                button(label)
                    .key(key)
                    .test_id(format!("cheetah-{key}"))
                    .width(-1.0)
                    .height(-1.0)
                    .size(size)
                    .bold(primary)
                    .color(color)
                    .bg("#00000000")
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
        let (author, time, body, color, initial, likes, liked, label, status) = match message {
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
                    message.color.as_str(),
                    initial,
                    message.likes,
                    message.liked,
                    message.author.as_str(),
                    message.status,
                )
            }
            None => (
                "",
                "",
                "",
                BG,
                String::new(),
                0,
                false,
                "",
                MessageStatus::Sent,
            ),
        };
        let time_color = match status {
            MessageStatus::Failed => "#ed4245",
            MessageStatus::Queued => "#f0b232",
            _ => MUTED,
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
                            .child(
                                text(time)
                                    .size(11.0)
                                    .color(time_color)
                                    .width(48.0)
                                    .height(14.0),
                            ),
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
        if self.overlay_chrome() {
            let keyboard = self.host.keyboard_inset.max(0.0);
            let padding = self.composer_padding();
            glass()
                .key("composer")
                .glass_style("regular")
                .glass_tint(HEADER_TINT)
                .absolute()
                .bottom(keyboard)
                .left(0.0)
                .width(-1.0)
                .height(COMPOSER_HEIGHT + padding)
                .child(
                    column()
                        .width(-1.0)
                        .height(-1.0)
                        .child(self.composer_content(true))
                        .child(row().width(-1.0).height(padding)),
                )
        } else {
            self.composer_content(false)
        }
    }

    fn composer_content(&self, glass_mode: bool) -> Element {
        let send = if glass_mode {
            self.glass_button("Send", "send", 64.0, 40.0, SEND_TINT)
        } else {
            button("Send")
                .key("send")
                .test_id("cheetah-send")
                .width(64.0)
                .height(40.0)
                .size(14.0)
                .bold(true)
                .color("#ffffff")
                .bg(ACCENT)
                .radius(8.0)
        };

        row()
            .width(-1.0)
            .height(COMPOSER_HEIGHT)
            .bg(if glass_mode { "#00000000" } else { HEADER })
            .pad(12.0)
            .gap(8.0)
            .align_items("center")
            .child(
                input("Message #rust-mobile", self.draft.as_str())
                    .key("composer")
                    .test_id("cheetah-composer")
                    .weight(1.0)
                    .height(40.0)
                    .size(15.0)
                    .color(TEXT)
                    .bg(INPUT)
                    .pad_h(12.0)
                    .align("start")
                    .radius(8.0),
            )
            .child(send)
    }

    fn send(&mut self) {
        let body = self.draft.trim().to_string();
        if body.is_empty() {
            return;
        }

        self.next_id += 1;
        let id = self.next_id;
        let online = self.connection == ConnectionStatus::Online;
        let message = Message {
            id,
            author: "you".into(),
            body: body.clone(),
            time: if online { "sending" } else { "queued" }.into(),
            color: ACCENT.into(),
            likes: 0,
            liked: false,
            status: if online {
                MessageStatus::Sending
            } else {
                MessageStatus::Queued
            },
            attempts: 0,
        };

        self.store.save(&message);
        self.messages.push(message);
        self.draft.clear();

        if online {
            self.actions.push(Action::SendMessage { id, body });
        }

        self.stick_to_bottom.set(true);
        self.scroll_offset = f32::MAX;

        if self.messages.len() > MAX_MESSAGES {
            let removed = self.messages.remove(0);
            self.store.delete(removed.id);
            self.heights.borrow_mut().values.remove(&removed.id);
        }
    }

    fn flush_queue(&mut self) {
        if self.connection != ConnectionStatus::Online {
            return;
        }

        let mut outgoing = Vec::new();
        for message in &mut self.messages {
            if message.status == MessageStatus::Queued {
                message.status = MessageStatus::Sending;
                message.time = "sending".into();
                outgoing.push((message.id, message.body.clone(), message.clone()));
            }
        }

        for (_, _, message) in &outgoing {
            self.store.save(message);
        }
        for (id, body, _) in outgoing {
            self.actions.push(Action::SendMessage { id, body });
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
                    let offset = offset.max(0.0);
                    self.stick_to_bottom
                        .set(offset >= self.last_max_offset.get() - 1.0);
                    self.scroll_offset = offset;
                }
            }
            "ack" => {
                if let Ok(id) = event.payload.parse::<u32>() {
                    let updated = if let Some(message) =
                        self.messages.iter_mut().find(|message| message.id == id)
                    {
                        if message.status == MessageStatus::Sending {
                            message.status = MessageStatus::Sent;
                            message.time = "now".into();
                            Some(message.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(message) = updated {
                        self.store.save(&message);
                    }
                }
            }
            "fail" => {
                if let Ok(id) = event.payload.parse::<u32>() {
                    let updated = if let Some(message) =
                        self.messages.iter_mut().find(|message| message.id == id)
                    {
                        if message.status == MessageStatus::Sending {
                            message.attempts += 1;
                            if message.attempts >= MAX_ATTEMPTS {
                                message.status = MessageStatus::Failed;
                                message.time = "failed".into();
                            } else {
                                message.status = MessageStatus::Queued;
                                message.time = "queued".into();
                            }
                            Some(message.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(message) = updated {
                        self.store.save(&message);
                    }
                }
            }
            "host" => {
                if let Ok(host) = serde_json::from_str::<HostInfo>(&event.payload) {
                    self.host = host;
                }
            }
            "connection" => {
                self.connection = match event.payload.as_str() {
                    "online" => ConnectionStatus::Online,
                    "offline" => ConnectionStatus::Offline,
                    _ => ConnectionStatus::Connecting,
                };
                if self.connection == ConnectionStatus::Online {
                    self.flush_queue();
                } else if self.connection == ConnectionStatus::Offline {
                    let mut changed = false;
                    for message in &mut self.messages {
                        if message.status == MessageStatus::Sending {
                            message.status = MessageStatus::Queued;
                            message.time = "queued".into();
                            changed = true;
                        }
                    }
                    if changed {
                        self.store.save_all(&self.messages);
                    }
                }
            }
            "flush" => {
                self.flush_queue();
            }
            "incoming" => {
                if let Some((author, body)) = event.payload.split_once('\u{1f}') {
                    if !body.is_empty() {
                        self.next_id += 1;
                        let color = match author {
                            "mara" => "#5865f2",
                            "lena" => "#eb459e",
                            "dev" => "#23a55a",
                            _ => "#f0b232",
                        };
                        let message = Message {
                            id: self.next_id,
                            author: author.to_string(),
                            body: body.to_string(),
                            time: "now".into(),
                            color: color.into(),
                            likes: 0,
                            liked: false,
                            status: MessageStatus::Sent,
                            attempts: 0,
                        };
                        self.store.save(&message);
                        self.messages.push(message);

                        if self.stick_to_bottom.get() {
                            self.scroll_offset = f32::MAX;
                        }

                        if self.messages.len() > MAX_MESSAGES {
                            let removed = self.messages.remove(0);
                            self.store.delete(removed.id);
                            self.heights.borrow_mut().values.remove(&removed.id);
                        }
                    }
                }
            }
            "click" => match event.key.as_deref() {
                Some("send") => self.send(),
                Some("clear") => {
                    self.messages.clear();
                    self.store.clear();
                    self.scroll_offset = 0.0;
                    self.stick_to_bottom.set(true);
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
                            let updated = if let Some(message) =
                                self.messages.iter_mut().find(|message| message.id == id)
                            {
                                message.liked = !message.liked;
                                message.likes = if message.liked {
                                    message.likes + 1
                                } else {
                                    message.likes.saturating_sub(1)
                                };
                                Some(message.clone())
                            } else {
                                None
                            };

                            if let Some(message) = updated {
                                self.store.save(&message);
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

impl App for ChatApp {
    fn view(&self, context: &ViewContext) -> Element {
        self.build_view(context)
    }

    fn update(&mut self, event: Event) {
        self.handle(event);
    }

    fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    fn has_queued(&self) -> bool {
        self.messages
            .iter()
            .any(|message| message.status == MessageStatus::Queued)
    }
}
