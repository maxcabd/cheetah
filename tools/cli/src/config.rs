//! App config: `apps/<app>/cheetah.toml`, the one place an app declares its
//! identity, permissions, links and native modules. `cheetah prebuild`
//! writes it into the iOS and Android projects.
//!
//! ```toml
//! name = "Nightshade"                  # name under the icon
//! version = "1.0.0"
//! build = 1
//! scheme = "nightshade"                # nightshade://… deep links
//! launch_background = "#0b0b0f"        # behind the launch, before the app draws
//! links = ["nightshade.app"]           # https universal / app links
//! ios_bundle_id = "com.nightshade.cheetah"
//! android_package = "com.example.rustui"
//!
//! [permissions]                        # iOS shows these strings
//! location = "Show nearby restaurants on the map."
//! camera = "Scan menus."
//! notifications = "Tell you when a table opens up."
//!
//! [capabilities]
//! push = true
//! apple_sign_in = true
//!
//! [[modules]]                          # the app's own native code in
//! name = "weather"                     # apps/<app>/native/{ios,android}
//! kind = "api"                         # or "view" (native("weather"))
//! ios = "WeatherModule"
//! android = "WeatherModule"
//! ```

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_build")]
    pub build: u32,
    /// The runtime app opened at launch (defaults to the folder name).
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub scheme: Option<String>,
    /// "#rrggbb" shown while the app launches (default black).
    #[serde(default)]
    pub launch_background: Option<String>,
    #[serde(default)]
    pub links: Vec<String>,
    pub ios_bundle_id: String,
    pub android_package: String,
    #[serde(default)]
    pub permissions: BTreeMap<String, String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub modules: Vec<ModuleConfig>,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub apple_sign_in: bool,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    pub name: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub ios: Option<String>,
    pub android: Option<String>,
}

fn default_version() -> String {
    "1.0.0".into()
}

fn default_build() -> u32 {
    1
}

fn default_kind() -> String {
    "api".into()
}

/// Permission keys and what each turns into on each platform.
pub struct Permission {
    pub key: &'static str,
    pub ios: &'static [&'static str],
    pub android: &'static [&'static str],
}

pub const PERMISSIONS: &[Permission] = &[
    Permission {
        key: "location",
        ios: &["NSLocationWhenInUseUsageDescription"],
        android: &[
            "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.ACCESS_COARSE_LOCATION",
        ],
    },
    Permission {
        key: "location_always",
        ios: &["NSLocationAlwaysAndWhenInUseUsageDescription"],
        android: &["android.permission.ACCESS_BACKGROUND_LOCATION"],
    },
    Permission {
        key: "camera",
        ios: &["NSCameraUsageDescription"],
        android: &["android.permission.CAMERA"],
    },
    Permission {
        key: "photos",
        ios: &[
            "NSPhotoLibraryUsageDescription",
            "NSPhotoLibraryAddUsageDescription",
        ],
        android: &["android.permission.READ_MEDIA_IMAGES"],
    },
    Permission {
        key: "microphone",
        ios: &["NSMicrophoneUsageDescription"],
        android: &["android.permission.RECORD_AUDIO"],
    },
    Permission {
        key: "contacts",
        ios: &["NSContactsUsageDescription"],
        android: &["android.permission.READ_CONTACTS"],
    },
    Permission {
        key: "calendar",
        ios: &[
            "NSCalendarsFullAccessUsageDescription",
            "NSCalendarsUsageDescription",
        ],
        android: &[
            "android.permission.READ_CALENDAR",
            "android.permission.WRITE_CALENDAR",
        ],
    },
    Permission {
        key: "face_id",
        ios: &["NSFaceIDUsageDescription"],
        android: &["android.permission.USE_BIOMETRIC"],
    },
    Permission {
        key: "notifications",
        ios: &[],
        android: &["android.permission.POST_NOTIFICATIONS"],
    },
];

impl AppConfig {
    pub fn path(root: &Path, app: &str) -> PathBuf {
        root.join("apps").join(app).join("cheetah.toml")
    }

    pub fn load(root: &Path, app: &str) -> Result<Self, String> {
        let path = Self::path(root, app);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let mut config =
            Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        if config.app.is_none() {
            config.app = Some(app.to_string());
        }
        Ok(config)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let config: AppConfig = toml::from_str(text).map_err(|error| error.to_string())?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        let identifier = |value: &str| {
            value.split('.').count() >= 2
                && value.split('.').all(|part| {
                    !part.is_empty()
                        && part
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                })
        };
        if !identifier(&self.ios_bundle_id) {
            return Err(format!(
                "ios_bundle_id `{}` is not like com.company.app",
                self.ios_bundle_id
            ));
        }
        if !identifier(&self.android_package) || self.android_package.contains('-') {
            return Err(format!(
                "android_package `{}` is not like com.company.app",
                self.android_package
            ));
        }
        if let Some(scheme) = &self.scheme {
            let valid = scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c));
            if !valid || scheme == "cheetah" {
                return Err(format!("scheme `{scheme}` must be lowercase letters, digits, + - . (and not `cheetah`)"));
            }
        }
        for key in self.permissions.keys() {
            if !PERMISSIONS.iter().any(|permission| permission.key == key) {
                let known: Vec<&str> = PERMISSIONS.iter().map(|p| p.key).collect();
                return Err(format!(
                    "unknown permission `{key}` (known: {})",
                    known.join(", ")
                ));
            }
        }
        for module in &self.modules {
            if module.kind != "api" && module.kind != "view" {
                return Err(format!(
                    "module `{}`: kind must be \"api\" or \"view\"",
                    module.name
                ));
            }
            if module.ios.is_none() && module.android.is_none() {
                return Err(format!(
                    "module `{}` names no ios or android class",
                    module.name
                ));
            }
        }
        Ok(())
    }

    pub fn runtime_app(&self) -> &str {
        self.app.as_deref().unwrap_or("chat")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name = "Nightshade"
scheme = "nightshade"
links = ["nightshade.app"]
ios_bundle_id = "com.nightshade.cheetah"
android_package = "com.example.rustui"

[permissions]
location = "Show nearby restaurants."

[capabilities]
push = true

[[modules]]
name = "weather"
ios = "WeatherModule"
"#;

    #[test]
    fn parses_with_defaults() {
        let config = AppConfig::parse(SAMPLE).unwrap();
        assert_eq!(config.version, "1.0.0");
        assert_eq!(config.build, 1);
        assert!(config.capabilities.push && !config.capabilities.apple_sign_in);
        assert_eq!(config.modules[0].kind, "api");
    }

    #[test]
    fn rejects_mistakes_with_a_reason() {
        let err = |text: &str| AppConfig::parse(text).unwrap_err();
        assert!(err(&SAMPLE.replace("location =", "locaton =")).contains("unknown permission"));
        assert!(
            err(&SAMPLE.replace("com.nightshade.cheetah", "nightshade")).contains("ios_bundle_id")
        );
        assert!(
            err(&SAMPLE.replace("scheme = \"nightshade\"", "scheme = \"Night Shade\""))
                .contains("scheme")
        );
        assert!(err(&format!("{SAMPLE}\nicon = 1\n")).contains("unknown field"));
    }
}
