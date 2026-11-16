//! Shared pieces of the `cheetah` CLI, also used by the dev server and CI.

pub mod config;
pub mod fingerprint;
pub mod prebuild;
pub mod publish;

use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};

/// Deep link that points a dev build at a dev server:
/// `cheetah://dev?url=ws%3A%2F%2F192.168.1.20%3A8787`. Without `url` it
/// makes the dev build forget the server and run its bundled app.
pub fn dev_link(server: &str) -> String {
    format!("cheetah://dev?url={}", percent_encode(server))
}

pub fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// This machine's address on the local network (the interface used for the
/// default route; no packets are sent).
pub fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

/// The Cheetah checkout: the nearest directory above the current one with
/// `crates/runtime`, else the one this binary was built from.
pub fn repo_root() -> PathBuf {
    let is_root = |dir: &Path| dir.join("crates/runtime/Cargo.toml").is_file();
    if let Ok(current) = std::env::current_dir() {
        if let Some(found) = current.ancestors().find(|dir| is_root(dir)) {
            return found.to_path_buf();
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_links_encode_the_server_url() {
        assert_eq!(
            dev_link("ws://192.168.1.20:8787"),
            "cheetah://dev?url=ws%3A%2F%2F192.168.1.20%3A8787"
        );
    }

    #[test]
    fn finds_the_checkout() {
        assert!(repo_root().join("tools/cli/Cargo.toml").is_file());
    }
}
