//! End to end: an agent on `/agent` sees the connected device's screen, taps
//! it by key, and the device gets the new frame; the device stays connected.

use cheetah_devserver::{run, AppSource, Options};
use futures_util::{SinkExt, StreamExt};
use runtime::app::{App, Event, ViewContext};
use runtime::{column, text, Element, Runtime};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

/// A counter: tapping `more` counts up.
struct Counter(u32);

impl App for Counter {
    fn view(&self, _: &ViewContext) -> Element {
        column()
            .width(300.0)
            .height(600.0)
            .bg("#101010")
            .child(text(format!("count {}", self.0)).key("count").size(18.0))
            .child(text("more").key("more").pressable(true).size(18.0))
    }

    fn update(&mut self, event: Event) {
        if event.kind == "click" && event.key.as_deref() == Some("more") {
            self.0 += 1;
        }
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn contains(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// The device: answers text measurements, and says when a frame with
/// `needle` arrives.
async fn device_sees(ws: &mut Socket, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let Ok(Some(Ok(message))) =
            tokio::time::timeout(Duration::from_millis(200), ws.next()).await
        else {
            continue;
        };
        match message {
            Message::Binary(bytes) if contains(&bytes, needle) => return true,
            Message::Text(text) => {
                let request: Value = serde_json::from_str(text.as_str()).unwrap();
                if request["t"] == "measure" {
                    let count = request["items"].as_array().map_or(0, Vec::len);
                    let sizes: Vec<f32> =
                        std::iter::repeat_n([60.0, 22.0], count).flatten().collect();
                    let reply = json!({"t": "measured", "seq": request["seq"], "sizes": sizes});
                    ws.send(Message::Text(reply.to_string().into()))
                        .await
                        .unwrap();
                }
            }
            _ => {}
        }
    }
    false
}

async fn ask(agent: &mut Socket, request: Value) -> Value {
    agent
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), agent.next())
            .await
            .expect("an answer")
            .unwrap()
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn connect(url: &str) -> Socket {
    for _ in 0..100 {
        if let Ok((ws, _)) = tokio_tungstenite::connect_async(url).await {
            return ws;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("dev server did not start");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_sees_and_taps_the_connected_device() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    std::thread::spawn(move || {
        let options = Options {
            port,
            lan: false,
            live: Vec::new(),
            rest: Vec::new(),
        };
        run(
            "agent-e2e",
            options,
            AppSource::custom(|| Runtime::new(Counter(0))),
        );
    });

    // No device yet: the agent is told so.
    let mut agent = connect(&format!("ws://127.0.0.1:{port}/agent")).await;
    let early = ask(&mut agent, json!({"id": 1, "t": "elements"})).await;
    assert_eq!(early["ok"], false);
    assert!(early["error"].as_str().unwrap().contains("no device"));

    let mut device = connect(&format!("ws://127.0.0.1:{port}")).await;
    device
        .send(Message::Text(
            json!({"t": "resize", "w": 300, "h": 600})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert!(device_sees(&mut device, "count 0").await, "first frame");

    // What's on screen, with keys.
    let elements = ask(&mut agent, json!({"id": 2, "t": "elements"})).await;
    assert_eq!(elements["ok"], true, "{elements}");
    assert_eq!(elements["id"], 2);
    let listed = elements["elements"].to_string();
    assert!(
        listed.contains("\"key\":\"more\"") && listed.contains("count 0"),
        "{listed}"
    );

    // A tap by key reaches the app, and the device gets the new frame.
    let tap = ask(&mut agent, json!({"id": 3, "t": "tap", "key": "more"})).await;
    assert_eq!(tap["ok"], true, "{tap}");
    assert!(
        device_sees(&mut device, "count 1").await,
        "the tap's frame reached the device"
    );

    // The screen as a PNG, painted from the live tree.
    let screen = ask(&mut agent, json!({"id": 4, "t": "screen", "scale": 1})).await;
    assert_eq!(screen["ok"], true, "{screen}");
    let png = screen["png"].as_str().unwrap();
    assert!(png.starts_with("iVBORw0KGgo"), "a PNG");
    assert!(screen["elements"].to_string().contains("count 1"));

    let missing = ask(&mut agent, json!({"id": 5, "t": "tap", "key": "nope"})).await;
    assert_eq!(missing["ok"], false);
    assert!(missing["error"].as_str().unwrap().contains("nope"));
}
