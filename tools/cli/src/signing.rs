//! Signing credentials the TestFlight workflow reuses for every build, like
//! EAS credentials: one Apple Distribution certificate (its private key kept
//! in `~/.cheetah`) and an App Store provisioning profile per app ID.
//!
//! Without them, each CI run starts with an empty keychain and Xcode's
//! automatic signing asks Apple for a new certificate, until the account hits
//! Apple's certificate limit and builds stop. With them, builds install the
//! same certificate and sign manually.

use crate::apple::{query, Client};
use base64::Engine;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the workflow needs, ready for GitHub secrets.
pub struct Credentials {
    pub certificate_id: String,
    pub certificate_created: bool,
    /// The certificate and private key, PKCS#12, base64.
    pub p12: String,
    pub password: String,
    /// `(app ID, profile base64, created)`.
    pub profiles: Vec<(String, String, bool)>,
}

const P12_NAME: &str = "ios_distribution.p12";
const KEY_NAME: &str = "ios_distribution.key";
const PROFILE_PREFIX: &str = "Cheetah CI";

/// Reuse the saved certificate while Apple still has it (unless `rotate`),
/// else create one; then make sure each app ID has an App Store profile for
/// it. `bundles` is `(identifier, bundleIds resource id)`.
pub fn ensure(
    client: &Client,
    dir: &Path,
    saved: Option<(&str, &str)>,
    bundles: &[(String, String)],
    rotate: bool,
) -> Result<Credentials, String> {
    let p12_path = dir.join(P12_NAME);
    let reusable = match saved {
        Some((id, password)) if !rotate && p12_path.is_file() => {
            let (status, _) = client.request("GET", &format!("/v1/certificates/{id}"), None)?;
            (status == 200).then(|| (id.to_string(), password.to_string()))
        }
        _ => None,
    };
    let (certificate_id, password, certificate_created) = match reusable {
        Some((id, password)) => (id, password, false),
        None => {
            let (id, password) = create_certificate(client, dir)?;
            (id, password, true)
        }
    };
    let p12 = std::fs::read(&p12_path).map_err(|e| format!("{}: {e}", p12_path.display()))?;

    let mut profiles = Vec::new();
    for (identifier, bundle) in bundles {
        let (content, created) = ensure_profile(client, identifier, bundle, &certificate_id)?;
        profiles.push((identifier.clone(), content, created));
    }

    Ok(Credentials {
        certificate_id,
        certificate_created,
        p12: base64::engine::general_purpose::STANDARD.encode(p12),
        password,
        profiles,
    })
}

fn openssl(args: &[&str], envs: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let output = Command::new("openssl")
        .args(args)
        .envs(envs.iter().copied())
        .output()
        .map_err(|error| {
            format!("cannot run openssl ({error}); install it or run from Git Bash")
        })?;
    if !output.status.success() {
        return Err(format!(
            "openssl {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A new private key and CSR here, the certificate from Apple, and both
/// packed into a password-protected PKCS#12 for the workflow's keychain.
fn create_certificate(client: &Client, dir: &Path) -> Result<(String, String), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let key = dir.join(KEY_NAME);
    let csr = dir.join("ios_distribution.csr");
    let der = dir.join("ios_distribution.cer");
    let pem = dir.join("ios_distribution.pem");
    openssl(&["genrsa", "-out", &path(&key), "2048"], &[])?;
    openssl(
        &["req", "-new", "-key", &path(&key), "-subj", "/CN=Cheetah CI", "-out", &path(&csr)],
        &[],
    )?;
    let csr_text = std::fs::read_to_string(&csr).map_err(|e| format!("{}: {e}", csr.display()))?;

    let body = json!({"data": {"type": "certificates", "attributes": {
        "certificateType": "DISTRIBUTION",
        "csrContent": csr_text,
    }}});
    let created = client.post("/v1/certificates", &body).map_err(|error| {
        format!(
            "{error}\n  Apple allows only a few distribution certificates: revoke an unused one at \
             developer.apple.com/account/resources/certificates, then run this again"
        )
    })?;
    let id = created["data"]["id"].as_str().unwrap_or_default().to_string();
    let content = created["data"]["attributes"]["certificateContent"]
        .as_str()
        .ok_or("App Store Connect returned no certificate content")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content)
        .map_err(|error| format!("certificate content: {error}"))?;
    std::fs::write(&der, bytes).map_err(|e| format!("{}: {e}", der.display()))?;
    openssl(&["x509", "-inform", "DER", "-in", &path(&der), "-out", &path(&pem)], &[])?;

    let password = String::from_utf8_lossy(&openssl(&["rand", "-hex", "16"], &[])?)
        .trim()
        .to_string();
    // SHA1/3DES: the PKCS#12 encryption macOS keychains import everywhere.
    openssl(
        &[
            "pkcs12", "-export", "-inkey", &path(&key), "-in", &path(&pem),
            "-out", &path(&dir.join(P12_NAME)), "-name", "Cheetah CI",
            "-passout", "env:CHEETAH_P12_PASSWORD",
            "-keypbe", "PBE-SHA1-3DES", "-certpbe", "PBE-SHA1-3DES", "-macalg", "sha1",
        ],
        &[("CHEETAH_P12_PASSWORD", &password)],
    )?;
    let _ = std::fs::remove_file(&csr);
    Ok((id, password))
}

/// The `Cheetah CI <app ID>` App Store profile, made for `certificate`;
/// returns its content (base64) and whether it was created.
fn ensure_profile(
    client: &Client,
    identifier: &str,
    bundle: &str,
    certificate: &str,
) -> Result<(String, bool), String> {
    let name = format!("{PROFILE_PREFIX} {identifier}");
    let found = client.get(&format!(
        "/v1/profiles?filter[name]={}&limit=50",
        query(&name)
    ))?;
    for profile in found["data"].as_array().into_iter().flatten() {
        let id = profile["id"].as_str().unwrap_or_default();
        let active = profile["attributes"]["profileState"].as_str() == Some("ACTIVE");
        let certificates = client.get(&format!("/v1/profiles/{id}/certificates?limit=50"))?;
        let has_certificate = certificates["data"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| item["id"].as_str() == Some(certificate));
        if active && has_certificate {
            if let Some(content) = profile["attributes"]["profileContent"].as_str() {
                return Ok((content.to_string(), false));
            }
        }
        // Stale (another certificate, expired): replace it.
        client.call("DELETE", &format!("/v1/profiles/{id}"), None)?;
    }

    let body = json!({"data": {
        "type": "profiles",
        "attributes": {"name": name, "profileType": "IOS_APP_STORE"},
        "relationships": {
            "bundleId": {"data": {"type": "bundleIds", "id": bundle}},
            "certificates": {"data": [{"type": "certificates", "id": certificate}]},
        },
    }});
    let created = client.post("/v1/profiles", &body)?;
    let content = created["data"]["attributes"]["profileContent"]
        .as_str()
        .ok_or("App Store Connect returned no profile content")?;
    Ok((content.to_string(), true))
}

/// Where the certificate's files live: next to the saved API key.
pub fn dir(saved_file: &Path) -> PathBuf {
    saved_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The GitHub secret holding an app ID's profile: the `.dev` ID's and the
/// release one's, which the workflow picks between by BUNDLE_ID.
pub fn profile_secret(identifier: &str) -> &'static str {
    if identifier.ends_with(".dev") {
        "IOS_PROFILE_DEV"
    } else {
        "IOS_PROFILE_RELEASE"
    }
}
