//! App Store Connect API (the official one): ES256 tokens from an API key,
//! and JSON requests through `curl`.
//! https://developer.apple.com/documentation/appstoreconnectapi

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::pkcs8::DecodePrivateKey;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const BASE: &str = "https://api.appstoreconnect.apple.com";

/// An API key: `AuthKey_<key id>.p8` plus the issuer ID shown above the
/// key list in App Store Connect.
#[derive(Clone)]
pub struct ApiKey {
    pub key_id: String,
    pub issuer_id: String,
    signing: SigningKey,
}

impl ApiKey {
    pub fn new(key_id: &str, issuer_id: &str, pem: &str) -> Result<Self, String> {
        let signing = SigningKey::from_pkcs8_pem(pem.trim())
            .map_err(|error| format!("not an App Store Connect .p8 key: {error}"))?;
        Ok(Self {
            key_id: key_id.to_string(),
            issuer_id: issuer_id.to_string(),
            signing,
        })
    }

    /// A token valid for 15 minutes (Apple allows at most 20).
    pub fn token(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let header = json!({"alg": "ES256", "kid": self.key_id, "typ": "JWT"});
        let claims = json!({
            "iss": self.issuer_id,
            "iat": now,
            // Apple rejects tokens living longer than 20 minutes; stay under.
            "exp": now + 15 * 60,
            "aud": "appstoreconnect-v1",
        });
        let encode = |value: &Value| URL_SAFE_NO_PAD.encode(value.to_string());
        let message = format!("{}.{}", encode(&header), encode(&claims));
        // JOSE ES256 is the raw 64-byte r||s, which is p256's fixed encoding.
        let signature: Signature = self.signing.sign(message.as_bytes());
        format!("{message}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }
}

/// The key ID in an `AuthKey_<key id>.p8` file name.
pub fn key_id_from_file_name(name: &str) -> Option<String> {
    let id = name.strip_prefix("AuthKey_")?.strip_suffix(".p8")?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

pub struct Client {
    key: ApiKey,
}

impl Client {
    pub fn new(key: ApiKey) -> Self {
        Self { key }
    }

    /// `(status, json body)`; transport failures are errors.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        // The token goes in a header file, not on the command line.
        let header = std::env::temp_dir().join(format!("cheetah-asc-{}.txt", std::process::id()));
        std::fs::write(
            &header,
            format!("Authorization: Bearer {}\n", self.key.token()),
        )
        .map_err(|error| format!("cannot write a temp file: {error}"))?;
        let mut command = Command::new("curl");
        command
            // -g: query keys like filter[identifier] are not curl globs.
            .args(["-sS", "-g", "-X", method, "-w", "\n%{http_code}", "-H"])
            .arg(format!("@{}", header.display()))
            .args(["-H", "Content-Type: application/json"])
            // CHEETAH_ASC_BASE points at a local server, for testing.
            .arg(format!(
                "{}{path}",
                std::env::var("CHEETAH_ASC_BASE").unwrap_or_else(|_| BASE.to_string())
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if body.is_some() {
            command.args(["--data-binary", "@-"]).stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        let result = command.spawn().and_then(|mut child| {
            if let (Some(body), Some(mut stdin)) = (body, child.stdin.take()) {
                stdin.write_all(body.to_string().as_bytes())?;
            }
            child.wait_with_output()
        });
        let _ = std::fs::remove_file(&header);
        let output = result.map_err(|error| format!("cannot run curl: {error}"))?;
        let text = String::from_utf8_lossy(&output.stdout);
        let (body, status) = text.rsplit_once('\n').unwrap_or(("", text.trim()));
        let status: u16 = status.trim().parse().map_err(|_| {
            format!(
                "no response from App Store Connect: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )
        })?;
        let json = serde_json::from_str(body).unwrap_or(Value::Null);
        Ok((status, json))
    }

    /// A 2xx response's body, or Apple's error message.
    pub fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let (status, json) = self.request(method, path, body)?;
        if (200..300).contains(&status) {
            Ok(json)
        } else {
            let endpoint = path.split('?').next().unwrap_or(path);
            Err(format!("{method} {endpoint}: {}", api_error(status, &json)))
        }
    }

    pub fn get(&self, path: &str) -> Result<Value, String> {
        self.call("GET", path, None)
    }

    pub fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        self.call("POST", path, Some(body))
    }
}

/// `HTTP 409: An attribute value is not acceptable…` from Apple's error list.
pub fn api_error(status: u16, body: &Value) -> String {
    let details: Vec<String> = body["errors"]
        .as_array()
        .map(|errors| {
            errors
                .iter()
                .map(|error| {
                    let title = error["title"].as_str().unwrap_or("");
                    let detail = error["detail"].as_str().unwrap_or("");
                    if detail.is_empty() || detail == title {
                        title.to_string()
                    } else {
                        format!("{title}: {detail}")
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let agreement = details.iter().any(|detail| detail.contains("agreement"));
    let hint = match status {
        401 => " (check the key ID and issuer ID; a revoked key also gives this)",
        403 if agreement => {
            " (accept the latest agreements as the Account Holder: developer.apple.com/account, then App Store Connect under Business)"
        }
        403 => " (the key needs the Admin role: the build signs with cloud-managed certificates)",
        _ => "",
    };
    if details.is_empty() {
        format!("App Store Connect answered HTTP {status}{hint}")
    } else {
        format!(
            "App Store Connect answered HTTP {status}{hint}: {}",
            details.join("; ")
        )
    }
}

/// Percent-encode a query value.
pub fn query(value: &str) -> String {
    cheetah_cli::percent_encode(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;
    use p256::pkcs8::{EncodePrivateKey, LineEnding};

    #[test]
    fn tokens_are_es256_jwts_apple_accepts() {
        let signing = SigningKey::random(&mut rand_core::OsRng);
        let pem = signing.to_pkcs8_pem(LineEnding::LF).unwrap();
        let key = ApiKey::new("ABC123DEFG", "issuer-uuid", &pem).unwrap();
        let token = key.token();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(
            header,
            json!({"alg": "ES256", "kid": "ABC123DEFG", "typ": "JWT"})
        );
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["aud"], "appstoreconnect-v1");
        assert_eq!(claims["iss"], "issuer-uuid");
        assert_eq!(
            claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
            900
        );
        let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        assert_eq!(signature.len(), 64, "raw r||s");
        let signature = Signature::from_slice(&signature).unwrap();
        VerifyingKey::from(&signing)
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    #[test]
    fn key_ids_and_errors() {
        assert_eq!(
            key_id_from_file_name("AuthKey_2X9R4HXF34.p8").as_deref(),
            Some("2X9R4HXF34")
        );
        assert_eq!(key_id_from_file_name("AuthKey_.p8"), None);
        assert_eq!(key_id_from_file_name("notes.p8"), None);
        assert!(ApiKey::new("x", "y", "not a key").is_err());
        let body = json!({"errors": [{"title": "A conflict", "detail": "already exists"}]});
        assert_eq!(
            api_error(409, &body),
            "App Store Connect answered HTTP 409: A conflict: already exists"
        );
        assert!(api_error(401, &Value::Null).contains("issuer ID"));
    }
}
