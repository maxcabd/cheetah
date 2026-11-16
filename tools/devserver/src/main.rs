//! cheetah-devserver: run a Cheetah app on this machine and render it on a
//! device over WebSocket, so app changes only need a desktop rebuild.
//!
//! The device host connects in dev mode (Android: `--es devServer
//! ws://localhost:8787` with `adb reverse tcp:8787 tcp:8787`). The protocol
//! mirrors the FFI boundary:
//!
//! device -> server (JSON text):
//!   {"t":"resize","w":393,"h":852}
//!   {"t":"event","id":12,"code":1,"payload":""}      numeric event code
//!   {"t":"event","id":0,"kind":"host","payload":"{...}"}
//!   {"t":"lifecycle","active":true}
//!   {"t":"measured","seq":7,"sizes":[w0,h0,w1,h1,...]}
//! server -> device:
//!   binary: an op frame, byte-identical to what the FFI callback delivers
//!   text:   {"t":"measure","seq":7,"items":[[text,size,bold,maxWidth],...]}
//!
//! One device at a time; a new connection replaces the previous one. Each
//! connection starts a fresh app, so the device clears its views on connect.

use futures_util::{SinkExt, StreamExt};
use runtime::dev::{rough_measure, DevRuntime, Host, TextRequest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

const MEASURE_TIMEOUT: Duration = Duration::from_secs(5);

struct Options {
    app: String,
    port: u16,
    lan: bool,
}

fn options() -> Result<Options, String> {
    let mut options = Options {
        app: "nightshade".into(),
        port: 8787,
        lan: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--app" => options.app = args.next().ok_or("--app needs a name")?,
            "--port" => {
                options.port = args
                    .next()
                    .and_then(|p| p.parse().ok())
                    .ok_or("--port needs a number")?
            }
            "--lan" => options.lan = true,
            "-h" | "--help" => {
                return Err("usage: cheetah-devserver [--app nightshade|nightshade-demo|chat] [--port 8787] [--lan]\n\
                    --lan listens on all interfaces (for an iPhone on Wi-Fi); default is localhost only"
                    .into())
            }
            other => return Err(format!("unknown argument {other:?}; see --help")),
        }
    }
    Ok(options)
}

/// Host that forwards ops to the connected device and asks it to measure.
struct WsHost {
    out: UnboundedSender<Message>,
    seq: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Vec<f32>>>>,
    alive: AtomicBool,
}

impl WsHost {
    fn answer(&self, seq: u64, sizes: Vec<f32>) {
        if let Some(sender) = self.pending.lock().unwrap().remove(&seq) {
            let _ = sender.send(sizes);
        }
    }

    fn close(&self) {
        self.alive.store(false, Ordering::SeqCst);
        // Dropping the senders wakes any measurement waiting on the device.
        self.pending.lock().unwrap().clear();
    }
}

impl Host for WsHost {
    fn measure_text(&self, text: &str, font_size: f32, bold: bool, max_width: f32) -> (f32, f32) {
        self.measure_text_batch(&[TextRequest {
            text: text.to_string(),
            font_size,
            bold,
            max_width,
        }])[0]
    }

    fn measure_text_batch(&self, requests: &[TextRequest]) -> Vec<(f32, f32)> {
        let fallback = || {
            requests
                .iter()
                .map(|r| rough_measure(&r.text, r.font_size, r.max_width))
                .collect::<Vec<_>>()
        };
        if requests.is_empty() || !self.alive.load(Ordering::SeqCst) {
            return fallback();
        }

        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        self.pending.lock().unwrap().insert(seq, sender);
        let items: Vec<Value> = requests
            .iter()
            .map(|r| json!([r.text, r.font_size, r.bold, r.max_width]))
            .collect();
        let message = json!({"t": "measure", "seq": seq, "items": items}).to_string();
        if self.out.send(Message::Text(message.into())).is_err() {
            self.pending.lock().unwrap().remove(&seq);
            return fallback();
        }

        match receiver.recv_timeout(MEASURE_TIMEOUT) {
            Ok(sizes) if sizes.len() == requests.len() * 2 => {
                sizes.as_chunks::<2>().0.iter().map(|[w, h]| (*w, *h)).collect()
            }
            _ => {
                self.pending.lock().unwrap().remove(&seq);
                if self.alive.load(Ordering::SeqCst) {
                    eprintln!("devserver: measurement {seq} timed out; using estimates");
                }
                fallback()
            }
        }
    }

