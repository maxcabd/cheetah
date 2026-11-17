//! `cheetah setup ios`: everything between an Apple developer account and a
//! TestFlight build, through the official App Store Connect API.
//!
//! The two steps Apple's API can't do are browser clicks, once per account:
//! creating the API key (the command opens the page and picks up the
//! download), and creating the App Store Connect app (it opens the page with
//! the values to use, then waits for the app to appear). Everything else is
//! automatic: app IDs and capabilities from cheetah.toml, a TestFlight group
//! for the team, and the GitHub secrets the build workflow signs with.

use crate::apple::{key_id_from_file_name, query, ApiKey, Client};
use crate::{output_in, Args, Out};
use cheetah_cli::config::AppConfig;
use cheetah_cli::repo_root;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime};

const KEYS_URL: &str = "https://appstoreconnect.apple.com/access/integrations/api";
const APPS_URL: &str = "https://appstoreconnect.apple.com/apps";
const TESTFLIGHT_GROUP: &str = "Cheetah";

/// What setup remembers in `~/.cheetah/apple.json` (the key itself is copied
/// next to it).
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    key_id: Option<String>,
    issuer_id: Option<String>,
    team_id: Option<String>,
    key_file: Option<PathBuf>,
}

fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn saved_path() -> PathBuf {
    home().join(".cheetah").join("apple.json")
}

