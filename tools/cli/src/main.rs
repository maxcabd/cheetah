//! `cheetah`: one command surface for developing and shipping Cheetah apps.
//!
//!   cheetah dev [--app nightshade] [--port 8787] [--host IP] [--no-qr]
//!   cheetah build ios|android [--dev] [--app nightshade] [--install]
//!   cheetah fingerprint [ios|android] [--app nightshade]
//!   cheetah prebuild [--app nightshade] [--dev] [--check]
//!   cheetah doctor
//!
//! Every command takes `--json` for machine-readable output (one JSON object
//! per line), which is what agents should use.

mod apple;
mod setup;
mod signing;

use cheetah_cli::config::AppConfig;
use cheetah_cli::{dev_link, fingerprint, lan_ip, prebuild, repo_root};
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

const USAGE: &str = "\
cheetah: develop and ship Cheetah apps

  cheetah dev [--app NAME] [--port N] [--host IP] [--no-qr]
      Run the app on this machine with live reload, and print a QR code that
      connects a dev build to it. A USB Android device is connected
      automatically.
  cheetah build ios|android [--dev] [--app NAME] [--install]
      --dev builds the dev client (connects to `cheetah dev`). Android builds
      locally (--install puts it on the USB device); iOS queues the TestFlight
      workflow on GitHub (GitHub CLI login, or GITHUB_TOKEN).
  cheetah prebuild [--app NAME] [--dev] [--check]
      Write apps/NAME/cheetah.toml (name, IDs, permissions, links, native
      modules) into the iOS and Android projects. --dev makes the iOS dev
      build; --check only reports what would change.
  cheetah setup ios [--app NAME] [--issuer ID] [--key FILE] [--team ID]
                    [--release] [--build] [--no-secrets] [--no-wait]
                    [--new-certificate]
      Connect an Apple developer account through the App Store Connect API:
      picks up the API key download, registers app IDs and capabilities from
      cheetah.toml, waits for the App Store Connect app, adds the team as
      TestFlight testers, makes one distribution certificate and App Store
      profiles that every build reuses (like EAS credentials), and stores the
      GitHub secrets for the build. Reruns reuse the certificate;
      --new-certificate replaces it. --build continues with
      `cheetah build ios --dev`.
  cheetah publish [--app NAME] [--runtime VERSION] [--out DIR]
      Bundle the app's .view files for over-the-air updates
      (runtime::updates). Host the file and point Updates at its URL; it
      applies to app builds whose runtime (version+build) matches.
  cheetah fingerprint [ios|android] [--app NAME]
      Native code fingerprint. A dev build needs rebuilding only when this
      changes; Rust and .view edits never need one.
  cheetah doctor
      Check the tools each command needs.
  cheetah metrics [--app NAME]
      What each frame cost in the dev server's current or last session, by
      what caused it (tap, scroll, results arriving).
  cheetah qr TEXT
      Print TEXT as a QR code, e.g. a dev link for the phone's camera.

  --json   machine-readable output (one JSON object per line)";

const DEFAULT_APP: &str = "nightshade";
const DEFAULT_ANDROID_PACKAGE: &str = "com.example.rustui";
const TESTFLIGHT_WORKFLOW: &str = "testflight.yml";

struct Args {
    json: bool,
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Self {
        const VALUED: &[&str] = &[
            "--app",
            "--port",
            "--host",
            "--runtime",
            "--out",
            "--issuer",
            "--key",
            "--team",
        ];
        let mut args = Args {
            json: false,
            positional: Vec::new(),
            flags: Vec::new(),
        };
        let mut iter = raw.into_iter();
        while let Some(arg) = iter.next() {
            if arg == "--json" {
                args.json = true;
            } else if VALUED.contains(&arg.as_str()) {
                let value = iter.next();
                args.flags.push((arg, value));
            } else if arg.starts_with("--") {
                args.flags.push((arg, None));
            } else {
                args.positional.push(arg);
            }
        }
        args
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|(flag, _)| flag == name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(flag, _)| flag == name)
            .and_then(|(_, value)| value.as_deref())
    }

    fn unknown_flags(&self, known: &[&str]) -> Option<String> {
        self.flags
            .iter()
            .map(|(flag, _)| flag)
            .find(|flag| !known.contains(&flag.as_str()))
            .cloned()
    }
}

