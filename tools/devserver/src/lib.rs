//! cheetah-devserver: run a Cheetah app on this machine and render it on a
//! device over WebSocket, so app changes only need a desktop rebuild, or no
//! rebuild at all when running under `dx serve --hot-patch` (see [`Hot`]).
//!
//! The device host connects in dev mode (Android: `--es devServer
//! ws://localhost:8787` with `adb reverse tcp:8787 tcp:8787`). The protocol
//! mirrors the FFI boundary:
//!
//! device -> server (JSON text):
//!   {"t":"hello","platform":"ios","native":"ios-0123…"}   first, from dev builds
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
use runtime::dev::{
    rough_measure, DevRuntime, Host, LiteralOverrides, RerenderHandle, TextRequest, Views,
};
use runtime::{Element, NativeCall, Runtime};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

pub use subsecond;

pub mod live;

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
    /// Source folders whose literal-only edits are applied live (no
    /// rebuild): the app crate's `src`. Empty disables live edits.
    pub live: Vec<PathBuf>,
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
            live: Vec::new(),
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
// Hot patching (dx serve --hot-patch)
// ---------------------------------------------------------------------------

/// Wraps an app so its `view` and `update` run through Subsecond: under
/// `dx serve --hot-patch`, edits to the app crate are patched into this
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

    fn views(&self) -> Vec<(&'static str, &'static str)> {
        self.0.views()
    }

    fn take_views(&mut self) -> Option<Vec<(String, String)>> {
        self.0.take_views()
    }
}

/// The connected session, re-rendered after each hot patch.
static CURRENT: Mutex<Option<RerenderHandle>> = Mutex::new(None);

fn install_hot_patching() {
    // Connects only when launched by `dx serve` (it sets the endpoint).
    dioxus_devtools::connect_subsecond();
    subsecond::register_handler(Arc::new(|| {
        // The patched code now contains every saved literal: start over.
        rebase_live_edits();
        match CURRENT.lock().unwrap().as_ref() {
            Some(handle) => {
                println!("devserver: code patched, re-rendering");
                handle.rerender();
            }
            None => println!("devserver: code patched (no device connected)"),
        }
    }));
}

// ---------------------------------------------------------------------------
// Live literal edits
// ---------------------------------------------------------------------------

/// Literal overrides currently in effect, re-sent to new sessions.
static LIVE: Mutex<LiveState> = Mutex::new(LiveState::empty());

struct LiveState {
    /// Source of each file as compiled into the running binary.
    baseline: Option<HashMap<PathBuf, String>>,
    /// Per file: literal edits relative to the baseline.
    edits: Option<HashMap<PathBuf, Vec<live::Located>>>,
    overrides: Option<LiteralOverrides>,
    /// Latest `.view` templates parsed from disk, sent to every session.
    views: Option<Views>,
}

impl LiveState {
    const fn empty() -> Self {
        Self {
            baseline: None,
            edits: None,
            overrides: None,
            views: None,
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn snapshot(dirs: &[PathBuf]) -> HashMap<PathBuf, String> {
    let mut files = Vec::new();
    for dir in dirs {
        rust_files(dir, &mut files);
    }
    files
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(&path).ok().map(|text| (path, text)))
        .collect()
}

/// Workspace-relative path with `/`, as rustc records it for the binary.
fn source_key(path: &Path) -> String {
    let root = workspace_root();
    let relative = path
        .canonicalize()
        .ok()
        .and_then(|full| {
            root.canonicalize()
                .ok()
                .and_then(|r| full.strip_prefix(r).ok().map(Path::to_path_buf))
        })
        .unwrap_or_else(|| path.to_path_buf());
    relative.to_string_lossy().replace('\\', "/")
}

fn rebuild_overrides(edits: &HashMap<PathBuf, Vec<live::Located>>) -> LiteralOverrides {
    let keyed: Vec<(String, &Vec<live::Located>)> = edits
        .iter()
        .map(|(path, located)| (source_key(path), located))
        .collect();
    live::overrides(
        keyed
            .iter()
            .map(|(key, located)| (key.as_str(), located.as_slice())),
    )
}

fn rebase_live_edits() {
    let mut state = LIVE.lock().unwrap();
    if let Some(baseline) = state.baseline.as_mut() {
        for (path, text) in baseline.iter_mut() {
            if let Ok(current) = std::fs::read_to_string(path) {
                *text = current;
            }
        }
    }
    state.edits = Some(HashMap::new());
    state.overrides = Some(LiteralOverrides::default());
}

fn view_files(dirs: &[PathBuf]) -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "view") {
                out.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    for dir in dirs {
        walk(dir, &mut paths);
    }
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            std::fs::read_to_string(&path)
                .ok()
                .map(|text| (source_key(&path), text))
        })
        .collect()
}

