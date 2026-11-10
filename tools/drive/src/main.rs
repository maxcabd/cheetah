use roxmltree::Document;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

const APP_ID: &str = "com.example.rustui";
const ACTIVITY: &str = "com.example.rustui/.MainActivity";

fn main() {
    if let Err(error) = run() {
        eprintln!("cheetah-drive: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(platform) = args.first().map(String::as_str) else {
        return Err(help());
    };
    let rest = &args[1..];

    match platform {
        "android" => android(rest),
        "ios" => ios(rest),
        "-h" | "--help" | "help" => {
            println!("{}", help());
            Ok(())
        }
        _ => Err(help()),
    }
}

fn help() -> String {
    "usage: cheetah-drive <android|ios> <command> [...]

android commands:
  start [app]              launch Cheetah (default: nightshade)
  stop
  tree                     print the accessibility tree as JSON
  tap <text|id|desc>       tap the first matching node
  type <target> <text>     tap a field and type text
  screenshot <path>        capture the device
  layout <target>          print a node's bounds
  settle [timeout_ms]      wait until two accessibility dumps match
  clock +<ms>|settle       advance the wall clock / settle
  logs [lines]             print recent device log lines
  test <file>              run a line-oriented drive script

ios commands (requires macOS/xcrun):
  tree [app]               launch through XCUITest and print its tree
  tap <target> [app]       launch through XCUITest and tap
  type <target> <text>     launch through XCUITest and type
  test <file>              run a drive script through XCUITest
  launch <bundle> [args...]
  terminate <bundle>
  screenshot <path>

Set CHEETAH_IOS_PROJECT and CHEETAH_IOS_DESTINATION to override the Xcode
project and simulator destination used by the XCUITest runner."
        .into()
}

// ---------------------------------------------------------------------------
// Android adapter
// ---------------------------------------------------------------------------

fn android(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(help());
    };
    let rest = &args[1..];
    let adb = Adb;

    match command {
        "start" => {
            let app = rest.first().map(String::as_str).unwrap_or("nightshade");
            adb.shell(&["am", "force-stop", APP_ID])?;
            adb.shell(&["am", "start", "-n", ACTIVITY, "--es", "app", app])?;
            adb.wait_for_app(Duration::from_secs(30))?;
            println!("started {app}");
            Ok(())
        }
        "stop" => {
            adb.shell(&["am", "force-stop", APP_ID])?;
            println!("stopped");
            Ok(())
        }
        "tree" => {
            let nodes = adb.tree()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&tree_json(&nodes))
                    .map_err(|error| error.to_string())?
            );
            Ok(())
        }
        "tap" => {
            let target = required(rest, "tap target")?;
            let node = adb.wait_find(target, Duration::from_secs(20))?;
            adb.tap(node.center())?;
            println!("tapped {}", node.describe());
            Ok(())
        }
        "type" => {
            let target = required(rest, "type target")?;
            let text = rest.get(1).map(String::as_str).unwrap_or("");
            let node = adb.wait_find(target, Duration::from_secs(20))?;
            adb.tap(node.center())?;
            thread::sleep(Duration::from_millis(250));
            adb.type_text(text)?;
            println!("typed into {}", node.describe());
            Ok(())
        }
        "screenshot" => {
            let path = required(rest, "screenshot path")?;
            adb.screenshot(Path::new(path))?;
            println!("screenshot {path}");
            Ok(())
        }
        "layout" => {
            let target = required(rest, "layout target")?;
            let node = adb.wait_find(target, Duration::from_secs(20))?;
            println!("{}", node.bounds_json());
            Ok(())
        }
        "settle" => {
            let timeout = rest
                .first()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(10_000);
            adb.settle(Duration::from_millis(timeout))?;
            println!("settled");
            Ok(())
        }
        "clock" => advance_clock(&adb, rest.first().map(String::as_str).unwrap_or("settle")),
        "logs" => {
            let lines = rest
                .first()
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(200);
            let output = adb.run(&["logcat", "-d", "-t", &lines.to_string()])?;
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            Ok(())
        }
        "test" => {
            let path = required(rest, "test file")?;
            run_android_test(&adb, Path::new(path))
        }
        _ => Err(help()),
    }
}

