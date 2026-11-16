//! End to end: adding a new element to a `.view` file reaches a connected
//! device as an op frame, with no rebuild.

use cheetah_devserver::{run, AppSource, Options};
use futures_util::{SinkExt, StreamExt};
use runtime::app::{App, Event, ViewContext};
use runtime::{Element, Runtime};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

struct Fixture;

impl App for Fixture {
    fn view(&self, context: &ViewContext) -> Element {
        context.view("Screen", &json!({}))
    }

    fn update(&mut self, _: Event) {}

    fn views(&self) -> Vec<(&'static str, &'static str)> {
        vec![("screen.view", include_str!("fixture_views/screen.view"))]
    }
}

/// Puts the fixture source back even if the test fails.
struct Restore(PathBuf, String);

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.0, &self.1);
    }
}

fn contains(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Read frames until one contains `needle`, answering text measurements.
async fn wait_for(ws: &mut Socket, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let Ok(Some(Ok(message))) =
            tokio::time::timeout(Duration::from_millis(500), ws.next()).await
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
                        std::iter::repeat_n([40.0, 18.0], count).flatten().collect();
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

#[tokio::test(flavor = "multi_thread")]
async fn new_view_reaches_the_device_without_a_rebuild() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixture_views");
    let file = dir.join("screen.view");
    let original = std::fs::read_to_string(&file).unwrap();
    let _restore = Restore(file.clone(), original.clone());

    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    std::thread::spawn(move || {
        let options = Options {
            port,
            lan: false,
            live: vec![dir],
            rest: Vec::new(),
        };
        run(
            "view-e2e",
            options,
            AppSource::custom(|| Runtime::new(Fixture)),
        );
    });

    let url = format!("ws://127.0.0.1:{port}");
    let mut socket = None;
    for _ in 0..100 {
        if let Ok((ws, _)) = tokio_tungstenite::connect_async(&url).await {
            socket = Some(ws);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut ws = socket.expect("dev server did not start");
    ws.send(Message::Text(
        json!({"t": "resize", "w": 300, "h": 600})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    assert!(wait_for(&mut ws, "view-before").await, "initial render");

    // A brand-new element, which Rust literal patching could never add.
    let saved = Instant::now();
    let edited = original.replace(
        "    text \"view-before\" size=14\n",
        "    text \"view-before\" size=14\n    row gap=4\n      text \"view-added\" size=18\n",
    );
    assert_ne!(edited, original, "fixture edit did not apply");
    std::fs::write(&file, edited).unwrap();
    assert!(
        wait_for(&mut ws, "view-added").await,
        "new view never reached the device"
    );
    let elapsed = saved.elapsed();
    println!("save -> device frame: {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "view edit took {elapsed:?}"
    );
}
