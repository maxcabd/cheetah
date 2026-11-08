use crate::app::{App, Event};
use crate::element::{button, column, gap_w, input, row, scroll, text, Element};

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
}

impl ChatApp {
    pub fn new() -> Self {
        Self {
            messages: vec![
                Message {
                    id: 1,
                    author: "mara".into(),
                    body: "the rust ui runtime is alive".into(),
                    time: "09:41".into(),
                    color: "#5865f2",
                    likes: 3,
                    liked: false,
                },
                Message {
                    id: 2,
                    author: "lena".into(),
                    body: "no js engine, no bundle, just native views".into(),
                    time: "09:42".into(),
                    color: "#eb459e",
                    likes: 7,
                    liked: true,
                },
                Message {
                    id: 3,
                    author: "dev".into(),
                    body: "the diff only sends create, update, children, remove".into(),
                    time: "09:43".into(),
                    color: "#23a55a",
                    likes: 2,
                    liked: false,
                },
                Message {
                    id: 4,
                    author: "mara".into(),
                    body: "same view tree, different producer. that is the whole trick.".into(),
                    time: "09:44".into(),
                    color: "#5865f2",
                    likes: 5,
                    liked: false,
                },
                Message {
                    id: 5,
                    author: "lena".into(),
                    body: "so when do we add iOS?".into(),
                    time: "09:45".into(),
                    color: "#eb459e",
                    likes: 1,
                    liked: false,
                },
            ],
            draft: String::new(),
            next_id: 5,
        }
    }

    fn build_view(&self) -> Element {
        let mut list = column().pad(12.0);
        for message in &self.messages {
            list = list.child(self.message(message));
        }

        column()
            .bg(BG)
            .child(self.header())
            .child(scroll().weight(1.0).child(list))
            .child(self.composer())
    }

    fn header(&self) -> Element {
        row()
            .bg(HEADER)
            .pad_h(16.0)
            .pad_v(10.0)
            .align("center")
            .child(
                column()
                    .weight(1.0)
                    .child(text("Rust Chat").size(17.0).bold(true).color(TEXT))
                    .child(text("#rust-mobile · 4 online").size(11.0).color(MUTED)),
            )
            .child(gap_w(8.0))
            .child(
                button("Clear")
                    .key("clear")
                    .size(12.0)
                    .color(MUTED)
                    .bg(PANEL)
                    .pad_h(10.0)
                    .pad_v(6.0)
                    .radius(8.0),
            )
    }

    fn message(&self, message: &Message) -> Element {
        let initial = message
            .author
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();

        row()
            .key(format!("msg-{}", message.id))
            .pad_v(7.0)
            .align("center")
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
            .child(gap_w(10.0))
            .child(
                column()
                    .weight(1.0)
                    .child(
                        row()
                            .align("center")
                            .child(
                                text(message.author.as_str())
                                    .size(15.0)
                                    .bold(true)
                                    .color(TEXT),
                            )
                            .child(gap_w(8.0))
                            .child(text(message.time.as_str()).size(11.0).color(MUTED)),
                    )
                    .child(
                        text(message.body.as_str())
                            .size(15.0)
                            .color(BODY)
                            .pad_v(2.0),
                    ),
            )
            .child(gap_w(8.0))
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
                .pad_h(9.0)
                .pad_v(5.0)
                .radius(10.0)
                .label("Like message"),
            )
    }

    fn composer(&self) -> Element {
        row()
            .bg(HEADER)
            .pad(12.0)
            .align("center")
            .child(
                input("Message #rust-mobile", self.draft.as_str())
                    .key("composer")
                    .weight(1.0)
                    .size(15.0)
                    .color(TEXT)
                    .bg(INPUT)
                    .pad_h(12.0)
                    .pad_v(10.0)
                    .radius(8.0),
            )
            .child(gap_w(8.0))
            .child(
                button("Send")
                    .key("send")
                    .size(14.0)
                    .bold(true)
                    .color("#ffffff")
                    .bg(ACCENT)
                    .pad_h(16.0)
                    .pad_v(10.0)
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

        if self.messages.len() > 40 {
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
            "click" => match event.key.as_deref() {
                Some("send") => self.send(),
                Some("clear") => self.messages.clear(),
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
    fn view(&self) -> Element {
        self.build_view()
    }

    fn update(&mut self, event: Event) {
        self.handle(event);
    }
}