/// Prints for people, or one JSON line for agents.
struct Out {
    json: bool,
}

impl Out {
    fn say(&self, text: impl AsRef<str>) {
        if !self.json {
            println!("{}", text.as_ref());
        }
    }

    fn data(&self, value: Value) {
        if self.json {
            println!("{value}");
            let _ = std::io::stdout().flush();
        }
    }

    fn fail(&self, command: &str, message: &str) -> ExitCode {
        if self.json {
            println!(
                "{}",
                json!({"command": command, "ok": false, "error": message})
            );
        } else {
            eprintln!("cheetah {command}: {message}");
        }
        ExitCode::FAILURE
    }
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = raw.first().cloned() else {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    };
    let args = Args::parse(raw[1..].to_vec());
    let out = Out { json: args.json };
    if args.flag("--help") || command == "help" || command == "-h" {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match command.as_str() {
        "dev" => dev(&args, &out),
        "build" => build(&args, &out),
        "fingerprint" => fingerprint_command(&args, &out),
        "prebuild" => prebuild_command(&args, &out),
        "publish" => publish_command(&args, &out),
        "setup" => match args.positional.first().map(String::as_str) {
            Some("ios") => setup::ios(&args, &out),
            _ => out.fail("setup", "name a platform: cheetah setup ios"),
        },
        "doctor" => doctor(&out),
        "metrics" => metrics_command(&args, &out),
        "qr" => match args.positional.first().and_then(|text| qr(text)) {
            Some(code) => {
                println!("{code}");
                ExitCode::SUCCESS
            }
            None => out.fail("qr", "give the text to encode: cheetah qr TEXT"),
        },
        other => out.fail(other, "unknown command; run `cheetah help`"),
    }
}

// ---------------------------------------------------------------------------
// cheetah dev
// ---------------------------------------------------------------------------

fn dev(args: &Args, out: &Out) -> ExitCode {
    if let Some(flag) = args.unknown_flags(&["--app", "--port", "--host", "--no-qr"]) {
        return out.fail("dev", &format!("unknown option {flag}"));
    }
    let root = repo_root();
    let app = args.value("--app").unwrap_or("nightshade");
    let port: u16 = match args.value("--port").map(str::parse).transpose() {
        Ok(port) => port.unwrap_or(8787),
        Err(_) => return out.fail("dev", "--port needs a number"),
    };
    let host = match args.value("--host") {
        Some(host) => host.to_string(),
        None => match lan_ip() {
            Some(ip) => ip.to_string(),
            None => {
                out.say("cheetah dev: no network address found; phones on Wi-Fi can't connect (USB still works)");
                "127.0.0.1".to_string()
            }
        },
    };
    let server = format!("ws://{host}:{port}");
    let link = dev_link(&server);

    out.data(json!({
        "command": "dev",
        "event": "starting",
        "app": app,
        "server": server,
        "link": link,
        "native": native_fingerprints(&root, app),
    }));
    out.say(format!("Cheetah dev server for `{app}` at {server}"));
    if !args.flag("--no-qr") && !out.json {
        match qr(&link) {
            Some(code) => println!("{code}"),
            None => println!("(link too long for a QR code)"),
        }
        println!("Scan with the phone's camera (dev build installed): {link}");
        println!("Needs the phone on the same Wi-Fi, and port {port} allowed through this machine's firewall.");
    }
    let package = AppConfig::load(&root, app)
        .map(|config| config.android_package)
        .unwrap_or_else(|_| DEFAULT_ANDROID_PACKAGE.to_string());
    connect_usb_android(port, &package, out);

    // The watcher rebuilds and restarts the server on Rust changes; .view and
    // literal edits apply without a rebuild.
    let mut command = Command::new("cargo");
    command.current_dir(&root).args([
        "run",
        "-q",
        "-p",
        "cheetah-devserver",
        "--bin",
        "cheetah-dev",
        "--",
    ]);
    if root.join("apps").join(app).join("Cargo.toml").is_file() {
        command.args(["-p", app, "--bin", app]);
    } else {
        command.args([
            "-p",
            "cheetah-devserver",
            "--bin",
            "cheetah-devserver",
            "--app",
            app,
        ]);
    }
    command.args(["--lan", "--port", &port.to_string()]);
    match command.status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => out.fail("dev", &format!("dev server exited with {status}")),
        Err(error) => out.fail("dev", &format!("cannot run cargo: {error}")),
    }
}