fn parse_views(files: &[(String, String)]) -> Result<Views, runtime::dev::ViewError> {
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    Views::parse(&refs)
}

fn current_views() -> Option<Views> {
    LIVE.lock().unwrap().views.clone()
}

fn current_overrides() -> Option<LiteralOverrides> {
    LIVE.lock()
        .unwrap()
        .overrides
        .clone()
        .filter(|overrides| !overrides.is_empty())
}

/// Watch `dirs` and apply literal-only edits to the running app.
fn watch_live_edits(dirs: Vec<PathBuf>) {
    let initial_views = view_files(&dirs);
    {
        let mut state = LIVE.lock().unwrap();
        state.baseline = Some(snapshot(&dirs));
        state.edits = Some(HashMap::new());
        state.overrides = Some(LiteralOverrides::default());
        // What's on disk wins over what the binary embedded.
        match parse_views(&initial_views) {
            Ok(views) => state.views = Some(views),
            Err(error) => println!("devserver: view error: {error}"),
        }
    }
    std::thread::Builder::new()
        .name("cheetah-live".into())
        .spawn(move || {
            let mut seen: HashMap<PathBuf, String> = snapshot(&dirs);
            let mut seen_views = initial_views;
            loop {
                std::thread::sleep(Duration::from_millis(60));

                // .view templates: re-parse and swap into the running app.
                let views_now = view_files(&dirs);
                if views_now != seen_views {
                    let started = Instant::now();
                    seen_views = views_now;
                    match parse_views(&seen_views) {
                        Ok(views) => {
                            LIVE.lock().unwrap().views = Some(views.clone());
                            if let Some(handle) = CURRENT.lock().unwrap().as_ref() {
                                handle.set_views(views);
                            }
                            println!(
                                "devserver: views updated in {:.1} ms, no rebuild",
                                started.elapsed().as_secs_f64() * 1000.0
                            );
                        }
                        // Keep the last good views on screen.
                        Err(error) => println!("devserver: view error: {error}"),
                    }
                }

                let now = snapshot(&dirs);
                let changed: Vec<PathBuf> = now
                    .iter()
                    .filter(|(path, text)| seen.get(*path) != Some(*text))
                    .map(|(path, _)| path.clone())
                    .collect();
                if changed.is_empty() {
                    continue;
                }
                seen = now;

                let mut state = LIVE.lock().unwrap();
                let mut applied = 0usize;
                for path in &changed {
                    let current = seen.get(path).cloned().unwrap_or_default();
                    let baseline = state.baseline.as_ref().and_then(|b| b.get(path)).cloned();
                    let name = source_key(path);
                    let Some(baseline) = baseline else {
                        println!("devserver: new file {name}; needs a rebuild");
                        continue;
                    };
                    let edits = state.edits.get_or_insert_with(HashMap::new);
                    match live::diff(&baseline, &current) {
                        live::Change::Unchanged => {
                            edits.remove(path);
                            applied += 1;
                        }
                        live::Change::Literals(located) => {
                            let count: usize = located.iter().map(|l| l.edits.len()).sum();
                            println!(
                                "devserver: live edit in {name} ({count} value(s)), no rebuild"
                            );
                            edits.insert(path.clone(), located);
                            applied += 1;
                        }
                        live::Change::Code(reason) => {
                            println!(
                                "devserver: code change in {name} ({reason}); waiting for rebuild"
                            );
                        }
                    }
                }
                if applied == 0 {
                    continue;
                }
                let overrides = rebuild_overrides(state.edits.as_ref().unwrap());
                state.overrides = Some(overrides.clone());
                drop(state);
                if let Some(handle) = CURRENT.lock().unwrap().as_ref() {
                    handle.set_literals(overrides);
                }
            }
        })
        .expect("live edit watcher");
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
    /// Sizes the device already measured. Each measurement is a network
    /// round trip that blocks layout, so every text is measured once per
    /// connection, not on every render.
    measured: Mutex<HashMap<MeasureKey, (f32, f32)>>,
}

/// Text, font size and max width (as bits, so they hash), bold.
type MeasureKey = (String, u32, bool, u32);

fn measure_key(request: &TextRequest) -> MeasureKey {
    (
        request.text.clone(),
        request.font_size.to_bits(),
        request.bold,
        request.max_width.to_bits(),
    )
}

