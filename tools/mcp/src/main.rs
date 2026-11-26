//! `cheetah-mcp`: Cheetah's tools for agents, over the Model Context
//! Protocol (JSON-RPC, one message per line on stdin and stdout).
//!
//!   render     the app's screen as an image, after taps and typing, with
//!              what's on it (keys to tap, text, frames)
//!   elements   only what's on it: cheaper than an image
//!   test       the app's tests, with what failed
//!   metrics    what each frame cost in the dev server's last session
//!   doctor     which tools each command needs, and what's missing
//!
//! And the real app on a phone or simulator, through a dev server on any
//! machine (`cheetah dev`; its `/agent` endpoint), by key, as the user would:
//!
//!   device_elements, device_screen, device_tap, device_type, device_event
//!   device_screenshot   a true screenshot, from a device on this machine
//!                       (adb, or the iOS simulator on a Mac)
//!
//! Each call runs the current source through cargo (`cheetah-render`, the
//! `cheetah` CLI), so it reflects an edit made a moment before; a compile
//! error comes back as the tool's error, which is what an agent needs then.
//! Nothing else is ever written to stdout: cargo's own output is captured.

use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Tool text beyond this is cut, keeping the end, where errors and test
/// summaries are.
const MAX_TEXT: usize = 12_000;

/// What a command printed and whether it succeeded.
pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs cargo and talks to dev servers; tests swap it for canned answers.
pub trait Runner {
    fn cargo(&self, args: &[String]) -> Output;
    /// One request to a dev server's `/agent`, and its reply.
    fn agent(&self, server: &str, request: &Value) -> Result<Value, String>;
}

pub struct Cargo {
    root: PathBuf,
}

impl Runner for Cargo {
    fn cargo(&self, args: &[String]) -> Output {
        match Command::new("cargo")
            .current_dir(&self.root)
            .args(args)
            .output()
        {
            Ok(output) => Output {
                ok: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            },
            Err(error) => Output {
                ok: false,
                stdout: String::new(),
                stderr: format!("cannot run cargo: {error}"),
            },
        }
    }

    fn agent(&self, server: &str, request: &Value) -> Result<Value, String> {
        let address = agent_url(server, request["token"].as_str().unwrap_or(""));
        // Messages name the server, never its token.
        let url = address.split('?').next().unwrap_or("").to_string();
        let mut request = request.clone();
        if let Some(fields) = request.as_object_mut() {
            fields.remove("token");
        }
        let (mut socket, _) = tungstenite::connect(address.as_str()).map_err(|error| match error {
            tungstenite::Error::Http(response) if response.status() == 401 => format!(
                "{url} wants its agent token: pass `token` (cheetah dev prints it) or set CHEETAH_AGENT_TOKEN"
            ),
            error => format!("cannot reach the dev server at {url}: {error}"),
        })?;
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
        }
        socket
            .send(tungstenite::Message::text(request.to_string()))
            .map_err(|error| error.to_string())?;
        loop {
            match socket
                .read()
                .map_err(|error| format!("no answer from {url}: {error}"))?
            {
                tungstenite::Message::Text(text) => {
                    let _ = socket.close(None);
                    return serde_json::from_str(text.as_str()).map_err(|error| error.to_string());
                }
                tungstenite::Message::Close(_) => {
                    return Err(format!("{url} closed the connection"))
                }
                _ => {}
            }
        }
    }
}

/// The dev server to drive (as given, else CHEETAH_DEV_SERVER, else this
/// machine's), with its token: as given, else CHEETAH_AGENT_TOKEN, else the
/// one a dev server on this machine wrote for its port.
fn agent_url(server: &str, token: &str) -> String {
    let server = if server.is_empty() {
        std::env::var("CHEETAH_DEV_SERVER").unwrap_or_else(|_| "ws://127.0.0.1:8787".into())
    } else {
        server.to_string()
    };
    let server = if server.contains("://") {
        server
    } else {
        format!("ws://{server}")
    };
    let base = server.trim_end_matches('/').to_string();
    let token = Some(token.to_string())
        .filter(|token| !token.is_empty())
        .or_else(|| {
            std::env::var("CHEETAH_AGENT_TOKEN")
                .ok()
                .filter(|token| !token.is_empty())
        })
        .or_else(|| local_token(&base));
    match token {
        Some(token) => format!("{base}/agent?token={token}"),
        None => format!("{base}/agent"),
    }
}