/// The dev server's frame metrics: a table, or the whole file with `--json`.
fn metrics_command(args: &Args, out: &Out) -> ExitCode {
    if let Some(flag) = args.unknown_flags(&["--app"]) {
        return out.fail("metrics", &format!("unknown option {flag}"));
    }
    let app = args.value("--app").unwrap_or(DEFAULT_APP);
    let path = cheetah_cli::metrics_path(app);
    let Some(data) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return out.fail(
            "metrics",
            &format!("no metrics for {app} yet: connect a phone to `cheetah dev` and use the app"),
        );
    };
    let age = std::fs::metadata(&path)
        .and_then(|file| file.modified())
        .ok()
        .and_then(|at| at.elapsed().ok())
        .map_or(0, |age| age.as_secs());
    out.data(json!({"command": "metrics", "ok": true, "age_s": age, "metrics": data}));
    out.say(metrics_table(&data, age));
    ExitCode::SUCCESS
}

fn metrics_table(data: &Value, age: u64) -> String {
    let ms = |value: &Value| format!("{:.1}", value.as_f64().unwrap_or(0.0) / 1000.0);
    let count = |value: &Value| value.as_u64().unwrap_or(0);
    let mut lines = vec![
        format!(
            "{} ({} build), updated {age} s ago; {} text measurements, {} timed out",
            data["app"].as_str().unwrap_or("app"),
            data["profile"].as_str().unwrap_or("?"),
            count(&data["text_measuring"]["round_trips"]),
            count(&data["text_measuring"]["timeouts"]),
        ),
        format!(
            "{:<24}{:>7}{:>7}{:>9}{:>9}{:>9}{:>9}{:>8}{:>9}",
            "ms, by cause", "frames", "slow", "p50", "p95", "max", "measure", "ops", "bytes"
        ),
    ];
    let row = |name: &str, entry: &Value| {
        format!(
            "{name:<24}{:>7}{:>7}{:>9}{:>9}{:>9}{:>9}{:>8}{:>9}",
            count(&entry["frames"]),
            count(&entry["over_budget"]),
            ms(&entry["work_us"]["p50"]),
            ms(&entry["work_us"]["p95"]),
            ms(&entry["work_us"]["max"]),
            ms(&entry["measure_us"]["p95"]),
            count(&entry["ops"]["p50"]),
            count(&entry["wire_bytes"]["p50"]),
        )
    };
    let mut triggers: Vec<(&String, &Value)> = data["triggers"]
        .as_object()
        .map(|entries| entries.iter().collect())
        .unwrap_or_default();
    triggers.sort_by_key(|(_, entry)| std::cmp::Reverse(count(&entry["work_us"]["p95"])));
    lines.extend(triggers.iter().map(|(name, entry)| row(name, entry)));
    lines.push(row("all", &data["total"]));
    let mean = &data["total"]["mean_us"];
    lines.push(format!(
        "mean stage ms: update {} / build {} / diff {} / layout {} / encode {}",
        ms(&mean["update"]),
        ms(&mean["build"]),
        ms(&mean["diff"]),
        ms(&mean["layout"]),
        ms(&mean["encode"]),
    ));
    lines.push(
        "slow: over 16.7 ms of work. measure: p95 wait for the phone to measure text. \
         These are this computer's times."
            .into(),
    );
    if data["total"]["device"].is_null() {
        lines.push("On the phone: no reports (needs a dev build from this version on).".into());
        return lines.join("\n");
    }
    const PARTS: [&str; 13] = [
        "apply", "commit", "create", "props", "children", "layout", "remove", "insert", "glass",
        "modules", "calls", "map", "after",
    ];
    let mut header = format!("{:<24}{:>7}{:>8}{:>8} |", "on the phone, ms", "frames", "p50", "p95");
    for part in PARTS {
        header.push_str(&format!("{part:>9}"));
    }
    lines.push(String::new());
    lines.push(header);
    let device_row = |name: &str, device: &Value| {
        let mut line = format!(
            "{name:<24}{:>7}{:>8}{:>8} |",
            count(&device["frames"]),
            ms(&device["main_us"]["p50"]),
            ms(&device["main_us"]["p95"]),
        );
        for part in PARTS {
            line.push_str(&format!("{:>9}", ms(&device["mean_us"][part])));
        }
        line
    };
    let mut on_device: Vec<(&String, &Value)> = triggers
        .iter()
        .filter(|(_, entry)| !entry["device"].is_null())
        .map(|(name, entry)| (*name, &entry["device"]))
        .collect();
    on_device.sort_by_key(|(_, device)| std::cmp::Reverse(count(&device["main_us"]["p95"])));
    lines.extend(on_device.iter().map(|(name, device)| device_row(name, device)));
    lines.push(device_row("all", &data["total"]["device"]));
    lines.push(
        "p50, p95: main-thread time, apply + commit (UIKit's layout and drawing) + after \
         (haptics). The rest are means; create to map are parts of apply."
            .into(),
    );
    let mut named: Vec<(&String, &Value)> = data["named"]
        .as_object()
        .map(|entries| entries.iter().collect())
        .unwrap_or_default();
    if !named.is_empty() {
        named.sort_by_key(|(_, work)| std::cmp::Reverse(count(&work["max_us"])));
        lines.push(String::new());
        lines.push(format!(
            "{:<32}{:>7}{:>8}{:>8}  worst in",
            "native work, ms per frame", "frames", "mean", "max"
        ));
        lines.extend(named.iter().take(20).map(|(name, work)| {
            format!(
                "{name:<32}{:>7}{:>8}{:>8}  {}",
                count(&work["frames"]),
                ms(&work["mean_us"]),
                ms(&work["max_us"]),
                work["worst"].as_str().unwrap_or(""),
            )
        }));
        lines.push("late: ran after the frame before this one was on screen.".into());
    }
    lines.join("\n")
}