struct Adb;

impl Adb {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("adb");
        command.args(args);
        command
    }

    fn run(&self, args: &[&str]) -> Result<Output, String> {
        let output = self
            .command(args)
            .output()
            .map_err(|error| format!("cannot run adb: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "adb {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(output)
    }

    fn shell(&self, args: &[&str]) -> Result<Output, String> {
        let mut full = Vec::with_capacity(args.len() + 1);
        full.push("shell");
        full.extend_from_slice(args);
        self.run(&full)
    }

    fn tree(&self) -> Result<Vec<Node>, String> {
        let remote = "/sdcard/cheetah-drive-window.xml";
        let local = env::temp_dir().join("cheetah-drive-window.xml");

        for _ in 0..6 {
            self.shell(&["uiautomator", "dump", remote])?;
            if self
                .run(&["pull", remote, local.to_string_lossy().as_ref()])
                .is_ok()
            {
                if let Ok(xml) = fs::read_to_string(&local) {
                    let nodes = parse_tree(&xml)?;
                    if !nodes.is_empty() {
                        return Ok(nodes);
                    }
                }
            }
            thread::sleep(Duration::from_millis(250));
        }

        Err("the device did not expose an accessibility tree".into())
    }

    fn wait_find(&self, target: &str, timeout: Duration) -> Result<Node, String> {
        let deadline = std::time::Instant::now() + timeout;
        let mut last_error = String::new();
        while std::time::Instant::now() < deadline {
            match self.tree() {
                Ok(nodes) => {
                    if let Some(node) = find_optional(&nodes, target) {
                        return Ok(node.clone());
                    }
                    last_error = format!("node {target:?} not found");
                }
                Err(error) => last_error = error,
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(last_error)
    }

    fn settle(&self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        let mut previous = String::new();
        let mut has_sample = false;

        while std::time::Instant::now() < deadline {
            let nodes = self.tree()?;
            let snapshot =
                serde_json::to_string(&tree_json(&nodes)).map_err(|error| error.to_string())?;
            if has_sample && snapshot == previous {
                return Ok(());
            }
            has_sample = true;
            previous = snapshot;
            thread::sleep(Duration::from_millis(250));
        }

        Err("accessibility tree did not settle".into())
    }

    fn wait_for_app(&self, timeout: Duration) -> Result<(), String> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let output = self.run(&["shell", "dumpsys", "window", "windows"])?;
            if String::from_utf8_lossy(&output.stdout).contains(APP_ID) {
                thread::sleep(Duration::from_millis(1200));
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }
        Err(format!("{APP_ID} did not come to the foreground"))
    }

    fn tap(&self, (x, y): (i32, i32)) -> Result<(), String> {
        self.shell(&["input", "tap", &x.to_string(), &y.to_string()])?;
        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<(), String> {
        let encoded = text.replace(' ', "%s");
        self.shell(&["input", "text", &encoded])?;
        Ok(())
    }

    fn screenshot(&self, path: &Path) -> Result<(), String> {
        let output = self.run(&["exec-out", "screencap", "-p"])?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
        }
        fs::write(path, output.stdout).map_err(|error| format!("cannot write: {error}"))
    }
}

fn run_android_test(adb: &Adb, path: &Path) -> Result<(), String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut passed = 0usize;

    for (index, raw) in contents.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts = split_quoted(line);
        let Some(command) = parts.first().map(String::as_str) else {
            continue;
        };

        let result = match command {
            "start" => {
                let app = parts.get(1).map(String::as_str).unwrap_or("nightshade");
                adb.shell(&["am", "force-stop", APP_ID])?;
                adb.shell(&["am", "start", "-n", ACTIVITY, "--es", "app", app])?;
                adb.wait_for_app(Duration::from_secs(30))?;
                Ok(())
            }
            "tap" => {
                let target = parts.get(1).ok_or("tap needs a target")?;
                let node = adb.wait_find(target, Duration::from_secs(20))?;
                adb.tap(node.center())
            }
            "type" => {
                let target = parts.get(1).ok_or("type needs a target")?;
                let text = parts.get(2).map(String::as_str).unwrap_or("");
                let node = adb.wait_find(target, Duration::from_secs(20))?;
                adb.tap(node.center())?;
                thread::sleep(Duration::from_millis(200));
                adb.type_text(text)
            }
            "expect" => {
                let kind = parts.get(1).map(String::as_str).unwrap_or("");
                let target = parts.get(2).ok_or("expect needs a target")?;
                let nodes = adb.tree()?;
                let found = find_optional(&nodes, target).is_some();
                match kind {
                    "exists" if found => Ok(()),
                    "exists" => Err(format!("expected {target:?} to exist")),
                    "missing" if !found => Ok(()),
                    "missing" => Err(format!("expected {target:?} to be missing")),
                    "text" if found => Ok(()),
                    "text" => Err(format!("expected text {target:?}")),
                    _ => Err(format!("unknown expect kind {kind:?}")),
                }
            }
            "wait" => {
                let millis = parts
                    .get(1)
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(500);
                thread::sleep(Duration::from_millis(millis));
                Ok(())
            }
            "settle" => {
                let millis = parts
                    .get(1)
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(10_000);
                adb.settle(Duration::from_millis(millis))
            }
            "clock" => advance_clock(adb, parts.get(1).map(String::as_str).unwrap_or("settle")),
            "screenshot" => {
                let output = parts.get(1).ok_or("screenshot needs a path")?;
                adb.screenshot(Path::new(output))
            }
            "stop" => {
                adb.shell(&["am", "force-stop", APP_ID])?;
                Ok(())
            }
            _ => Err(format!("unknown command {command:?}")),
        };

        match result {
            Ok(()) => {
                passed += 1;
                println!("ok {}: {}", index + 1, line);
            }
            Err(error) => {
                return Err(format!("{}:{}: {error}", path.display(), index + 1));
            }
        }
    }

    println!("{passed} operations passed");
    Ok(())
}

// ---------------------------------------------------------------------------
// Accessibility tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Bounds {
    fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    fn json(&self) -> Value {
        json!([self.left, self.top, self.right, self.bottom])
    }
}