    fn emit_ops(&self, bytes: &[u8]) {
        let _ = self.out.send(Message::Binary(bytes.to_vec().into()));
    }
}

struct Session {
    host: Arc<WsHost>,
    runtime: Option<DevRuntime>,
}

impl Session {
    fn handle(&self, text: &str) {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let Some(runtime) = &self.runtime else {
            return;
        };
        match message["t"].as_str() {
            Some("resize") => runtime.resize(
                message["w"].as_f64().unwrap_or(0.0) as f32,
                message["h"].as_f64().unwrap_or(0.0) as f32,
            ),
            Some("event") => {
                let id = message["id"].as_u64().unwrap_or(0) as u32;
                let payload = message["payload"].as_str().unwrap_or("").to_string();
                if let Some(code) = message["code"].as_u64() {
                    runtime.event_code(id, code as u32, payload);
                } else if let Some(kind) = message["kind"].as_str() {
                    runtime.event(id, kind, payload);
                }
            }
            Some("lifecycle") => runtime.lifecycle(message["active"].as_bool().unwrap_or(true)),
            Some("measured") => {
                let seq = message["seq"].as_u64().unwrap_or(u64::MAX);
                let sizes = message["sizes"]
                    .as_array()
                    .map(|values| values.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
                    .unwrap_or_default();
                self.host.answer(seq, sizes);
            }
            _ => {}
        }
    }
}

async fn serve(stream: TcpStream, app: String, store_path: String, current: Arc<Mutex<u64>>, id: u64) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("devserver: handshake with {peer} failed: {error}");
            return;
        }
    };
    *current.lock().unwrap() = id;
    println!("devserver: device connected ({peer}), starting {app}");

    let (mut sink, mut stream) = ws.split();
    let (out, mut outbound) = unbounded_channel::<Message>();
    let host = Arc::new(WsHost {
        out,
        seq: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
    });

    let writer = tokio::spawn(async move {
        while let Some(message) = outbound.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });

    let runtime = DevRuntime::spawn(
        Arc::clone(&host) as Arc<dyn Host>,
        store_path,
        "ws://127.0.0.1:9001",
        app,
    );
    let mut session = Session {
        host: Arc::clone(&host),
        runtime: Some(runtime),
    };

    while let Some(message) = stream.next().await {
        if *current.lock().unwrap() != id {
            println!("devserver: {peer} replaced by a newer connection");
            break;
        }
        match message {
            Ok(Message::Text(text)) => session.handle(text.as_str()),
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    host.close();
    writer.abort();
    // Joining the actor thread blocks; keep it off the async workers.
    let runtime = session.runtime.take();
    let _ = tokio::task::spawn_blocking(move || drop(runtime)).await;
    println!("devserver: device disconnected ({peer})");
}

#[tokio::main]
async fn main() {
    let options = match options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let host = if options.lan { "0.0.0.0" } else { "127.0.0.1" };
    let listener = match TcpListener::bind((host, options.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("devserver: cannot listen on {host}:{}: {error}", options.port);
            std::process::exit(1);
        }
    };
    let store_path = std::env::temp_dir()
        .join(format!("cheetah-dev-{}.redb", options.app))
        .to_string_lossy()
        .into_owned();
    println!(
        "devserver: {} on ws://{host}:{} (Android: adb reverse tcp:{port} tcp:{port})",
        options.app,
        options.port,
        port = options.port
    );

    let current = Arc::new(Mutex::new(0u64));
    let mut next_id = 0u64;
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let _ = stream.set_nodelay(true);
        next_id += 1;
        tokio::spawn(serve(
            stream,
            options.app.clone(),
            store_path.clone(),
            Arc::clone(&current),
            next_id,
        ));
    }
}
