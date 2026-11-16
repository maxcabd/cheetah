//! Native fingerprint: a hash of everything a dev build compiles from one
//! platform's native code (Swift or Kotlin, resources, manifests, build
//! files), the app's `cheetah.toml` and its own native modules, plus the dev
//! server protocol version.
//!
//! Rust and `.view` changes never need a new dev build, because in
//! development they run on the dev server. When this fingerprint differs
//! between the installed dev build and the local checkout, the phone needs a
//! new dev build.
//!
//! What `cheetah prebuild` generates is left out (it follows from
//! `cheetah.toml`, which is in), so a checkout that never ran prebuild and the
//! CI build that did agree.

use std::path::Path;
use std::process::Command;

/// Bump when the device <-> dev server protocol changes incompatibly.
pub const PROTOCOL: u32 = 1;

pub const PLATFORMS: &[&str] = &["ios", "android"];

/// Files that don't end up in the app (docs, tests, build output) or that
/// prebuild generates.
fn excluded(path: &str) -> bool {
    path.ends_with(".md")
        || path.contains("/build/")
        || path.contains("/.gradle/")
        || path.ends_with("local.properties")
        || path.starts_with("ios/CheetahUITests/")
        || path.starts_with("ios/Libs/")
        || path.starts_with("ios/Cheetah.xcodeproj/")
        || path.starts_with("ios/Cheetah/Generated/")
        || path == "android/app/cheetah.properties"
        || path.ends_with("/GeneratedModules.kt")
        || path.starts_with("android/app/src/test/")
        || path.starts_with("android/app/src/androidTest/")
        || path.starts_with("android/app/src/main/jniLibs/")
}

fn git_files(root: &Path, paths: &[String]) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
        ])
        .args(paths)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|path| !path.is_empty() && !excluded(path))
            .map(str::to_string)
            .collect(),
    )
}

/// The files a `platform` dev build of `app` compiles from (tracked plus new
/// untracked, minus ignored), sorted, with `/` separators.
pub fn files(root: &Path, platform: &str, app: Option<&str>) -> Option<Vec<String>> {
    let mut paths = vec![platform.to_string()];
    if let Some(app) = app {
        paths.push(format!("apps/{app}/cheetah.toml"));
        paths.push(format!("apps/{app}/native/{platform}"));
    }
    let mut files = git_files(root, &paths)?;
    files.sort();
    files.dedup();
    Some(files)
}

/// `ios-0123456789abcdef`, or `None` outside a git checkout. `app` adds its
/// config and native modules (when `apps/<app>/cheetah.toml` exists).
pub fn native(root: &Path, platform: &str, app: Option<&str>) -> Option<String> {
    let app = app.filter(|app| root.join("apps").join(app).join("cheetah.toml").is_file());
    let mut hash = Fnv::new();
    hash.write(format!("cheetah-native/{PROTOCOL}/{platform}").as_bytes());
    for file in files(root, platform, app)? {
        // Listed by git but deleted on disk: not part of the build.
        let Ok(bytes) = std::fs::read(root.join(&file)) else {
            continue;
        };
        hash.write(&[0]);
        hash.write(file.as_bytes());
        hash.write(&[0]);
        hash.write(&without_generated(&without_cr(&bytes)));
    }
    Some(format!("{platform}-{:016x}", hash.0))
}

/// CRLF -> LF, so Windows and macOS checkouts hash the same.
fn without_cr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            continue;
        }
        out.push(*byte);
    }
    out
}

/// Drop the lines between `cheetah:<name>:begin` / `:end` markers (prebuild
/// output); the markers themselves stay.
fn without_generated(bytes: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    if !text.contains(":begin") {
        return bytes.to_vec();
    }
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let marker = line.contains("cheetah:");
        if marker && line.contains(":end") {
            inside = false;
        }
        if !inside || marker {
            out.push_str(line);
        }
        if marker && line.contains(":begin") {
            inside = true;
        }
    }
    out.into_bytes()
}

/// FNV-1a, 64-bit: stable across machines and Rust versions.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_do_not_change_the_hash() {
        assert_eq!(without_cr(b"a\r\nb\r\n"), b"a\nb\n");
        assert_eq!(without_cr(b"lone\rcr"), b"lone\rcr");
    }

    #[test]
    fn prebuild_output_is_ignored() {
        let a = "x\n# cheetah:settings:begin\nID: one\n# cheetah:settings:end\ny\n";
        let b = "x\n# cheetah:settings:begin\nID: two\nMORE: 1\n# cheetah:settings:end\ny\n";
        assert_eq!(
            without_generated(a.as_bytes()),
            without_generated(b.as_bytes())
        );
        assert_ne!(
            without_generated(a.as_bytes()),
            without_generated(b"x\nz\n")
        );
        assert!(excluded("ios/Cheetah/Generated/GeneratedModules.swift"));
        assert!(excluded("android/app/cheetah.properties"));
    }

    #[test]
    fn docs_tests_and_build_output_are_ignored() {
        assert!(excluded("ios/README.md"));
        assert!(excluded("android/app/build/outputs/x.apk"));
        assert!(excluded("ios/CheetahUITests/CheetahUITests.swift"));
        assert!(!excluded("ios/Cheetah/NativeApis.swift"));
        assert!(!excluded("android/app/src/main/AndroidManifest.xml"));
    }

    #[test]
    fn fingerprints_this_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ios = native(&root, "ios", None).expect("git checkout");
        assert!(ios.starts_with("ios-") && ios.len() == 20, "{ios}");
        assert_eq!(native(&root, "ios", None), Some(ios.clone()), "stable");
        assert_ne!(
            native(&root, "ios", Some("nightshade")),
            Some(ios.clone()),
            "app config counts"
        );
        assert_eq!(
            native(&root, "ios", Some("no-such-app")),
            Some(ios),
            "unknown apps add nothing"
        );
        let files = files(&root, "ios", Some("nightshade")).unwrap();
        assert!(files.iter().any(|f| f == "ios/Cheetah/NativeApis.swift"));
        assert!(files.iter().any(|f| f == "apps/nightshade/cheetah.toml"));
    }
}
