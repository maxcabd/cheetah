//! cheetah-devserver: run a Cheetah app on this machine and render it on a
//! device over WebSocket, so app changes only need a desktop rebuild, or no
//! rebuild at all when running under `dx serve --hotpatch` (see [`Hot`]).
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
use runtime::app::{Action, App, Event, ViewContext};
use runtime::dev::{rough_measure, DevRuntime, Host, RerenderHandle, TextRequest};
use runtime::{Element, NativeCall, Runtime};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

pub use subsecond;

const MEASURE_TIMEOUT: Duration = Duration::from_secs(5);
const GATEWAY_URL: &str = "ws://127.0.0.1:9001";

/// Which app each device connection runs.
#[derive(Clone)]
pub enum AppSource {
    /// An app compiled into the runtime, by name ("nightshade", "chat", ...).
    Named(String),
    /// An app built by the calling crate, e.g. `Runtime::new(Hot(MyApp::new()))`.
    Custom(Arc<dyn Fn() -> Runtime + Send + Sync>),
}

impl AppSource {
    pub fn custom(make: impl Fn() -> Runtime + Send + Sync + 'static) -> Self {
        Self::Custom(Arc::new(make))
    }
}

pub struct Options {
    pub port: u16,
    pub lan: bool,
    /// Remaining arguments the server didn't consume (app-specific flags).
    pub rest: Vec<String>,
}

impl Options {
    /// Parse `--port N` and `--lan` from the process arguments; anything
    /// else is left in `rest`. `--help` prints `usage` and exits.
    pub fn from_args(usage: &str) -> Self {
        let mut options = Options {
            port: 8787,
            lan: false,
            rest: Vec::new(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--port" => match args.next().and_then(|p| p.parse().ok()) {
                    Some(port) => options.port = port,
                    None => fail("--port needs a number"),
                },
                "--lan" => options.lan = true,
                "-h" | "--help" => {
                    println!(
                        "{usage}\n  --port N   listen port (default 8787)\n  \
                         --lan      listen on all interfaces, for a phone on Wi-Fi \
                         (default: localhost only)"
                    );
                    std::process::exit(0);
                }
                _ => options.rest.push(arg),
            }
        }
        options
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}

// ---------------------------------------------------------------------------
// Hot patching (dx serve --hotpatch)
// ---------------------------------------------------------------------------

/// Wraps an app so its `view` and `update` run through Subsecond: under
/// `dx serve --hotpatch`, edits to the app crate are patched into this
/// running process and take effect on the next call, keeping app state.
///
/// Subsecond only patches the crate that owns `main`, so the app's code must
/// be compiled into that crate (see apps/nightshade). Changing the fields of
/// the app's structs can't be patched in place; restart for those.
pub struct Hot<A>(pub A);

impl<A: App> App for Hot<A> {
    fn view(&self, context: &ViewContext) -> Element {
        subsecond::call(|| self.0.view(context))
    }

    fn update(&mut self, event: Event) {
        let mut event = Some(event);
        subsecond::call(|| {
            if let Some(event) = event.take() {
                self.0.update(event);
            }
        })
    }

    fn take_actions(&mut self) -> Vec<Action> {
        self.0.take_actions()
    }

    fn has_queued(&self) -> bool {
        self.0.has_queued()
    }

    fn take_native_calls(&mut self) -> Vec<NativeCall> {
        subsecond::call(|| self.0.take_native_calls())
    }

    fn hot_reloaded(&mut self) {
        self.0.hot_reloaded();
    }
}

/// The connected session, re-rendered after each hot patch.
static CURRENT: Mutex<Option<RerenderHandle>> = Mutex::new(None);

fn install_hot_patching() {
    // Connects only when launched by `dx serve` (it sets the endpoint).
    dioxus_devtools::connect_subsecond();
    subsecond::register_handler(Arc::new(|| {
        if let Some(handle) = CURRENT.lock().unwrap().as_ref() {
            println!("devserver: code patched, re-rendering");
            handle.rerender();
        }
    }));
}

// ---------------------------------------------------------------------------
// Device connection
// ---------------------------------------------------------------------------

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

fn handle(runtime: &DevRuntime, host: &WsHost, text: &str) {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
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
                .map(|values| {
                    values
                        .iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect()
                })
                .unwrap_or_default();
            host.answer(seq, sizes);
        }
        _ => {}
    }
}

async fn serve(stream: TcpStream, source: AppSource, store_path: String, current: Arc<Mutex<u64>>, id: u64) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
    let ws = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("devserver: handshake with {peer} failed: {error}");
            return;
        }
    };
    *current.lock().unwrap() = id;
    println!("devserver: device connected ({peer})");

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

    let dyn_host = Arc::clone(&host) as Arc<dyn Host>;
    let runtime = match source {
        AppSource::Named(app) => DevRuntime::spawn(dyn_host, store_path, GATEWAY_URL, app),
        AppSource::Custom(make) => DevRuntime::spawn_with(dyn_host, GATEWAY_URL, move || make()),
    };
    *CURRENT.lock().unwrap() = runtime.rerender_handle();

    while let Some(message) = stream.next().await {
        if *current.lock().unwrap() != id {
            println!("devserver: {peer} replaced by a newer connection");
            break;
        }
        match message {
            Ok(Message::Text(text)) => handle(&runtime, &host, text.as_str()),
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    if *current.lock().unwrap() == id {
        CURRENT.lock().unwrap().take();
    }
    host.close();
    writer.abort();
    // Joining the actor thread blocks; keep it off the async workers.
    let _ = tokio::task::spawn_blocking(move || drop(runtime)).await;
    println!("devserver: device disconnected ({peer})");
}

/// Serve `source` to devices until the process is stopped.
pub fn run(name: &str, options: Options, source: AppSource) {
    install_hot_patching();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async move {
        let host = if options.lan { "0.0.0.0" } else { "127.0.0.1" };
        let listener = match TcpListener::bind((host, options.port)).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("devserver: cannot listen on {host}:{}: {error}", options.port);
                std::process::exit(1);
            }
        };
        let store_path = std::env::temp_dir()
            .join(format!("cheetah-dev-{name}.redb"))
            .to_string_lossy()
            .into_owned();
        println!(
            "devserver: {name} on ws://{host}:{port} (Android: adb reverse tcp:{port} tcp:{port})",
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
                source.clone(),
                store_path.clone(),
                Arc::clone(&current),
                next_id,
            ));
        }
    });
}
