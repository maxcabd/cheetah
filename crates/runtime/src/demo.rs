use crate::app::{App, Event, Viewport};
use crate::element::{button, column, input, list, row, text, Element};
use crate::layout::visible_range;

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
const ITEM_HEIGHT: f32 = 78.0;
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

pub struct ChatApp {
    messages: Vec<Message>,
    draft: String,
    next_id: u32,
    scroll_offset: f32,
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
        }
    }

    fn list_height(&self, viewport: Viewport) -> f32 {
        (viewport.height - HEADER_HEIGHT - COMPOSER_HEIGHT).max(0.0)
    }

    fn content_height(&self) -> f32 {
        self.messages.len() as f32 * ITEM_HEIGHT
    }

    fn build_view(&self, viewport: Viewport) -> Element {
        let list_height = self.list_height(viewport);
        let message_height = self.content_height();
        let content_height = message_height.max(list_height);
        let bottom_offset = (list_height - message_height).max(0.0);
        let max_offset = (content_height - list_height).max(0.0);
        let offset = self.scroll_offset.min(max_offset);
        let (start, end) = visible_range(offset, list_height, ITEM_HEIGHT, self.messages.len());

        let items = (start..end)
            .map(|index| self.message(&self.messages[index], index, bottom_offset))
            .collect::<Vec<_>>();

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

    fn message(&self, message: &Message, index: usize, bottom_offset: f32) -> Element {
        let initial = message
            .author
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();

        row()
            .key(format!("msg-{}", message.id))
            .absolute()
            .top(bottom_offset + index as f32 * ITEM_HEIGHT)
            .left(0.0)
            .width(-1.0)
            .height(ITEM_HEIGHT)
            .pad_h(12.0)
            .pad_v(8.0)
            .gap(10.0)
            .align_items("center")
            .child(
                text(initial)
                    .size(15.0)
                    .bold(true)
                    .color("#ffffff")
                    .bg(message.color)
                    .width(38.0)
                    .height(38.0)
                    .align("center")
                    .radius(19.0)
                    .label(message.author.as_str()),
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
                            .child(
                                text(message.author.as_str())
                                    .size(15.0)
                                    .bold(true)
                                    .color(TEXT)
                                    .height(18.0),
                            )
                            .child(
                                text(message.time.as_str())
                                    .size(11.0)
                                    .color(MUTED)
                                    .width(40.0)
                                    .height(14.0),
                            ),
                    )
                    .child(
                        text(message.body.as_str())
                            .size(15.0)
                            .color(BODY)
                            .height(40.0),
                    ),
            )
            .child(
                button(if message.liked {
                    format!("\u{2665} {}", message.likes)
                } else {
                    format!("\u{2661} {}", message.likes)
                })
                .key(format!("like-{}", message.id))
                .size(12.0)
                .color(if message.liked { PINK } else { MUTED })
                .bg(if message.liked { PINK_BG } else { PANEL })
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
            self.messages.remove(0);
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
                }
                Some(key) if key.starts_with("like-") => {
                    if let Ok(id) = key[5..].parse::<u32>() {
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
    fn view(&self, viewport: Viewport) -> Element {
        self.build_view(viewport)
    }

    fn update(&mut self, event: Event) {
        self.handle(event);
    }
}