/// The token a dev server on this machine wrote for its port
/// (cheetah-devserver's `token_file`).
fn local_token(base: &str) -> Option<String> {
    let host_port = base.split("://").nth(1)?;
    let (host, port) = host_port.rsplit_once(':')?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return None;
    }
    let file = std::env::temp_dir().join(format!("cheetah-agent-token-{port}"));
    let token = std::fs::read_to_string(file).ok()?;
    Some(token.trim().to_string())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tools() -> Value {
    let app = json!({
        "type": "string",
        "description": "The app: nightshade, or nightshade-demo (sample results already showing).",
        "default": "nightshade"
    });
    let steps = json!({
        "type": "array",
        "description": "Done in order before the screen is drawn, each on the screen the one before left. Timers then fire at once, so animations have ended.",
        "items": {
            "type": "object",
            "properties": {
                "tap": {"type": "string", "description": "The key of an element to tap (from `elements`)."},
                "type": {"type": "string", "description": "The key of an input to type into; the text goes in `text`."},
                "text": {"type": "string"},
                "event": {"type": "string", "description": "Any event kind (submit, select, gesture...) for `key`, with `payload`."},
                "key": {"type": "string"},
                "payload": {"type": "string"}
            }
        }
    });
    let size = |default: f64, description: &str| json!({"type": "number", "default": default, "description": description});
    let server = json!({
        "type": "string",
        "description": "The dev server: ws://host:port. Default: CHEETAH_DEV_SERVER, else ws://127.0.0.1:8787."
    });
    let token = json!({
        "type": "string",
        "description": "The dev server's agent token, which `cheetah dev` prints. Default: CHEETAH_AGENT_TOKEN, else the one a dev server on this machine wrote."
    });
    json!([
        {
            "name": "render",
            "title": "Render the app's screen",
            "description": "Draw the app's screen as a PNG on this machine, from its own Rust and .view files and the runtime's layout, as a phone would; no device or simulator. Returns the image and the elements on it (keys to tap, text, frames in points). Native views are stand-ins: icons are line drawings, the map is a grid, web photos are gray.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": app,
                    "steps": steps,
                    "width": size(393.0, "Screen width in points (iPhone 16: 393)."),
                    "height": size(852.0, "Screen height in points (iPhone 16: 852)."),
                    "scale": size(1.0, "Pixels per point. 1 keeps the image small; 2 or 3 for detail."),
                    "glass": {"type": "boolean", "default": true, "description": "Liquid Glass, as on iOS 26."}
                }
            }
        },
        {
            "name": "elements",
            "title": "List what's on the screen",
            "description": "The elements on the app's screen after the steps, without an image: key, kind, text or label, frame [x, y, width, height] in points, and whether it can be tapped. Use the keys with render's steps.",
            "inputSchema": {
                "type": "object",
                "properties": {"app": app, "steps": steps}
            }
        },
        {
            "name": "test",
            "title": "Run the app's tests",
            "description": "cargo test for the app (its views render in tests, without a device). Returns the summary, and what failed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "package": {"type": "string", "default": "nightshade", "description": "The cargo package: nightshade, runtime, cheetah-render..."},
                    "filter": {"type": "string", "description": "Only tests whose names contain this."}
                }
            }
        },
        {
            "name": "device_elements",
            "title": "List what's on the connected device",
            "description": "What the app on the device connected to a dev server (`cheetah dev`) is showing: keys, text, frames, what can be tapped. The server can be on another machine.",
            "inputSchema": {"type": "object", "properties": {"server": server.clone(), "token": token.clone()}}
        },
        {
            "name": "device_screen",
            "title": "See the connected device's screen",
            "description": "The live screen of the app on the device connected to a dev server, painted from its live tree (native views are stand-ins; for a true screenshot of a device on this machine, use device_screenshot), with what's on it.",
            "inputSchema": {"type": "object", "properties": {"server": server.clone(), "token": token.clone(), "scale": size(1.0, "Pixels per point.")}}
        },
        {
            "name": "device_tap",
            "title": "Tap on the connected device",
            "description": "Tap the element with `key` in the app on the device connected to a dev server; the device updates as if touched. Keys come from device_elements.",
            "inputSchema": {"type": "object", "properties": {"key": {"type": "string"}, "server": server.clone(), "token": token.clone()}, "required": ["key"]}
        },
        {
            "name": "device_type",
            "title": "Type on the connected device",
            "description": "Set the text of the input with `key` in the app on the device connected to a dev server.",
            "inputSchema": {"type": "object", "properties": {"key": {"type": "string"}, "text": {"type": "string"}, "server": server.clone(), "token": token.clone()}, "required": ["key", "text"]}
        },
        {
            "name": "device_event",
            "title": "Send an event on the connected device",
            "description": "Any event (submit, select, gesture...) for the element with `key`, in the app on the device connected to a dev server.",
            "inputSchema": {"type": "object", "properties": {"kind": {"type": "string"}, "key": {"type": "string"}, "payload": {"type": "string"}, "server": server.clone(), "token": token.clone()}, "required": ["kind", "key"]}
        },
        {
            "name": "device_screenshot",
            "title": "Screenshot a device on this machine",
            "description": "A true screenshot: an Android device or emulator over adb, or the booted iOS simulator (macOS only).",
            "inputSchema": {"type": "object", "properties": {"platform": {"type": "string", "enum": ["android", "ios"], "default": "android"}}}
        },
        {
            "name": "metrics",
            "title": "Frame costs",
            "description": "What each frame cost in the dev server's current or last session (`cheetah metrics`), by what caused it.",
            "inputSchema": {"type": "object", "properties": {"app": app}}
        },
        {
            "name": "doctor",
            "title": "Check the tools",
            "description": "Which tools each cheetah command needs, and which are missing (`cheetah doctor`).",
            "inputSchema": {"type": "object", "properties": {}}
        }
    ])
}

