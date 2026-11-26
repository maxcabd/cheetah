//! `/agent`: the connected device's app, for agents on any machine. Beside
//! the device's own WebSocket, on the same port; it never replaces the
//! device. The app runs here, so its screen is here too: agents read it,
//! tap and type into it by key, and the phone follows as if touched.
//!
//! agent -> server (JSON text, one request at a time):
//!   {"id":1,"t":"elements"}                    what's on screen, with keys
//!   {"id":2,"t":"screen","scale":1}            that, and a PNG (base64)
//!   {"id":3,"t":"tap","key":"drawer:open"}
//!   {"id":4,"t":"type","key":"search","text":"sushi"}
//!   {"id":5,"t":"event","kind":"submit","key":"search","payload":""}
//! server -> agent: {"id":n,"ok":true,...} or {"id":n,"ok":false,"error":"..."}
//!
//! The PNG is painted here from the live tree (tools/render): native views
//! are stand-ins, but layout, text and colors are the device's.

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use runtime::dev::RerenderHandle;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// How long the app's thread may take to hand over its screen.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(3);
/// After an input: long enough for the app to render the result, so the
/// next look shows it.
const SETTLE: Duration = Duration::from_millis(350);

pub(crate) async fn serve(ws: WebSocketStream<TcpStream>, peer: String) {
    println!("devserver: agent connected ({peer})");
    let (mut sink, mut stream) = ws.split();
    while let Some(message) = stream.next().await {
        let text = match message {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Ping(data)) => {
                let _ = sink.send(Message::Pong(data)).await;
                continue;
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => continue,
        };
        let request: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let id = request["id"].clone();
        let mut reply = tokio::task::spawn_blocking(move || answer(&request))
            .await
            .unwrap_or_else(|error| failure(&format!("agent request failed: {error}")));
        reply["id"] = id;
        if sink.send(Message::text(reply.to_string())).await.is_err() {
            break;
        }
    }
    println!("devserver: agent disconnected ({peer})");
}

fn failure(error: &str) -> Value {
    json!({"ok": false, "error": error})
}

fn current() -> Option<RerenderHandle> {
    crate::CURRENT.lock().ok()?.clone()
}

/// One request, off the async workers: painting and waiting block.
fn answer(request: &Value) -> Value {
    let Some(app) = current() else {
        return failure("no device connected: open the dev build and connect it to this server");
    };
    let kind = request["t"].as_str().unwrap_or("");
    match kind {
        "elements" | "screen" => {
            let Some(snapshot) = app.snapshot(SNAPSHOT_TIMEOUT) else {
                return failure("the app hasn't drawn a screen yet");
            };
            let Some(root) = snapshot.tree() else {
                return failure("the screen is empty");
            };
            let scale = if kind == "screen" {
                request["scale"].as_f64().unwrap_or(1.0).clamp(0.25, 3.0) as f32
            } else {
                0.25
            };
            match cheetah_render::draw(
                &root,
                &snapshot.frames,
                (snapshot.width, snapshot.height),
                scale,
                &cheetah_render::default_assets(),
            ) {
                Ok((png, elements)) => {
                    let mut reply = json!({"ok": true, "size": [snapshot.width, snapshot.height], "elements": elements});
                    if kind == "screen" {
                        reply["png"] = base64::engine::general_purpose::STANDARD.encode(png).into();
                    }
                    reply
                }
                Err(error) => failure(&error),
            }
        }
        "tap" | "type" | "event" => {
            let Some(key) = request["key"].as_str() else {
                return failure("needs a key");
            };
            let Some(snapshot) = app.snapshot(SNAPSHOT_TIMEOUT) else {
                return failure("the app hasn't drawn a screen yet");
            };
            let Some(node) = snapshot.find(key) else {
                return failure(&format!("nothing on screen with key {key}"));
            };
            match kind {
                // A click on a link navigates, as a tap on the device does.
                "tap" => app.event(node.id, "click", ""),
                "type" => app.event(node.id, "change", request["text"].as_str().unwrap_or("")),
                _ => app.event(
                    node.id,
                    request["kind"].as_str().unwrap_or("click"),
                    request["payload"].as_str().unwrap_or(""),
                ),
            }
            std::thread::sleep(SETTLE);
            json!({"ok": true})
        }
        other => failure(&format!(
            "unknown request {other:?}: elements, screen, tap, type or event"
        )),
    }
}