/// White-on-dark half-block QR code for a terminal.
fn qr(text: &str) -> Option<String> {
    use qrcode::render::unicode::Dense1x2;
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    Some(
        code.render::<Dense1x2>()
            .dark_color(Dense1x2::Light)
            .light_color(Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
    )
}

/// A USB Android device with the dev build: forward the port and open the
/// dev link on it, so no scanning is needed.
fn connect_usb_android(port: u16, package: &str, out: &Out) {
    let devices = adb_devices();
    if devices.is_empty() {
        return;
    }
    let installed = output("adb", &["shell", "pm", "list", "packages", package])
        .is_some_and(|text| text.contains(package));
    if !installed {
        out.say("Android device found, but the dev build isn't installed: cheetah build android --dev --install");
        out.data(json!({"command": "dev", "event": "android", "connected": false, "reason": "dev build not installed"}));
        return;
    }
    let forwarded = output(
        "adb",
        &["reverse", &format!("tcp:{port}"), &format!("tcp:{port}")],
    )
    .is_some();
    let link = dev_link(&format!("ws://localhost:{port}"));
    // Quoted for the device shell (`?` is a glob character there).
    let opened = output(
        "adb",
        &[
            "shell",
            "am",
            "start",
            "-a",
            "android.intent.action.VIEW",
            "-d",
            &format!("'{link}'"),
        ],
    )
    .is_some();
    let connected = forwarded && opened;
    out.say(if connected {
        "Android device on USB: opened the dev build, connecting to this server"
    } else {
        "Android device on USB: could not open the dev build (adb reverse or am start failed)"
    });
    out.data(
        json!({"command": "dev", "event": "android", "connected": connected, "devices": devices}),
    );
}

// ---------------------------------------------------------------------------
// cheetah build
// ---------------------------------------------------------------------------

fn build(args: &Args, out: &Out) -> ExitCode {
    if let Some(flag) = args.unknown_flags(&["--dev", "--app", "--install"]) {
        return out.fail("build", &format!("unknown option {flag}"));
    }
    let root = repo_root();
    let app = args.value("--app").unwrap_or("nightshade");
    let dev = args.flag("--dev");
    match args.positional.first().map(String::as_str) {
        Some("android") => build_android(&root, app, dev, args.flag("--install"), out),
        Some("ios") => build_ios(&root, app, dev, out),
        _ => out.fail(
            "build",
            "name a platform: cheetah build ios|android [--dev]",
        ),
    }
}

fn build_android(root: &Path, app: &str, dev: bool, install: bool, out: &Out) -> ExitCode {
    if !dev {
        return out.fail(
            "build",
            "only dev builds are supported for Android so far: add --dev",
        );
    }
    // The Android dev build is the debug build of the app's identity.
    if let Err(error) = run_prebuild(root, app, false, out) {
        return out.fail("build", &error);
    }
    let native =
        fingerprint::native(root, "android", Some(app)).unwrap_or_else(|| "unknown".into());
    out.say(format!("Building the Android dev build ({native})"));
    let shell = if cfg!(windows) { "powershell" } else { "pwsh" };
    let mut command = Command::new(shell);
    command
        .current_dir(root)
        .env("CHEETAH_NATIVE_FINGERPRINT", &native)
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "build-android.ps1",
            "-App",
            app,
        ]);
    if install {
        command.arg("-Install");
    }
    if out.json {
        // Keep stdout for the JSON result.
        command.stdout(Stdio::null());
    }
    let status = match command.status() {
        Ok(status) => status,
        Err(error) => return out.fail("build", &format!("cannot run {shell}: {error}")),
    };
    if !status.success() {
        return out.fail("build", &format!("build-android.ps1 failed ({status})"));
    }
    let apk = root.join("android/app/build/outputs/apk/debug/app-debug.apk");
    out.say(format!("Built {}", apk.display()));
    out.data(json!({
        "command": "build", "ok": true, "platform": "android", "dev": true,
        "native": native, "apk": apk, "installed": install,
    }));
    ExitCode::SUCCESS
}