/// The last JSON line a command printed (cheetah tools print one).
fn last_json(stdout: &str) -> Option<Value> {
    stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str(line.trim()).ok())
}

/// Keep the end of long text: errors and summaries come last.
fn trim(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return text.to_string();
    }
    let mut start = text.len() - MAX_TEXT;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("[… cut …]\n{}", &text[start..])
}

fn text(content: impl Into<String>) -> Value {
    json!({"type": "text", "text": content.into()})
}

fn failure(message: impl Into<String>) -> Value {
    json!({"content": [text(message)], "isError": true})
}

/// render's command line from a call's arguments.
fn render_args(arguments: &Value, out: &Path, tree: bool) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = ["run", "-q", "-p", "cheetah-render", "--"]
        .map(String::from)
        .to_vec();
    args.extend([
        "--app".into(),
        arguments["app"]
            .as_str()
            .unwrap_or("nightshade")
            .to_string(),
    ]);
    for (name, flag) in [
        ("width", "--width"),
        ("height", "--height"),
        ("scale", "--scale"),
    ] {
        if let Some(number) = arguments[name].as_f64() {
            args.extend([flag.to_string(), number.to_string()]);
        }
    }
    if arguments.get("scale").is_none() {
        args.extend(["--scale".into(), "1".into()]);
    }
    if arguments["glass"] == false {
        args.push("--no-glass".into());
    }
    for step in arguments["steps"].as_array().into_iter().flatten() {
        let field = |name: &str| step[name].as_str().map(str::to_string);
        if let Some(key) = field("tap") {
            args.extend(["--tap".into(), key]);
        } else if let Some(key) = field("type") {
            args.extend(["--type".into(), key, field("text").unwrap_or_default()]);
        } else if let Some(kind) = field("event") {
            let key = field("key").ok_or("an event step needs a key")?;
            args.extend([
                "--event".into(),
                kind,
                key,
                field("payload").unwrap_or_default(),
            ]);
        } else {
            return Err(format!("a step needs tap, type or event: {step}"));
        }
    }
    if tree {
        args.push("--tree".into());
    }
    args.extend(["--out".into(), out.display().to_string()]);
    Ok(args)
}

/// 314.4, not 314.3999938964844 (the frames were f32).
fn points(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    }
}

