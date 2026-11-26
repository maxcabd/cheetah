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
//!
//! An agent drives the app, so it needs the server's token: `?token=…` on
//! the URL, or `Authorization: Bearer …`. Each server start makes a new one
//! (or takes CHEETAH_AGENT_TOKEN), prints the URL with it, and writes it to
//! [`token_file`], where tools on this machine read it.

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

static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Where the token for the server on `port` is kept, for tools on this
/// machine (cheetah-mcp reads it).
pub fn token_file(port: u16) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cheetah-agent-token-{port}"))
}

/// This server's token: CHEETAH_AGENT_TOKEN, else 128 random bits. Written
/// to `token_file(port)`, readable by this user only where that's possible.
pub(crate) fn start(port: u16) -> String {
    let token = TOKEN
        .get_or_init(|| {
            std::env::var("CHEETAH_AGENT_TOKEN")
                .ok()
                .filter(|token| !token.is_empty())
                .unwrap_or_else(random_token)
        })
        .clone();
    let path = token_file(port);
    if let Err(error) = std::fs::write(&path, &token) {
        eprintln!("devserver: cannot write {}: {error}", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    token
}

/// 32 hex digits from the OS's randomness (std seeds each RandomState with
/// fresh random keys).
fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|part| {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u64(part);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

/// Whether a handshake to `/agent` carries this server's token.
pub(crate) fn authorized(
    request: &tokio_tungstenite::tungstenite::handshake::server::Request,
) -> bool {
    let Some(expected) = TOKEN.get() else {
        return false;
    };
    let from_query = request.uri().query().and_then(|query| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("token="))
            .map(str::to_string)
    });
    let from_header = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);
    [from_query, from_header]
        .into_iter()
        .flatten()
        .any(|given| same(given.as_bytes(), expected.as_bytes()))
}

/// Equal, taking as long whatever the first difference: no timing hints.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

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
