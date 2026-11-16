//! Native fingerprint: a hash of everything a dev build compiles from one
//! platform's native code (Swift or Kotlin, resources, manifests, build
//! files) plus the dev server protocol version.
//!
//! Rust and `.view` changes never need a new dev build, because in
//! development they run on the dev server. When this fingerprint differs
//! between the installed dev build and the local checkout, the phone needs a
//! new dev build.

use std::path::Path;
use std::process::Command;

/// Bump when the device <-> dev server protocol changes incompatibly.
pub const PROTOCOL: u32 = 1;

pub const PLATFORMS: &[&str] = &["ios", "android"];

/// Files that don't end up in the app: docs, tests, build output.
fn excluded(path: &str) -> bool {
    path.ends_with(".md")
        || path.contains("/build/")
        || path.contains("/.gradle/")
        || path.ends_with("local.properties")
        || path.starts_with("ios/CheetahUITests/")
        || path.starts_with("ios/Libs/")
        || path.starts_with("ios/Cheetah.xcodeproj/")
        || path.starts_with("android/app/src/test/")
        || path.starts_with("android/app/src/androidTest/")
        || path.starts_with("android/app/src/main/jniLibs/")
}

/// The platform's native files (tracked plus new untracked, minus ignored),
/// sorted, with `/` separators.
pub fn files(root: &Path, platform: &str) -> Option<Vec<String>> {
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
        .arg(platform)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|path| !path.is_empty() && !excluded(path))
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    Some(files)
}

/// `ios-0123456789abcdef`, or `None` outside a git checkout.
pub fn native(root: &Path, platform: &str) -> Option<String> {
    let mut hash = Fnv::new();
    hash.write(format!("cheetah-native/{PROTOCOL}/{platform}").as_bytes());
    for file in files(root, platform)? {
        // Listed by git but deleted on disk: not part of the build.
        let Ok(bytes) = std::fs::read(root.join(&file)) else {
            continue;
        };
        hash.write(&[0]);
        hash.write(file.as_bytes());
        hash.write(&[0]);
        hash.write(&without_cr(&bytes));
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
        let ios = native(&root, "ios").expect("git checkout");
        assert!(ios.starts_with("ios-") && ios.len() == 20, "{ios}");
        assert_eq!(native(&root, "ios"), Some(ios.clone()), "stable");
        assert_ne!(
            native(&root, "android"),
            Some(ios.replace("ios", "android"))
        );
        assert!(files(&root, "ios")
            .unwrap()
            .iter()
            .any(|f| f == "ios/Cheetah/NativeApis.swift"));
    }
}