/// One element per line: what an agent reads to pick the next tap.
fn element_lines(report: &Value) -> String {
    let mut lines =
        vec!["key | kind | text or label | [x, y, width, height] | tappable".to_string()];
    for element in report["elements"].as_array().into_iter().flatten() {
        let shown = element["text"]
            .as_str()
            .or(element["label"].as_str())
            .unwrap_or("");
        let frame = element["frame"].as_array().map(|frame| {
            frame
                .iter()
                .map(|n| points(n.as_f64().unwrap_or(0.0)))
                .collect::<Vec<_>>()
                .join(", ")
        });
        lines.push(format!(
            "{} | {} | {} | [{}] |{}",
            element["key"].as_str().unwrap_or("-"),
            element["native"]
                .as_str()
                .unwrap_or(element["kind"].as_str().unwrap_or("")),
            if shown.is_empty() {
                "-".to_string()
            } else {
                format!("{shown:?}")
            },
            frame.unwrap_or_default(),
            if element["pressable"] == true {
                " tap"
            } else {
                ""
            },
        ));
    }
    lines.join("\n")
}

fn render(runner: &dyn Runner, arguments: &Value, image: bool) -> Value {
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!("cheetah-mcp-{}-{call}.png", std::process::id()));
    let args = match render_args(arguments, &out, true) {
        Ok(args) => args,
        Err(error) => return failure(error),
    };
    let output = runner.cargo(&args);
    let Some(report) = last_json(&output.stdout) else {
        // It didn't build: the compiler's errors are the answer.
        return failure(trim(&format!(
            "cheetah-render didn't run:\n{}",
            output.stderr
        )));
    };
    if report["ok"] != true {
        return failure(
            report["error"]
                .as_str()
                .unwrap_or("render failed")
                .to_string(),
        );
    }
    let mut content = Vec::new();
    if image {
        match std::fs::read(&out) {
            Ok(png) => content.push(json!({
                "type": "image",
                "data": base64::engine::general_purpose::STANDARD.encode(png),
                "mimeType": "image/png"
            })),
            Err(error) => return failure(format!("{}: {error}", out.display())),
        }
    }
    let _ = std::fs::remove_file(&out);
    content.push(text(element_lines(&report)));
    json!({"content": content, "isError": false})
}

fn test(runner: &dyn Runner, arguments: &Value) -> Value {
    let package = arguments["package"]
        .as_str()
        .unwrap_or("nightshade")
        .to_string();
    let mut args: Vec<String> = vec!["test".into(), "-p".into(), package];
    if let Some(filter) = arguments["filter"]
        .as_str()
        .filter(|filter| !filter.is_empty())
    {
        args.extend(["--".into(), filter.to_string()]);
    }
    let output = runner.cargo(&args);
    // The summary and failures from stdout; compile errors from stderr.
    let mut report: Vec<&str> = output
        .stdout
        .lines()
        .filter(|line| {
            line.starts_with("test result")
                || line.contains("FAILED")
                || line.contains("panicked")
                || line.starts_with("---- ")
                || line.trim_start().starts_with("left:")
                || line.trim_start().starts_with("right:")
                || line.starts_with("assertion")
        })
        .collect();
    if !output.ok && report.iter().all(|line| !line.contains("FAILED")) {
        report.push(output.stderr.as_str());
    }
    let summary = if report.is_empty() {
        "No tests ran.".to_string()
    } else {
        report.join("\n")
    };
    json!({"content": [text(trim(&summary))], "isError": !output.ok})
}

fn cli(runner: &dyn Runner, command: &[&str]) -> Value {
    let mut args: Vec<String> = ["run", "-q", "-p", "cheetah-cli", "--bin", "cheetah", "--"]
        .map(String::from)
        .to_vec();
    args.extend(command.iter().map(|part| part.to_string()));
    args.push("--json".into());
    let output = runner.cargo(&args);
    let shown = if output.stdout.trim().is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    json!({"content": [text(trim(&shown))], "isError": !output.ok})
}