/// Round trips slower than this are logged (they show as UI lag).
const SLOW_MEASURE: Duration = Duration::from_millis(60);

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
        // Only ask the device about texts it hasn't measured yet.
        let mut sizes: Vec<Option<(f32, f32)>> = {
            let measured = self.measured.lock().unwrap();
            requests
                .iter()
                .map(|request| measured.get(&measure_key(request)).copied())
                .collect()
        };
        let missing: Vec<TextRequest> = requests
            .iter()
            .zip(&sizes)
            .filter(|(_, size)| size.is_none())
            .map(|(request, _)| request.clone())
            .collect();
        if missing.is_empty() {
            return sizes.into_iter().map(Option::unwrap_or_default).collect();
        }
        let started = Instant::now();
        let fresh = self.measure_remote(&missing);
        let elapsed = started.elapsed();
        if elapsed > SLOW_MEASURE {
            println!(
                "devserver: measuring {} texts on the device took {:.0} ms",
                missing.len(),
                elapsed.as_secs_f64() * 1000.0
            );
        }
        let mut fresh = fresh.into_iter();
        let mut measured = self.measured.lock().unwrap();
        for (request, size) in requests.iter().zip(sizes.iter_mut()) {
            if size.is_none() {
                let value = fresh.next().unwrap_or_default();
                measured.insert(measure_key(request), value);
                *size = Some(value);
            }
        }
        sizes.into_iter().map(Option::unwrap_or_default).collect()
    }

    fn emit_ops(&self, bytes: &[u8]) {
        let _ = self.out.send(Message::Binary(bytes.to_vec().into()));
    }
}

impl WsHost {
    /// One round trip: the device measures `requests` with its fonts.
    fn measure_remote(&self, requests: &[TextRequest]) -> Vec<(f32, f32)> {
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
            Ok(sizes) if sizes.len() == requests.len() * 2 => sizes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[w, h]| (*w, *h))
                .collect(),
            _ => {
                self.pending.lock().unwrap().remove(&seq);
                if self.alive.load(Ordering::SeqCst) {
                    eprintln!("devserver: measurement {seq} timed out; using estimates");
                }
                fallback()
            }
        }
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
        Some("hello") => println!(
            "devserver: {}",
            native_status(
                message["platform"].as_str().unwrap_or(""),
                message["native"].as_str().unwrap_or("unknown"),
            )
        ),
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

/// Whether the connected dev build has this checkout's native code. Rust and
/// `.view` changes run here; only Swift/Kotlin changes need a new dev build.
/// The app this server runs; its cheetah.toml and native modules are part
/// of the dev build's native fingerprint.
static APP_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn native_status(platform: &str, installed: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let app = APP_NAME.get().map(String::as_str);
    let Some(local) = cheetah_cli::fingerprint::native(&root, platform, app) else {
        return format!("{platform} dev build connected ({installed}); no git checkout to compare");
    };
    if installed == local {
        format!("{platform} dev build is current ({local})")
    } else if installed == "unknown" || installed.is_empty() {
        format!(
            "{platform} dev build doesn't report its native code version; if native code changed, rebuild it: cheetah build {platform} --dev"
        )
    } else {
        format!(
            "NATIVE CODE CHANGED since this {platform} dev build (installed {installed}, local {local}); new native APIs won't work until you rebuild: cheetah build {platform} --dev"
        )
    }
}

async fn serve(
    stream: TcpStream,
    source: AppSource,
    store_path: String,
    current: Arc<Mutex<u64>>,
    id: u64,
) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();
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
        measured: Mutex::new(HashMap::new()),
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
    // Live edits made before this connection still differ from the binary.
    if let Some(overrides) = current_overrides() {
        runtime.set_literals(overrides);
    }
    if let Some(views) = current_views() {
        runtime.set_views(views);
    }

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
    let _ = APP_NAME.set(name.to_string());
    install_hot_patching();
    if !options.live.is_empty() {
        watch_live_edits(options.live.clone());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async move {
        let host = if options.lan { "0.0.0.0" } else { "127.0.0.1" };
        let listener = match TcpListener::bind((host, options.port)).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!(
                    "devserver: cannot listen on {host}:{}: {error}",
                    options.port
                );
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

#[cfg(test)]
mod tests {
    use super::native_status;

    #[test]
    fn says_when_the_dev_build_needs_rebuilding() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let local = cheetah_cli::fingerprint::native(&root, "ios", None).expect("git checkout");
        assert!(native_status("ios", &local).contains("is current"));
        assert!(native_status("ios", "ios-0000000000000000").contains("NATIVE CODE CHANGED"));
        assert!(native_status("ios", "unknown").contains("cheetah build ios --dev"));
    }
}