impl Saved {
    fn load() -> Self {
        std::fs::read_to_string(saved_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn store(&self) -> Result<(), String> {
        let path = saved_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

fn open_browser(url: &str) {
    let _ = if cfg!(windows) {
        Command::new("cmd").args(["/c", "start", "", url]).status()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else {
        Command::new("xdg-open").arg(url).status()
    };
}

/// The newest `AuthKey_*.p8` in Downloads, optionally only ones newer than
/// `since`.
fn newest_key(since: Option<SystemTime>) -> Option<PathBuf> {
    let entries = std::fs::read_dir(home().join("Downloads")).ok()?;
    entries
        .flatten()
        .filter(|entry| key_id_from_file_name(&entry.file_name().to_string_lossy()).is_some())
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .filter(|(modified, _)| since.is_none_or(|since| *modified >= since))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// Apple app ID names: letters, digits, spaces.
fn app_id_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        "Cheetah App".into()
    } else {
        cleaned
    }
}

pub(crate) fn ios(args: &Args, out: &Out) -> ExitCode {
    let known = [
        "--app",
        "--key",
        "--issuer",
        "--team",
        "--release",
        "--no-secrets",
        "--no-wait",
        "--build",
    ];
    if let Some(flag) = args.unknown_flags(&known) {
        return out.fail("setup", &format!("unknown option {flag}"));
    }
    match run(args, out) {
        Ok(summary) => {
            out.data(summary);
            if args.flag("--build") {
                let root = repo_root();
                let app = args.value("--app").unwrap_or(crate::DEFAULT_APP);
                return crate::build_ios(&root, app, !args.flag("--release"), out);
            }
            ExitCode::SUCCESS
        }
        Err(error) => out.fail("setup", &error),
    }
}

fn run(args: &Args, out: &Out) -> Result<Value, String> {
    let root = repo_root();
    let app = args.value("--app").unwrap_or(crate::DEFAULT_APP);
    let config = AppConfig::load(&root, app)?;
    let wait = !args.flag("--no-wait");
    let mut saved = Saved::load();

    // 1. The API key.
    let key_path = match args
        .value("--key")
        .map(PathBuf::from)
        .or_else(|| saved.key_file.clone())
    {
        Some(path) => path,
        None => match newest_key(None) {
            Some(path) => path,
            None if wait => wait_for_key(out)?,
            None => {
                return Err(format!(
                    "no AuthKey_*.p8 in Downloads; create one at {KEYS_URL}"
                ))
            }
        },
    };
    let file_name = key_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key_id = key_id_from_file_name(&file_name)
        .or_else(|| saved.key_id.clone())
        .ok_or_else(|| format!("{file_name} isn't named AuthKey_<key id>.p8"))?;
    let issuer_id = args
        .value("--issuer")
        .map(str::to_string)
        .or_else(|| saved.issuer_id.clone())
        .ok_or_else(|| {
            format!(
                "pass --issuer <issuer id>: it's the UUID above the key list at {KEYS_URL} \
                 (only needed the first time)"
            )
        })?;
    let pem =
        std::fs::read_to_string(&key_path).map_err(|e| format!("{}: {e}", key_path.display()))?;
    let client = Client::new(ApiKey::new(&key_id, &issuer_id, &pem)?);
    client.get("/v1/apps?limit=1")?;
    out.say(format!("ok  API key {key_id} works"));

    // Keep a copy: Downloads gets cleaned up.
    let kept = saved_path().with_file_name(format!("AuthKey_{key_id}.p8"));
    if kept != key_path {
        if let Some(parent) = kept.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::copy(&key_path, &kept).map_err(|e| format!("{}: {e}", kept.display()))?;
    }
    saved.key_id = Some(key_id.clone());
    saved.issuer_id = Some(issuer_id.clone());
    saved.key_file = Some(kept.clone());
    if let Some(team) = args.value("--team") {
        saved.team_id = Some(team.to_string());
    }
    saved.store()?;

    // 2. App IDs (bundle identifiers) with the capabilities cheetah.toml asks for.
    let dev_id = format!("{}.dev", config.ios_bundle_id);
    let identifiers = [
        (config.ios_bundle_id.clone(), app_id_name(&config.name)),
        (dev_id.clone(), app_id_name(&format!("{} Dev", config.name))),
    ];
    for (identifier, name) in &identifiers {
        let (id, seed, created) = ensure_bundle_id(&client, identifier, name)?;
        out.say(format!(
            "ok  app ID {identifier}{}",
            if created { " (registered)" } else { "" }
        ));
        if saved.team_id.is_none() && !seed.is_empty() {
            saved.team_id = Some(seed);
        }
        for capability in ensure_capabilities(&client, &id, &config)? {
            out.say(format!("ok  {identifier}: enabled {capability}"));
        }
    }
    let team_id = saved
        .team_id
        .clone()
        .ok_or("pass --team <team id> (developer.apple.com > Account > Membership)")?;
    saved.store()?;

    // 3. App Store Connect apps (the dev app always; the release app with --release).
    let mut wanted = vec![(dev_id.clone(), format!("{} Dev", config.name))];
    if args.flag("--release") {
        wanted.push((config.ios_bundle_id.clone(), config.name.clone()));
    }
    let mut apps = Vec::new();
    for (bundle, name) in &wanted {
        let id = match find_app(&client, bundle)? {
            Some(id) => id,
            None if wait => wait_for_app(&client, out, bundle, name)?,
            None => {
                return Err(format!(
                    "no App Store Connect app for {bundle}; create it at {APPS_URL}"
                ))
            }
        };
        out.say(format!("ok  App Store Connect app for {bundle}"));
        // 4. A TestFlight group that gets every build, with the team in it.
        match ensure_testers(&client, &id) {
            Ok(added) => out.say(format!(
                "ok  TestFlight group \"{TESTFLIGHT_GROUP}\"{}",
                if added > 0 {
                    format!(" ({added} testers added)")
                } else {
                    String::new()
                }
            )),
            Err(error) => out.say(format!(
                "--  TestFlight testers: {error}; add yourself under the app's TestFlight tab"
            )),
        }
        apps.push(json!({"bundle": bundle, "id": id}));
    }

    // 5. GitHub secrets for the TestFlight workflow.
    let mut secrets = Vec::new();
    if !args.flag("--no-secrets") {
        let values = [
            ("APPLE_TEAM_ID", team_id.clone()),
            ("APP_STORE_CONNECT_KEY_ID", key_id.clone()),
            ("APP_STORE_CONNECT_ISSUER_ID", issuer_id.clone()),
            ("APP_STORE_CONNECT_PRIVATE_KEY", pem.clone()),
        ];
        for (name, value) in values {
            set_secret(&root, name, &value)?;
            secrets.push(name);
        }
        out.say("ok  GitHub secrets for the TestFlight workflow");
    }

    out.say(format!(
        "\nReady. Next: cheetah build ios --dev  (installs as \"{} Dev\" through TestFlight)",
        config.name
    ));
    Ok(json!({
        "command": "setup", "ok": true, "platform": "ios", "app": app,
        "key_id": key_id, "team_id": team_id, "apps": apps, "secrets": secrets,
    }))
}

fn wait_for_key(out: &Out) -> Result<PathBuf, String> {
    let started = SystemTime::now();
    out.say(format!(
        "Opening {KEYS_URL}\n  Create a key (\"+\", any name, role Admin: the build signs with Apple's cloud certificates) and download it.\n  \
         Waiting for AuthKey_*.p8 in Downloads..."
    ));
    open_browser(KEYS_URL);
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    while Instant::now() < deadline {
        if let Some(path) = newest_key(Some(started)) {
            return Ok(path);
        }
        std::thread::sleep(Duration::from_secs(3));
    }
    Err("no key downloaded within 15 minutes; rerun with --key <path>".into())
}

/// `(resource id, seed id = team id, created)`.
fn ensure_bundle_id(
    client: &Client,
    identifier: &str,
    name: &str,
) -> Result<(String, String, bool), String> {
    let found = client.get(&format!(
        "/v1/bundleIds?filter[identifier]={}&limit=200",
        query(identifier)
    ))?;
    let existing = found["data"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["attributes"]["identifier"].as_str() == Some(identifier));
    let (item, created) = match existing {
        Some(item) => (item.clone(), false),
        None => {
            let body = json!({"data": {"type": "bundleIds", "attributes": {
                "identifier": identifier, "name": name, "platform": "IOS",
            }}});
            (client.post("/v1/bundleIds", &body)?["data"].clone(), true)
        }
    };
    Ok((
        item["id"].as_str().unwrap_or_default().to_string(),
        item["attributes"]["seedId"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        created,
    ))
}

/// Enable what cheetah.toml needs; returns the newly enabled ones.
fn ensure_capabilities(
    client: &Client,
    bundle: &str,
    config: &AppConfig,
) -> Result<Vec<&'static str>, String> {
    let mut wanted: Vec<(&'static str, Value)> = Vec::new();
    if config.capabilities.push {
        wanted.push(("PUSH_NOTIFICATIONS", Value::Null));
    }
    if config.capabilities.apple_sign_in {
        wanted.push((
            "APPLE_ID_AUTH",
            json!([{"key": "APPLE_ID_AUTH_APP_CONSENT", "options": [{"key": "PRIMARY_APP_CONSENT"}]}]),
        ));
    }
    if !config.links.is_empty() {
        wanted.push(("ASSOCIATED_DOMAINS", Value::Null));
    }
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let existing = client.get(&format!("/v1/bundleIds/{bundle}/bundleIdCapabilities"))?;
    let have: Vec<&str> = existing["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["attributes"]["capabilityType"].as_str())
        .collect();
    let mut enabled = Vec::new();
    for (capability, settings) in wanted {
        if have.contains(&capability) {
            continue;
        }
        let mut attributes = json!({"capabilityType": capability});
        if !settings.is_null() {
            attributes["settings"] = settings;
        }
        client.post(
            "/v1/bundleIdCapabilities",
            &json!({"data": {
                "type": "bundleIdCapabilities",
                "attributes": attributes,
                "relationships": {"bundleId": {"data": {"type": "bundleIds", "id": bundle}}},
            }}),
        )?;
        enabled.push(capability);
    }
    Ok(enabled)
}

fn find_app(client: &Client, bundle: &str) -> Result<Option<String>, String> {
    let found = client.get(&format!("/v1/apps?filter[bundleId]={}", query(bundle)))?;
    Ok(found["data"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|app| app["attributes"]["bundleId"].as_str() == Some(bundle))
        .and_then(|app| app["id"].as_str())
        .map(str::to_string))
}

/// Apple's API can't create apps: open the page, show what to enter, and
/// wait for the app to appear.
fn wait_for_app(client: &Client, out: &Out, bundle: &str, name: &str) -> Result<String, String> {
    out.say(format!(
        "\nCreate the app in App Store Connect (Apple's API can't do this step):\n  \
         \"+\" > New App > iOS\n  Name:      {name}   (must be unique on the App Store; add a word if taken)\n  \
         Language:  any\n  Bundle ID: {bundle}\n  SKU:       {sku}\n  \
         Waiting for it to appear...",
        sku = bundle.replace('.', "-"),
    ));
    out.data(json!({"command": "setup", "event": "create_app", "bundle": bundle, "name": name, "url": APPS_URL}));
    open_browser(APPS_URL);
    let deadline = Instant::now() + Duration::from_secs(20 * 60);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(10));
        if let Some(id) = find_app(client, bundle)? {
            return Ok(id);
        }
    }
    Err(format!(
        "the app for {bundle} didn't appear within 20 minutes; rerun when it exists"
    ))
}

/// An internal TestFlight group with access to every build, holding the
/// team's App Store Connect users. Returns how many testers were added.
fn ensure_testers(client: &Client, app: &str) -> Result<usize, String> {
    let groups = client.get(&format!("/v1/apps/{app}/betaGroups?limit=200"))?;
    let existing = groups["data"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|group| {
            group["attributes"]["name"].as_str() == Some(TESTFLIGHT_GROUP)
                && group["attributes"]["isInternalGroup"].as_bool() == Some(true)
        });
    let group = match existing {
        Some(group) => group["id"].as_str().unwrap_or_default().to_string(),
        None => {
            let created = client.post(
                "/v1/betaGroups",
                &json!({"data": {
                    "type": "betaGroups",
                    "attributes": {"name": TESTFLIGHT_GROUP, "isInternalGroup": true, "hasAccessToAllBuilds": true},
                    "relationships": {"app": {"data": {"type": "apps", "id": app}}},
                }}),
            )?;
            created["data"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        }
    };
    let members = client.get(&format!("/v1/betaGroups/{group}/betaTesters?limit=200"))?;
    let emails: Vec<String> = members["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tester| tester["attributes"]["email"].as_str())
        .map(str::to_lowercase)
        .collect();
    let users = client.get("/v1/users?limit=200")?;
    let mut added = 0;
    for user in users["data"].as_array().into_iter().flatten() {
        let attributes = &user["attributes"];
        let Some(email) = attributes["username"].as_str() else {
            continue;
        };
        if emails.contains(&email.to_lowercase()) {
            continue;
        }
        client.post(
            "/v1/betaTesters",
            &json!({"data": {
                "type": "betaTesters",
                "attributes": {
                    "email": email,
                    "firstName": attributes["firstName"],
                    "lastName": attributes["lastName"],
                },
                "relationships": {"betaGroups": {"data": [{"type": "betaGroups", "id": group}]}},
            }}),
        )?;
        added += 1;
    }
    Ok(added)
}

/// `gh secret set NAME` with the value on stdin (never on the command line).
fn set_secret(root: &Path, name: &str, value: &str) -> Result<(), String> {
    let gh = ["gh", r"C:\Program Files\GitHub CLI\gh.exe"]
        .into_iter()
        .find(|gh| output_in(root, gh, &["--version"]).is_some())
        .ok_or(
            "the GitHub CLI isn't installed (needed to store the secrets); or pass --no-secrets",
        )?;
    let mut child = Command::new(gh)
        .current_dir(root)
        .args(["secret", "set", name])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run gh: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(value.as_bytes())
            .map_err(|error| format!("gh secret set {name}: {error}"))?;
    }
    let result = child
        .wait_with_output()
        .map_err(|error| format!("gh secret set {name}: {error}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "gh secret set {name} failed: {} (run gh auth login?)",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_names_are_letters_digits_and_spaces() {
        assert_eq!(app_id_name("Nightshade Dev"), "Nightshade Dev");
        assert_eq!(app_id_name("Night & Day: Dev"), "Night Day Dev");
        assert_eq!(app_id_name("!!"), "Cheetah App");
    }
}