/// A dev server's answer as a tool result: its PNG as an image, its
/// elements as lines.
fn device(runner: &dyn Runner, arguments: &Value, mut request: Value) -> Value {
    let server = arguments["server"].as_str().unwrap_or("");
    // For the client, which puts it on the URL; it isn't sent on.
    if let Some(token) = arguments["token"].as_str() {
        request["token"] = token.into();
    }
    let reply = match runner.agent(server, &request) {
        Ok(reply) => reply,
        Err(error) => {
            return failure(format!(
                "{error}\nIs `cheetah dev` running there, with the dev build connected?"
            ))
        }
    };
    if reply["ok"] != true {
        return failure(
            reply["error"]
                .as_str()
                .unwrap_or("the dev server said no")
                .to_string(),
        );
    }
    let mut content = Vec::new();
    if let Some(png) = reply["png"].as_str() {
        content.push(json!({"type": "image", "data": png, "mimeType": "image/png"}));
    }
    if reply.get("elements").is_some() {
        content.push(text(element_lines(&reply)));
    } else {
        content.push(text("done; the device shows the result"));
    }
    json!({"content": content, "isError": false})
}

/// A real screenshot from a device or simulator on this machine.
fn screenshot(runner: &dyn Runner, arguments: &Value) -> Value {
    let platform = arguments["platform"].as_str().unwrap_or("android");
    if platform != "android" && platform != "ios" {
        return failure("platform: android or ios");
    }
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = std::env::temp_dir().join(format!(
        "cheetah-mcp-shot-{}-{call}.png",
        std::process::id()
    ));
    let args: Vec<String> = [
        "run",
        "-q",
        "-p",
        "cheetah-drive",
        "--",
        platform,
        "screenshot",
    ]
    .map(String::from)
    .into_iter()
    .chain([out.display().to_string()])
    .collect();
    let output = runner.cargo(&args);
    let png = match std::fs::read(&out) {
        Ok(png) if output.ok => png,
        _ => {
            return failure(trim(&format!(
                "no screenshot from {platform}:\n{}{}",
                output.stdout, output.stderr
            )))
        }
    };
    let _ = std::fs::remove_file(&out);
    json!({"content": [{
        "type": "image",
        "data": base64::engine::general_purpose::STANDARD.encode(png),
        "mimeType": "image/png"
    }], "isError": false})
}

fn call(runner: &dyn Runner, params: &Value) -> Value {
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match params["name"].as_str().unwrap_or("") {
        "render" => render(runner, &arguments, true),
        "elements" => render(runner, &arguments, false),
        "test" => test(runner, &arguments),
        "metrics" => {
            let app = arguments["app"]
                .as_str()
                .unwrap_or("nightshade")
                .to_string();
            cli(runner, &["metrics", "--app", &app])
        }
        "doctor" => cli(runner, &["doctor"]),
        "device_elements" => device(runner, &arguments, json!({"id": 1, "t": "elements"})),
        "device_screen" => device(
            runner,
            &arguments,
            json!({"id": 1, "t": "screen", "scale": arguments["scale"].as_f64().unwrap_or(1.0)}),
        ),
        "device_tap" => device(
            runner,
            &arguments,
            json!({"id": 1, "t": "tap", "key": arguments["key"]}),
        ),
        "device_type" => device(
            runner,
            &arguments,
            json!({"id": 1, "t": "type", "key": arguments["key"], "text": arguments["text"]}),
        ),
        "device_event" => device(
            runner,
            &arguments,
            json!({
                "id": 1,
                "t": "event",
                "kind": arguments["kind"],
                "key": arguments["key"],
                "payload": arguments["payload"].as_str().unwrap_or("")
            }),
        ),
        "device_screenshot" => screenshot(runner, &arguments),
        other => failure(format!("no tool named {other}")),
    }
}