#[derive(Debug, Clone)]
struct Node {
    class: String,
    text: String,
    desc: String,
    id: String,
    clickable: bool,
    bounds: Bounds,
}

impl Node {
    fn center(&self) -> (i32, i32) {
        self.bounds.center()
    }

    fn describe(&self) -> String {
        if !self.desc.is_empty() {
            self.desc.clone()
        } else if !self.text.is_empty() {
            self.text.clone()
        } else {
            self.class.clone()
        }
    }

    fn bounds_json(&self) -> Value {
        json!({
            "class": self.class,
            "text": self.text,
            "desc": self.desc,
            "id": self.id,
            "clickable": self.clickable,
            "bounds": self.bounds.json(),
            "center": [self.center().0, self.center().1]
        })
    }
}

fn tree_json(nodes: &[Node]) -> Value {
    json!({
        "package": APP_ID,
        "nodes": nodes.iter().map(|node| json!({
            "class": node.class,
            "text": node.text,
            "desc": node.desc,
            "id": node.id,
            "clickable": node.clickable,
            "bounds": node.bounds.json()
        })).collect::<Vec<_>>()
    })
}

fn parse_tree(xml: &str) -> Result<Vec<Node>, String> {
    let document = Document::parse(xml).map_err(|error| format!("invalid UI tree: {error}"))?;
    let mut nodes = Vec::new();
    for element in document.descendants().filter(|node| node.is_element()) {
        if element.tag_name().name() != "node" {
            continue;
        }
        let Some(bounds) = element.attribute("bounds").and_then(parse_bounds) else {
            continue;
        };
        nodes.push(Node {
            class: element.attribute("class").unwrap_or("").into(),
            text: element.attribute("text").unwrap_or("").into(),
            desc: element.attribute("content-desc").unwrap_or("").into(),
            id: element.attribute("resource-id").unwrap_or("").into(),
            clickable: element.attribute("clickable") == Some("true"),
            bounds,
        });
    }
    Ok(nodes)
}