/// Queues the TestFlight workflow. Dev builds use their own bundle ID
/// (`….dev`, "Cheetah Dev") so they never reach release testers.
fn build_ios(root: &Path, app: &str, dev: bool, out: &Out) -> ExitCode {
    let Some(repo) = github_repo(root) else {
        return out.fail(
            "build",
            "no GitHub `origin` remote; iOS builds run on GitHub Actions",
        );
    };
    let actions = format!("https://github.com/{repo}/actions/workflows/{TESTFLIGHT_WORKFLOW}");
    let Some((token, _)) = github_token() else {
        return out.fail(
            "build",
            &format!(
                "log in with the GitHub CLI (gh auth login) or set GITHUB_TOKEN to queue \
                 the build, or run it from {actions} with dev_client = {dev}"
            ),
        );
    };
    let body = json!({
        "ref": current_branch(root).unwrap_or_else(|| "main".into()),
        "inputs": {"app": app, "dev_client": dev},
    });
    // Headers via stdin, so the token isn't on the command line.
    let child = Command::new("curl")
        .args([
            "-sS",
            "-o",
            "-",
            "-w",
            "\n%{http_code}",
            "-X",
            "POST",
            "-H",
            "@-",
        ])
        .args([
            "-H",
            "Accept: application/vnd.github+json",
            "-d",
            &body.to_string(),
        ])
        .arg(format!(
            "https://api.github.com/repos/{repo}/actions/workflows/{TESTFLIGHT_WORKFLOW}/dispatches"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => return out.fail("build", &format!("cannot run curl: {error}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "Authorization: Bearer {token}");
    }
    let result = match child.wait_with_output() {
        Ok(result) => result,
        Err(error) => return out.fail("build", &format!("curl failed: {error}")),
    };
    let text = String::from_utf8_lossy(&result.stdout);
    let status = text.lines().last().unwrap_or("").trim();
    if status != "204" {
        let detail = text.lines().next().unwrap_or("").trim();
        return out.fail(
            "build",
            &format!("GitHub refused the dispatch (HTTP {status}): {detail}"),
        );
    }
    let kind = if dev { "dev build" } else { "release build" };
    out.say(format!(
        "Queued the iOS {kind}: {actions}\nTestFlight notifies installed testers when it's ready (usually 15-30 min)."
    ));
    out.data(json!({
        "command": "build", "ok": true, "platform": "ios", "dev": dev,
        "queued": true, "runs": actions,
        "native": fingerprint::native(root, "ios", Some(app)),
    }));
    ExitCode::SUCCESS
}

/// A GitHub token and where it came from: GITHUB_TOKEN / GH_TOKEN, else the
/// GitHub CLI's login (`gh auth token`).
fn github_token() -> Option<(String, &'static str)> {
    for name in ["GITHUB_TOKEN", "GH_TOKEN"] {
        if let Ok(token) = std::env::var(name) {
            if !token.trim().is_empty() {
                return Some((token.trim().to_string(), name));
            }
        }
    }
    // Not on PATH in shells opened before it was installed.
    for gh in ["gh", r"C:\Program Files\GitHub CLI\gh.exe"] {
        if let Some(token) = output(gh, &["auth", "token"]) {
            let token = token.trim().to_string();
            if !token.is_empty() {
                return Some((token, "GitHub CLI login"));
            }
        }
    }
    None
}

/// `owner/name` from the `origin` remote.
fn github_repo(root: &Path) -> Option<String> {
    let url = output_in(root, "git", &["remote", "get-url", "origin"])?;
    let path = url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| url.trim().strip_prefix("git@github.com:"))?;
    Some(
        path.trim_end_matches('/')
            .trim_end_matches(".git")
            .to_string(),
    )
}

fn current_branch(root: &Path) -> Option<String> {
    output_in(root, "git", &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
}

// ---------------------------------------------------------------------------
// cheetah fingerprint / doctor
// ---------------------------------------------------------------------------

fn native_fingerprints(root: &Path, app: &str) -> Value {
    fingerprint::PLATFORMS
        .iter()
        .map(|platform| {
            (
                platform.to_string(),
                json!(fingerprint::native(root, platform, Some(app))),
            )
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn fingerprint_command(args: &Args, out: &Out) -> ExitCode {
    if let Some(flag) = args.unknown_flags(&["--app"]) {
        return out.fail("fingerprint", &format!("unknown option {flag}"));
    }
    let root = repo_root();
    let app = args.value("--app").unwrap_or(DEFAULT_APP);
    let platforms: Vec<&str> = match args.positional.first().map(String::as_str) {
        None => fingerprint::PLATFORMS.to_vec(),
        Some(platform) if fingerprint::PLATFORMS.contains(&platform) => vec![platform],
        Some(other) => return out.fail("fingerprint", &format!("unknown platform `{other}`")),
    };
    let mut values = serde_json::Map::new();
    for platform in platforms {
        let Some(value) = fingerprint::native(&root, platform, Some(app)) else {
            return out.fail("fingerprint", "needs git and a git checkout");
        };
        out.say(&value);
        values.insert(platform.into(), json!(value));
    }
    out.data(json!({"command": "fingerprint", "ok": true, "native": values}));
    ExitCode::SUCCESS
}

fn publish_command(args: &Args, out: &Out) -> ExitCode {
    if let Some(flag) = args.unknown_flags(&["--app", "--runtime", "--out"]) {
        return out.fail("publish", &format!("unknown option {flag}"));
    }
    let root = repo_root();
    let app = args.value("--app").unwrap_or(DEFAULT_APP);
    let target = args.value("--out").map(std::path::PathBuf::from);
    match cheetah_cli::publish::publish(&root, app, args.value("--runtime"), target.as_deref()) {
        Ok(published) => {
            out.say(format!(
                "Bundled {} view files for runtime {} (version {}):
  {}
Upload it and point the app's Updates at its URL.",
                published.files.len(),
                published.runtime,
                published.version,
                published.path.display()
            ));
            out.data(json!({
                "command": "publish", "ok": true, "app": app,
                "runtime": published.runtime, "version": published.version,
                "path": published.path, "files": published.files,
            }));
            ExitCode::SUCCESS
        }
        Err(error) => out.fail("publish", &error),
    }
}

fn prebuild_command(args: &Args, out: &Out) -> ExitCode {
    if let Some(flag) = args.unknown_flags(&["--app", "--dev", "--check"]) {
        return out.fail("prebuild", &format!("unknown option {flag}"));
    }
    let root = repo_root();
    let app = args.value("--app").unwrap_or(DEFAULT_APP);
    let dev = args.flag("--dev");
    if args.flag("--check") {
        let config = match AppConfig::load(&root, app) {
            Ok(config) => config,
            Err(error) => return out.fail("prebuild", &error),
        };
        let outputs = match prebuild::plan(&root, app, &config, dev) {
            Ok(outputs) => outputs,
            Err(error) => return out.fail("prebuild", &error),
        };
        let stale: Vec<String> = outputs
            .iter()
            .filter(|output| {
                let current = std::fs::read_to_string(&output.path).unwrap_or_default();
                current.replace("\r\n", "\n") != output.contents
            })
            .map(|output| relative(&root, &output.path))
            .collect();
        for path in &stale {
            out.say(format!("would change {path}"));
        }
        if stale.is_empty() {
            out.say(format!("native projects match apps/{app}/cheetah.toml"));
        }
        out.data(
            json!({"command": "prebuild", "ok": true, "app": app, "check": true, "stale": stale}),
        );
        return ExitCode::SUCCESS;
    }
    match run_prebuild(&root, app, dev, out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => out.fail("prebuild", &error),
    }
}

/// Write the app's config into the native projects; reports changed files.
fn run_prebuild(root: &Path, app: &str, dev: bool, out: &Out) -> Result<(), String> {
    let config = AppConfig::load(root, app)?;
    let changed = prebuild::write(&prebuild::plan(root, app, &config, dev)?)?;
    let changed: Vec<String> = changed.iter().map(|path| relative(root, path)).collect();
    if changed.is_empty() {
        out.say(format!(
            "prebuild: native projects already match apps/{app}/cheetah.toml"
        ));
    } else {
        out.say(format!(
            "prebuild: wrote apps/{app}/cheetah.toml into {}",
            changed.join(", ")
        ));
    }
    out.data(
        json!({"command": "prebuild", "ok": true, "app": app, "dev": dev, "changed": changed}),
    );
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn doctor(out: &Out) -> ExitCode {
    let root = repo_root();
    let first_line = |text: String| text.lines().next().unwrap_or("").trim().to_string();
    let sdk = std::env::var("ANDROID_HOME")
        .or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
        .ok()
        .filter(|path| Path::new(path).is_dir());
    let devices = adb_devices();
    let android_targets = output("rustup", &["target", "list", "--installed"])
        .is_some_and(|t| t.contains("aarch64-linux-android") && t.contains("x86_64-linux-android"));
    // (name, found, required for, detail)
    let checks: Vec<(&str, bool, &str, String)> = vec![
        (
            "git",
            output("git", &["--version"]).is_some(),
            "everything",
            output("git", &["--version"])
                .map(first_line)
                .unwrap_or_default(),
        ),
        (
            "cargo",
            output("cargo", &["--version"]).is_some(),
            "everything",
            output("cargo", &["--version"])
                .map(first_line)
                .unwrap_or_default(),
        ),
        (
            "lan address",
            lan_ip().is_some(),
            "dev on Wi-Fi",
            lan_ip().map(|ip| ip.to_string()).unwrap_or_default(),
        ),
        (
            "android sdk",
            sdk.is_some(),
            "android builds",
            sdk.unwrap_or_else(|| "set ANDROID_HOME".into()),
        ),
        (
            "android targets",
            android_targets,
            "android builds",
            if android_targets {
                "aarch64/x86_64-linux-android".into()
            } else {
                "rustup target add aarch64-linux-android x86_64-linux-android".into()
            },
        ),
        (
            "adb",
            output("adb", &["version"]).is_some(),
            "android install",
            output("adb", &["version"])
                .map(first_line)
                .unwrap_or_default(),
        ),
        (
            "android device",
            !devices.is_empty(),
            "android install",
            devices.join(", "),
        ),
        (
            "github token",
            github_token().is_some(),
            "ios builds",
            github_token()
                .map(|(_, source)| source.to_string())
                .unwrap_or_else(|| "gh auth login, or set GITHUB_TOKEN".into()),
        ),
        (
            "github remote",
            github_repo(&root).is_some(),
            "ios builds",
            github_repo(&root).unwrap_or_default(),
        ),
        (
            "dx",
            output("dx", &["--version"]).is_some(),
            "rust hot patching (optional)",
            output("dx", &["--version"])
                .map(first_line)
                .unwrap_or_else(|| "cargo install dioxus-cli --version 0.7.10 --locked".into()),
        ),
    ];
    let required_missing = checks
        .iter()
        .any(|(_, found, needed, _)| !found && *needed == "everything");
    for (name, found, needed, detail) in &checks {
        let mark = if *found { "ok " } else { "-- " };
        out.say(format!(
            "{mark} {name:<15} {detail}{}",
            if *found {
                String::new()
            } else {
                format!("  (needed for {needed})")
            }
        ));
    }
    out.data(json!({
        "command": "doctor",
        "ok": !required_missing,
        "checks": checks.iter().map(|(name, found, needed, detail)| json!({
            "name": name, "ok": found, "needed_for": needed, "detail": detail,
        })).collect::<Vec<_>>(),
        "native": native_fingerprints(&root, DEFAULT_APP),
    }));
    if required_missing {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Stdout of a successful command, or `None`.
fn output(program: &str, args: &[&str]) -> Option<String> {
    run(Command::new(program).args(args))
}

fn output_in(dir: &Path, program: &str, args: &[&str]) -> Option<String> {
    run(Command::new(program).current_dir(dir).args(args))
}

fn run(command: &mut Command) -> Option<String> {
    let result = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    result
        .status
        .success()
        .then(|| String::from_utf8_lossy(&result.stdout).into_owned())
}

/// Serials of attached, authorized Android devices.
fn adb_devices() -> Vec<String> {
    output("adb", &["devices"])
        .map(|text| {
            text.lines()
                .skip(1)
                .filter_map(|line| line.strip_suffix("\tdevice"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::parse(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn parses_flags_values_and_positionals() {
        let parsed = args(&["ios", "--dev", "--app", "chat", "--json"]);
        assert_eq!(parsed.positional, ["ios"]);
        assert!(parsed.flag("--dev") && parsed.json);
        assert_eq!(parsed.value("--app"), Some("chat"));
        assert_eq!(parsed.unknown_flags(&["--dev", "--app"]), None);
        assert_eq!(
            args(&["--nope"]).unknown_flags(&["--dev"]).as_deref(),
            Some("--nope")
        );
    }

    #[test]
    fn qr_codes_render_for_dev_links() {
        let code = qr(&dev_link("ws://192.168.100.200:8787")).unwrap();
        assert!(code.lines().count() > 10);
    }
}