/// The reply to one message, if it wants one (notifications don't).
pub fn handle(runner: &dyn Runner, message: &Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match message["method"].as_str().unwrap_or("") {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == asked)
                .copied()
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "cheetah", "title": "Cheetah", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Cheetah apps are Rust with .view files. To see a change, call `render` (an image) or `elements` (what's on screen, with keys); tap by key in `steps`. `test` runs the app's tests, where its views render too. With a phone connected to `cheetah dev`, the device_* tools see and tap the real app."
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => call(runner, &params),
        method => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {method}")}
            }))
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn main() {
    let runner = Cargo { root: repo_root() };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle(&runner, &message),
            Err(error) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {"code": -32700, "message": format!("parse error: {error}")}
            })),
        };
        if let Some(reply) = reply {
            if writeln!(stdout, "{reply}")
                .and_then(|_| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers every cargo call with `output`, and remembers the calls.
    struct Canned {
        output: fn(&[String]) -> Output,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl Runner for Canned {
        fn cargo(&self, args: &[String]) -> Output {
            self.calls.borrow_mut().push(args.to_vec());
            (self.output)(args)
        }

        fn agent(&self, server: &str, request: &Value) -> Result<Value, String> {
            self.calls
                .borrow_mut()
                .push(vec![server.to_string(), request.to_string()]);
            match request["t"].as_str() {
                Some("screen") => Ok(json!({"ok": true, "png": "iVBORw0KGgo=", "elements": [
                    {"kind": "text", "key": "more", "text": "more", "frame": [0.0, 22.0, 60.0, 22.0], "pressable": true}
                ]})),
                Some("tap") if request["key"] == "nope" => {
                    Ok(json!({"ok": false, "error": "nothing on screen with key nope"}))
                }
                Some(_) => Ok(json!({"ok": true})),
                None => Err("cannot reach".into()),
            }
        }
    }

    fn canned(output: fn(&[String]) -> Output) -> Canned {
        Canned {
            output,
            calls: RefCell::new(Vec::new()),
        }
    }

    fn request(method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params})
    }

    fn rendered(args: &[String]) -> Output {
        let out = args
            .iter()
            .position(|arg| arg == "--out")
            .map(|index| args[index + 1].clone())
            .unwrap();
        std::fs::write(&out, b"\x89PNG fake").unwrap();
        Output {
            ok: true,
            stdout: json!({"ok": true, "png": out, "size": [393, 852], "elements": [
                {"kind": "glass", "key": "drawer:open", "label": "Menu", "frame": [16.0, 95.0, 48.0, 48.0], "pressable": true},
                {"kind": "text", "text": "Ready to discover?", "frame": [24.0, 560.0, 345.0, 26.0]}
            ]})
            .to_string(),
            stderr: "   Compiling cheetah-render\n".into(),
        }
    }

    #[test]
    fn initializes_and_lists_its_tools() {
        let runner = canned(rendered);
        let reply = handle(
            &runner,
            &request("initialize", json!({"protocolVersion": "2025-03-26"})),
        )
        .unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(
            reply["result"]["protocolVersion"], "2025-03-26",
            "the client's version when known"
        );
        assert!(reply["result"]["capabilities"]["tools"].is_object());

        assert!(handle(
            &runner,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none());
        let tools = handle(&runner, &request("tools/list", json!({}))).unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "render",
                "elements",
                "test",
                "device_elements",
                "device_screen",
                "device_tap",
                "device_type",
                "device_event",
                "device_screenshot",
                "metrics",
                "doctor"
            ]
        );
        let unknown = handle(&runner, &request("resources/list", json!({}))).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn render_returns_the_image_and_what_is_on_it() {
        let runner = canned(rendered);
        let reply = handle(
            &runner,
            &request("tools/call", json!({"name": "render", "arguments": {
                "app": "nightshade-demo",
                "steps": [{"tap": "drawer:open"}, {"type": "search", "text": "sushi"}, {"event": "submit", "key": "search"}]
            }})),
        )
        .unwrap();
        let result = &reply["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["type"], "image");
        assert_eq!(result["content"][0]["mimeType"], "image/png");
        let listing = result["content"][1]["text"].as_str().unwrap();
        assert!(
            listing.contains("drawer:open | glass | \"Menu\" | [16, 95, 48, 48] | tap"),
            "{listing}"
        );

        let calls = runner.calls.borrow();
        let args = calls[0].join(" ");
        assert!(
            args.starts_with("run -q -p cheetah-render -- --app nightshade-demo --scale 1"),
            "{args}"
        );
        assert!(
            args.contains("--tap drawer:open --type search sushi --event submit search  --tree"),
            "{args}"
        );
    }

    #[test]
    fn a_build_that_fails_says_why() {
        let runner = canned(|_| {
            Output {
            ok: false,
            stdout: String::new(),
            stderr: "error[E0425]: cannot find value `cards` in this scope\n --> apps/nightshade/src/nightshade/mod.rs:12:5".into(),
        }
        });
        let reply = handle(
            &runner,
            &request("tools/call", json!({"name": "elements", "arguments": {}})),
        )
        .unwrap();
        assert_eq!(reply["result"]["isError"], true);
        assert!(reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("cannot find value `cards`"));

        let bad = handle(
            &runner,
            &request(
                "tools/call",
                json!({"name": "render", "arguments": {"steps": [{"wave": 1}]}}),
            ),
        )
        .unwrap();
        assert_eq!(bad["result"]["isError"], true);
    }

    #[test]
    fn tests_report_their_summary_and_failures() {
        let runner = canned(|_| {
            Output {
            ok: false,
            stdout: "running 2 tests\ntest a ... ok\ntest b ... FAILED\n\n---- b stdout ----\nassertion `left == right` failed\n  left: 1\n right: 2\ntest result: FAILED. 1 passed; 1 failed\n".into(),
            stderr: String::new(),
        }
        });
        let reply = handle(
            &runner,
            &request(
                "tools/call",
                json!({"name": "test", "arguments": {"filter": "drawer"}}),
            ),
        )
        .unwrap();
        assert_eq!(reply["result"]["isError"], true);
        let summary = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert!(
            summary.contains("---- b stdout ----")
                && summary.contains("left: 1")
                && summary.contains("1 failed")
        );
        assert_eq!(
            runner.calls.borrow()[0],
            ["test", "-p", "nightshade", "--", "drawer"]
        );
    }

    #[test]
    fn long_text_keeps_its_end() {
        let long = format!("{}the end", "x".repeat(MAX_TEXT * 2));
        let cut = trim(&long);
        assert!(cut.len() < MAX_TEXT + 20 && cut.ends_with("the end"));
    }

    #[test]
    fn device_tools_talk_to_the_dev_server() {
        let runner = canned(rendered);
        let arguments =
            json!({"name": "device_screen", "arguments": {"server": "ws://10.0.0.5:8787"}});
        let screen = handle(&runner, &request("tools/call", arguments)).unwrap();
        assert_eq!(screen["result"]["isError"], false);
        assert_eq!(screen["result"]["content"][0]["type"], "image");
        let listing = screen["result"]["content"][1]["text"].as_str().unwrap();
        assert!(
            listing.contains("more | text | \"more\" | [0, 22, 60, 22] | tap"),
            "{listing}"
        );

        let arguments = json!({"name": "device_tap", "arguments": {"key": "more"}});
        let tap = handle(&runner, &request("tools/call", arguments)).unwrap();
        assert_eq!(tap["result"]["isError"], false);
        {
            let calls = runner.calls.borrow();
            assert_eq!(calls[0][0], "ws://10.0.0.5:8787");
            assert!(
                calls[1][1].contains("\"t\":\"tap\"") && calls[1][1].contains("\"key\":\"more\"")
            );
        }

        let arguments = json!({"name": "device_tap", "arguments": {"key": "nope"}});
        let missing = handle(&runner, &request("tools/call", arguments)).unwrap();
        assert_eq!(missing["result"]["isError"], true);
        assert!(missing["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("nope"));
    }

    #[test]
    fn the_agent_address_comes_from_the_server_given() {
        assert_eq!(
            agent_url("ws://10.0.0.5:8787/", "t0k"),
            "ws://10.0.0.5:8787/agent?token=t0k"
        );
        assert_eq!(
            agent_url("10.0.0.5:8787", "t0k"),
            "ws://10.0.0.5:8787/agent?token=t0k"
        );
        // A dev server here wrote its token for its port.
        let port = 40000 + std::process::id() % 20000;
        let file = std::env::temp_dir().join(format!("cheetah-agent-token-{port}"));
        std::fs::write(
            &file,
            "fromfile
",
        )
        .unwrap();
        assert_eq!(
            agent_url(&format!("ws://127.0.0.1:{port}"), ""),
            format!("ws://127.0.0.1:{port}/agent?token=fromfile")
        );
        // Not for a server elsewhere.
        assert_eq!(
            agent_url(&format!("ws://10.0.0.5:{port}"), ""),
            format!("ws://10.0.0.5:{port}/agent")
        );
        let _ = std::fs::remove_file(file);
    }
}