fn parse_bounds(value: &str) -> Option<Bounds> {
    let (left, rest) = parse_point(value)?;
    let (top, rest) = parse_point(rest)?;
    let (right, rest) = parse_point(rest)?;
    let (bottom, _) = parse_point(rest)?;
    Some(Bounds {
        left,
        top,
        right,
        bottom,
    })
}

fn parse_point(value: &str) -> Option<(i32, &str)> {
    let rest = value.trim_start_matches(['[', ']', ',']);
    let end = rest.find(|character: char| !character.is_ascii_digit() && character != '-')?;
    let number = rest[..end].parse::<i32>().ok()?;
    Some((number, &rest[end..]))
}

#[cfg(test)]
fn find<'a>(nodes: &'a [Node], target: &str) -> Result<&'a Node, String> {
    find_optional(nodes, target).ok_or_else(|| format!("node {target:?} not found"))
}

fn find_optional<'a>(nodes: &'a [Node], target: &str) -> Option<&'a Node> {
    let target = target.to_lowercase();
    nodes
        .iter()
        .find(|node| {
            node.desc.eq_ignore_ascii_case(&target)
                || node.text.eq_ignore_ascii_case(&target)
                || node.id.eq_ignore_ascii_case(&target)
        })
        .or_else(|| {
            nodes.iter().find(|node| {
                node.desc.to_lowercase().contains(&target)
                    || node.text.to_lowercase().contains(&target)
            })
        })
}

fn advance_clock(adb: &Adb, value: &str) -> Result<(), String> {
    if value == "settle" {
        adb.settle(Duration::from_secs(10))?;
        println!("settled");
        return Ok(());
    }

    let millis = value
        .trim_start_matches('+')
        .parse::<u64>()
        .map_err(|_| format!("clock expects +<milliseconds> or settle, got {value:?}"))?;
    thread::sleep(Duration::from_millis(millis));
    println!("clock +{millis}");
    Ok(())
}

fn required<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    args.first()
        .map(String::as_str)
        .ok_or_else(|| format!("missing {name}"))
}

// ---------------------------------------------------------------------------
// iOS adapter (launch and screenshots; interaction requires XCTest)
// ---------------------------------------------------------------------------

fn ios(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(help());
    };
    let rest = &args[1..];

    match command {
        "launch" => {
            let bundle = required(rest, "bundle id")?;
            let mut full = vec!["simctl", "launch", "booted", bundle];
            full.extend(rest[1..].iter().map(String::as_str));
            run_xcrun(&full)?;
            println!("launched {bundle}");
            Ok(())
        }
        "terminate" => {
            let bundle = required(rest, "bundle id")?;
            run_xcrun(&["simctl", "terminate", "booted", bundle])?;
            println!("terminated {bundle}");
            Ok(())
        }
        "screenshot" => {
            let path = required(rest, "screenshot path")?;
            run_xcrun(&["simctl", "io", "booted", "screenshot", path])?;
            println!("screenshot {path}");
            Ok(())
        }
        "tree" => {
            let app = rest.first().map(String::as_str).unwrap_or("nightshade");
            run_ios_script(&format!("start {app}\ntree\n"))
        }
        "tap" => {
            let target = required(rest, "tap target")?;
            let app = rest.get(1).map(String::as_str).unwrap_or("nightshade");
            run_ios_script(&format!("start {app}\ntap {target}\n"))
        }
        "type" => {
            let target = required(rest, "type target")?;
            let text = rest.get(1).map(String::as_str).unwrap_or("");
            let app = rest.get(2).map(String::as_str).unwrap_or("nightshade");
            run_ios_script(&format!("start {app}\ntype {target} \"{text}\"\n"))
        }
        "test" => {
            let path = required(rest, "test file")?;
            let script =
                fs::read_to_string(path).map_err(|error| format!("cannot read {path}: {error}"))?;
            run_ios_script(&script)
        }
        _ => Err(help()),
    }
}

