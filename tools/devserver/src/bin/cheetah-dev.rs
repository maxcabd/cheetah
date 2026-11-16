//! cheetah-dev: rebuild and restart a dev server whenever code changes.
//! Connected devices reconnect and re-render on their own. This is the
//! fallback when `dx serve --hotpatch` isn't available or a change can't be
//! patched (e.g. struct fields changed).
//!
//!   cargo run -p cheetah-devserver --bin cheetah-dev                 # apps/nightshade
//!   cargo run -p cheetah-devserver --bin cheetah-dev -- -p cheetah-devserver --bin cheetah-devserver --app chat
//!
//! `-p`/`--bin` pick the server to build (default: package and binary
//! `nightshade`); remaining arguments are passed to it. Set
//! NIGHTSHADE_API_URL in the environment to bake a backend URL into the build.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, SystemTime};

const WATCH: &[&str] = &["crates/runtime/src", "tools/devserver/src", "apps"];
const POLL: Duration = Duration::from_millis(150);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn newest(dir: &Path, latest: &mut SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            newest(&path, latest);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                if modified > *latest {
                    *latest = modified;
                }
            }
        }
    }
}

fn snapshot(root: &Path) -> SystemTime {
    let mut latest = SystemTime::UNIX_EPOCH;
    for dir in WATCH {
        newest(&root.join(dir), &mut latest);
    }
    latest
}

struct Target {
    package: String,
    bin: String,
}

fn build(root: &Path, target: &Target) -> bool {
    println!("cheetah-dev: building {}...", target.bin);
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "-p", &target.package, "--bin", &target.bin])
        .current_dir(root)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn server_exe(root: &Path, bin: &str) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    target
        .join("debug")
        .join(format!("{bin}{}", std::env::consts::EXE_SUFFIX))
}

fn start(root: &Path, target: &Target, args: &[String]) -> Option<Child> {
    match Command::new(server_exe(root, &target.bin)).args(args).spawn() {
        Ok(child) => Some(child),
        Err(error) => {
            eprintln!("cheetah-dev: could not start the server: {error}");
            None
        }
    }
}

fn stop(child: &mut Option<Child>) {
    if let Some(mut running) = child.take() {
        let _ = running.kill();
        let _ = running.wait();
    }
}

fn main() {
    let root = root();
    let mut target = Target {
        package: "nightshade".into(),
        bin: "nightshade".into(),
    };
    let mut args = Vec::new();
    let mut input = std::env::args().skip(1);
    while let Some(arg) = input.next() {
        match arg.as_str() {
            "-p" | "--package" => target.package = input.next().unwrap_or(target.package),
            "--bin" => target.bin = input.next().unwrap_or(target.bin),
            _ => args.push(arg),
        }
    }
    let mut child = None;
    let mut seen = snapshot(&root);
    if build(&root, &target) {
        child = start(&root, &target, &args);
    } else {
        println!("cheetah-dev: build failed; fix the error and save to retry");
    }

    loop {
        std::thread::sleep(POLL);
        if let Some(running) = child.as_mut() {
            if let Ok(Some(status)) = running.try_wait() {
                println!("cheetah-dev: server exited ({status}); restarting on next change");
                child = None;
            }
        }
        let latest = snapshot(&root);
        if latest <= seen {
            continue;
        }
        // Let a burst of saves settle before rebuilding.
        std::thread::sleep(Duration::from_millis(100));
        seen = snapshot(&root);
        println!("cheetah-dev: change detected, reloading");
        // Windows locks a running .exe, so stop before building.
        stop(&mut child);
        if build(&root, &target) {
            child = start(&root, &target, &args);
        } else {
            println!("cheetah-dev: build failed; fix the error and save to retry");
        }
    }
}