fn run_ios_script(script: &str) -> Result<(), String> {
    let script_path = PathBuf::from("/tmp/cheetah-drive.script");
    let output_path = PathBuf::from("/tmp/cheetah-drive.out");
    fs::write(&script_path, script)
        .map_err(|error| format!("cannot write drive script: {error}"))?;
    let _ = fs::remove_file(&output_path);

    let project =
        env::var("CHEETAH_IOS_PROJECT").unwrap_or_else(|_| "ios/Cheetah.xcodeproj".into());
    let destination = env::var("CHEETAH_IOS_DESTINATION")
        .unwrap_or_else(|_| "platform=iOS Simulator,name=iPhone 17 Pro".into());

    let mut command = Command::new("xcodebuild");
    command
        .arg("test")
        .arg("-project")
        .arg(project)
        .arg("-scheme")
        .arg("Cheetah")
        .arg("-destination")
        .arg(destination)
        .arg("-only-testing:CheetahUITests/CheetahDriveTests/testDriveScript")
        .env("CHEETAH_DRIVE_FILE", script_path.to_string_lossy().as_ref())
        .env("CHEETAH_DRIVE_OUT", output_path.to_string_lossy().as_ref())
        .env(
            "CHEETAH_DRIVE_CWD",
            env::current_dir()
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .as_ref(),
        )
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = command
        .status()
        .map_err(|error| format!("cannot run xcodebuild: {error}"))?;
    if !status.success() {
        return Err("XCUITest drive failed; see xcodebuild output above".into());
    }

    let result = fs::read_to_string(&output_path)
        .map_err(|error| format!("XCUITest did not write a result: {error}"))?;
    print!("{result}");
    if !result.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn run_xcrun(args: &[&str]) -> Result<(), String> {
    let output = Command::new("xcrun")
        .args(args)
        .output()
        .map_err(|error| format!("cannot run xcrun: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "xcrun {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Script tokenizer
// ---------------------------------------------------------------------------

fn split_quoted(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for character in line.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        if character == '"' || character == '\'' {
            quote = Some(character);
            continue;
        }
        if character.is_whitespace() {
            if !current.is_empty() {
                values.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(character);
    }

    if !current.is_empty() {
        values.push(current);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"
        <hierarchy>
          <node class="android.widget.TextView" text="Coffee"
                content-desc="chip-coffee" resource-id="chip"
                clickable="true" bounds="[10,20][110,80]" />
          <node class="android.widget.TextView" text="Java Haven"
                content-desc="" resource-id=""
                clickable="false" bounds="[20,30][220,90]" />
        </hierarchy>
    "#;

    #[test]
    fn parses_accessibility_nodes_and_bounds() {
        let nodes = parse_tree(XML).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].text, "Coffee");
        assert_eq!(nodes[0].center(), (60, 50));
        assert_eq!(nodes[1].bounds.right, 220);
    }

    #[test]
    fn finds_by_text_desc_and_id() {
        let nodes = parse_tree(XML).unwrap();
        assert_eq!(find(&nodes, "Coffee").unwrap().desc, "chip-coffee");
        assert_eq!(find(&nodes, "chip-coffee").unwrap().text, "Coffee");
        assert_eq!(find(&nodes, "chip").unwrap().text, "Coffee");
        assert!(find_optional(&nodes, "Missing").is_none());
    }

    #[test]
    fn tokenizer_handles_quoted_arguments() {
        assert_eq!(
            split_quoted(r#"type search "oat milk""#),
            vec!["type", "search", "oat milk"]
        );
        assert_eq!(split_quoted("tap 'Open now'"), vec!["tap", "Open now"]);
    }
}
