//! `cheetah prebuild`: write an app's `cheetah.toml` into the native
//! projects. Generated sections sit between `cheetah:<name>:begin` and
//! `cheetah:<name>:end` markers, so everything outside them stays
//! hand-written; whole generated files say so at the top.

use crate::config::{AppConfig, PERMISSIONS};
use std::path::{Path, PathBuf};

/// One file prebuild writes, with its new contents.
pub struct Output {
    pub path: PathBuf,
    pub contents: String,
}

/// Everything prebuild would write for `config` (nothing is written yet).
/// `dev` makes the iOS dev build: `.dev` bundle ID, "<name> Dev", and the dev
/// server connection compiled into Release.
pub fn plan(root: &Path, app: &str, config: &AppConfig, dev: bool) -> Result<Vec<Output>, String> {
    let native = root.join("apps").join(app).join("native");
    let mut outputs = Vec::new();

    // iOS: project.yml, Info.plist, entitlements, module list.
    let project = root.join("ios/project.yml");
    let text = read(&project)?;
    let text = replace_block(&text, "settings", &ios_settings(config, dev))?;
    let text = replace_block(&text, "sources", &ios_sources(&native))?;
    outputs.push(Output {
        path: project,
        contents: text,
    });

    let plist = root.join("ios/Cheetah/Info.plist");
    let text = read(&plist)?;
    let text = replace_block(&text, "permissions", &plist_permissions(config))?;
    let text = replace_block(&text, "schemes", &plist_schemes(config))?;
    outputs.push(Output {
        path: plist,
        contents: text,
    });

    outputs.push(Output {
        path: root.join("ios/Cheetah/Assets.xcassets/LaunchBackground.colorset/Contents.json"),
        contents: launch_color(config)?,
    });
    outputs.push(Output {
        path: root.join("ios/Cheetah/Generated/Cheetah.entitlements"),
        contents: entitlements(config),
    });
    outputs.push(Output {
        path: root.join("ios/Cheetah/Generated/GeneratedModules.swift"),
        contents: swift_modules(config),
    });

    // Android: gradle properties, manifest, module list.
    outputs.push(Output {
        path: root.join("android/app/cheetah.properties"),
        contents: android_properties(config, &native),
    });
    let manifest = root.join("android/app/src/main/AndroidManifest.xml");
    let text = read(&manifest)?;
    let text = replace_block(&text, "permissions", &android_permissions(config))?;
    let text = replace_block(&text, "links", &android_links(config))?;
    outputs.push(Output {
        path: manifest,
        contents: text,
    });
    outputs.push(Output {
        path: root.join("android/app/src/main/java/com/example/rustui/GeneratedModules.kt"),
        contents: kotlin_modules(config),
    });
    Ok(outputs)
}

/// Write the planned files; returns the ones whose contents changed.
pub fn write(outputs: &[Output]) -> Result<Vec<PathBuf>, String> {
    let mut changed = Vec::new();
    for output in outputs {
        let current = std::fs::read(&output.path).ok();
        // Keep each file's line endings (Windows checkouts use CRLF).
        let crlf = current
            .as_deref()
            .is_some_and(|bytes| bytes.windows(2).any(|w| w == b"\r\n"));
        let contents = if crlf {
            output.contents.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            output.contents.clone()
        };
        if current.as_deref() == Some(contents.as_bytes()) {
            continue;
        }
        if let Some(parent) = output.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&output.path, contents)
            .map_err(|e| format!("{}: {e}", output.path.display()))?;
        changed.push(output.path.clone());
    }
    Ok(changed)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map(|text| text.replace("\r\n", "\n"))
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// Replace the lines between the `cheetah:<name>:begin` and `:end` marker
/// lines with `body`, indented like the begin marker.
pub fn replace_block(text: &str, name: &str, body: &[String]) -> Result<String, String> {
    let begin = format!("cheetah:{name}:begin");
    let end = format!("cheetah:{name}:end");
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.contains(&begin))
        .ok_or_else(|| format!("missing `{begin}` marker"))?;
    let stop = lines[start..]
        .iter()
        .position(|line| line.contains(&end))
        .map(|offset| start + offset)
        .ok_or_else(|| format!("missing `{end}` marker"))?;
    let indent: String = lines[start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let mut out: Vec<String> = lines[..=start].iter().map(|l| l.to_string()).collect();
    out.extend(body.iter().map(|line| format!("{indent}{line}")));
    out.extend(lines[stop..].iter().map(|l| l.to_string()));
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    Ok(joined)
}

fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_default()
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn ios_settings(config: &AppConfig, dev: bool) -> Vec<String> {
    let (suffix, name_suffix) = if dev { (".dev", " Dev") } else { ("", "") };
    let mut lines = vec![
        format!(
            "PRODUCT_BUNDLE_IDENTIFIER: {}{suffix}",
            config.ios_bundle_id
        ),
        format!("MARKETING_VERSION: {}", quoted(&config.version)),
        format!("CURRENT_PROJECT_VERSION: \"{}\"", config.build),
        format!("CHEETAH_DEFAULT_APP: {}", config.runtime_app()),
        format!(
            "CHEETAH_DISPLAY_NAME: {}",
            quoted(&format!("{}{name_suffix}", config.name))
        ),
        "CODE_SIGN_ENTITLEMENTS: Cheetah/Generated/Cheetah.entitlements".into(),
    ];
    if dev {
        lines.push(
            "SWIFT_ACTIVE_COMPILATION_CONDITIONS: \"$(inherited) CHEETAH_DEV_CLIENT\"".into(),
        );
    }
    lines
}

fn ios_sources(native: &Path) -> Vec<String> {
    if native.join("ios").is_dir() {
        let relative = relative_from(Path::new("ios"), native, "ios");
        vec![format!("- path: {relative}")]
    } else {
        Vec::new()
    }
}

/// `apps/<app>/native/<platform>` as seen from the `from` directory.
fn relative_from(from: &Path, native: &Path, platform: &str) -> String {
    let app = native
        .parent()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ups = "../".repeat(from.components().count());
    format!("{ups}apps/{app}/native/{platform}")
}

fn plist_permissions(config: &AppConfig) -> Vec<String> {
    let mut lines = Vec::new();
    for permission in PERMISSIONS {
        if let Some(reason) = config.permissions.get(permission.key) {
            for key in permission.ios {
                lines.push(format!("<key>{key}</key>"));
                lines.push(format!("<string>{}</string>", xml(reason)));
            }
        }
    }
    if config.capabilities.push || config.permissions.contains_key("notifications") {
        lines.push("<key>UIBackgroundModes</key>".into());
        lines.push("<array>".into());
        lines.push("    <string>remote-notification</string>".into());
        lines.push("</array>".into());
    }
    lines
}

fn plist_schemes(config: &AppConfig) -> Vec<String> {
    let Some(scheme) = &config.scheme else {
        return Vec::new();
    };
    vec![
        "<dict>".into(),
        "    <key>CFBundleURLName</key>".into(),
        "    <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>".into(),
        "    <key>CFBundleURLSchemes</key>".into(),
        "    <array>".into(),
        format!("        <string>{}</string>", xml(scheme)),
        "    </array>".into(),
        "</dict>".into(),
    ]
}

/// The launch screen's color (Info.plist's UILaunchScreen names it).
fn launch_color(config: &AppConfig) -> Result<String, String> {
    let hex = config.launch_background.as_deref().unwrap_or("#000000");
    let digits = hex.trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("launch_background {hex:?} isn't #rrggbb"));
    }
    let channel = |i: usize| format!("0x{}", digits[i..i + 2].to_ascii_uppercase());
    Ok(format!(
        "{{\n  \"colors\" : [\n    {{\n      \"color\" : {{\n        \"color-space\" : \"srgb\",\n        \"components\" : {{\n          \"alpha\" : \"1.000\",\n          \"blue\" : \"{}\",\n          \"green\" : \"{}\",\n          \"red\" : \"{}\"\n        }}\n      }},\n      \"idiom\" : \"universal\"\n    }}\n  ],\n  \"info\" : {{\n    \"author\" : \"cheetah prebuild\",\n    \"version\" : 1\n  }}\n}}\n",
        channel(4),
        channel(2),
        channel(0),
    ))
}

fn entitlements(config: &AppConfig) -> String {
    let mut body = String::new();
    if !config.links.is_empty() {
        body.push_str("    <key>com.apple.developer.associated-domains</key>\n    <array>\n");
        for domain in &config.links {
            body.push_str(&format!(
                "        <string>applinks:{}</string>\n",
                xml(domain)
            ));
        }
        body.push_str("    </array>\n");
    }
    if config.capabilities.push {
        body.push_str("    <key>aps-environment</key>\n    <string>development</string>\n");
    }
    if config.capabilities.apple_sign_in {
        body.push_str(
            "    <key>com.apple.developer.applesignin</key>\n    <array>\n        <string>Default</string>\n    </array>\n",
        );
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!-- Generated by `cheetah prebuild` from the app's cheetah.toml. Do not edit. -->\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

fn swift_modules(config: &AppConfig) -> String {
    let list = |kind: &str| -> String {
        let names: Vec<String> = config
            .modules
            .iter()
            .filter(|module| module.kind == kind)
            .filter_map(|module| module.ios.as_ref().map(|class| format!("{class}.self")))
            .collect();
        names.join(", ")
    };
    format!(
        "// Generated by `cheetah prebuild` from the app's cheetah.toml. Do not edit.\n\n\
         enum GeneratedModules {{\n    \
         static let apis: [CheetahApiModule.Type] = [{}]\n    \
         static let views: [CheetahViewModule.Type] = [{}]\n}}\n",
        list("api"),
        list("view"),
    )
}

fn kotlin_class(class: &str) -> String {
    if class.contains('.') {
        class.to_string()
    } else {
        format!("com.example.rustui.{class}")
    }
}

fn kotlin_modules(config: &AppConfig) -> String {
    let mut apis = Vec::new();
    let mut views = Vec::new();
    for module in &config.modules {
        let Some(class) = &module.android else {
            continue;
        };
        let class = kotlin_class(class);
        if module.kind == "view" {
            views.push(format!(
                "        ViewModuleSpec(\"{}\") {{ context, host -> {class}(context, host) }},",
                module.name
            ));
        } else {
            apis.push(format!(
                "        ApiModuleSpec(\"{}\") {{ {class}(it) }},",
                module.name
            ));
        }
    }
    let list = |items: &[String]| {
        if items.is_empty() {
            "listOf()".to_string()
        } else {
            format!("listOf(\n{}\n    )", items.join("\n"))
        }
    };
    format!(
        "// Generated by `cheetah prebuild` from the app's cheetah.toml. Do not edit.\n\
         package com.example.rustui\n\n\
         object GeneratedModules {{\n    \
         val apis: List<ApiModuleSpec> = {}\n    \
         val views: List<ViewModuleSpec> = {}\n}}\n",
        list(&apis),
        list(&views),
    )
}

fn android_properties(config: &AppConfig, native: &Path) -> String {
    let source = if native.join("android").is_dir() {
        relative_from(Path::new("android/app"), native, "android")
    } else {
        String::new()
    };
    // `\` and non-ASCII are escaped the .properties way.
    let escape = |text: &str| -> String {
        text.chars()
            .map(|c| match c {
                '\\' => "\\\\".to_string(),
                c if c.is_ascii() => c.to_string(),
                c => format!("\\u{:04x}", c as u32),
            })
            .collect()
    };
    format!(
        "# Generated by `cheetah prebuild` from the app's cheetah.toml. Do not edit.\n\
         applicationId={}\nversionName={}\nversionCode={}\nappName={}\nscheme={}\nnativeSource={}\n",
        config.android_package,
        escape(&config.version),
        config.build,
        escape(&config.name),
        config.scheme.as_deref().unwrap_or(""),
        source,
    )
}

fn android_permissions(config: &AppConfig) -> Vec<String> {
    let mut lines = Vec::new();
    for permission in PERMISSIONS {
        let wanted = config.permissions.contains_key(permission.key)
            || (permission.key == "notifications" && config.capabilities.push);
        if wanted {
            for name in permission.android {
                lines.push(format!("<uses-permission android:name=\"{name}\" />"));
            }
        }
    }
    lines
}

fn android_links(config: &AppConfig) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(scheme) = &config.scheme {
        lines.extend([
            "<intent-filter>".to_string(),
            "    <action android:name=\"android.intent.action.VIEW\" />".into(),
            "    <category android:name=\"android.intent.category.DEFAULT\" />".into(),
            "    <category android:name=\"android.intent.category.BROWSABLE\" />".into(),
            format!("    <data android:scheme=\"{}\" />", xml(scheme)),
            "</intent-filter>".into(),
        ]);
    }
    for domain in &config.links {
        lines.extend([
            "<intent-filter android:autoVerify=\"true\">".to_string(),
            "    <action android:name=\"android.intent.action.VIEW\" />".into(),
            "    <category android:name=\"android.intent.category.DEFAULT\" />".into(),
            "    <category android:name=\"android.intent.category.BROWSABLE\" />".into(),
            format!(
                "    <data android:scheme=\"https\" android:host=\"{}\" />",
                xml(domain)
            ),
            "</intent-filter>".into(),
        ]);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AppConfig {
        AppConfig::parse(
            r#"
name = "Night & Day"
scheme = "nightshade"
links = ["nightshade.app"]
ios_bundle_id = "com.nightshade.cheetah"
android_package = "com.example.rustui"
app = "nightshade"
[permissions]
location = "Show nearby places."
camera = "Scan menus."
[capabilities]
push = true
[[modules]]
name = "weather"
ios = "WeatherModule"
android = "WeatherModule"
[[modules]]
name = "chart"
kind = "view"
ios = "ChartView"
android = "com.nightshade.ChartView"
"#,
        )
        .unwrap()
    }

    #[test]
    fn blocks_are_replaced_in_place_with_indentation() {
        let text = "a:\n  # cheetah:x:begin\n  old: 1\n  # cheetah:x:end\nb: 2\n";
        let out = replace_block(text, "x", &["new: 2".into(), "more: 3".into()]).unwrap();
        assert_eq!(
            out,
            "a:\n  # cheetah:x:begin\n  new: 2\n  more: 3\n  # cheetah:x:end\nb: 2\n"
        );
        assert_eq!(
            replace_block(&out, "x", &["new: 2".into(), "more: 3".into()]).unwrap(),
            out,
            "idempotent"
        );
        assert!(replace_block(text, "y", &[])
            .unwrap_err()
            .contains("missing"));
    }

    #[test]
    fn launch_color_is_checked() {
        let mut config = config();
        assert!(launch_color(&config).unwrap().contains("\"red\" : \"0x00\""));
        config.launch_background = Some("#0b0b0f".into());
        let color = launch_color(&config).unwrap();
        assert!(color.contains("\"red\" : \"0x0B\"") && color.contains("\"blue\" : \"0x0F\""));
        config.launch_background = Some("dark".into());
        assert!(launch_color(&config).is_err());
    }

    #[test]
    fn ios_output() {
        let config = config();
        let dev = ios_settings(&config, true).join("\n");
        assert!(dev.contains("PRODUCT_BUNDLE_IDENTIFIER: com.nightshade.cheetah.dev"));
        assert!(dev.contains("CHEETAH_DISPLAY_NAME: \"Night & Day Dev\""));
        assert!(dev.contains("CHEETAH_DEV_CLIENT"));
        assert!(!ios_settings(&config, false)
            .join("\n")
            .contains("CHEETAH_DEV_CLIENT"));
        let plist = plist_permissions(&config).join("\n");
        assert!(plist.contains("<key>NSCameraUsageDescription</key>"));
        assert!(plist.contains("remote-notification"));
        assert!(plist_schemes(&config)
            .join("\n")
            .contains("<string>nightshade</string>"));
        let entitlements = entitlements(&config);
        assert!(
            entitlements.contains("applinks:nightshade.app")
                && entitlements.contains("aps-environment")
        );
        assert!(
            swift_modules(&config).contains("apis: [CheetahApiModule.Type] = [WeatherModule.self]")
        );
        assert!(
            swift_modules(&config).contains("views: [CheetahViewModule.Type] = [ChartView.self]")
        );
    }

    #[test]
    fn android_output() {
        let config = config();
        let kotlin = kotlin_modules(&config);
        assert!(
            kotlin.contains("ApiModuleSpec(\"weather\") { com.example.rustui.WeatherModule(it) }")
        );
        assert!(kotlin.contains("ViewModuleSpec(\"chart\") { context, host -> com.nightshade.ChartView(context, host) }"));
        let permissions = android_permissions(&config).join("\n");
        assert!(permissions.contains("CAMERA") && permissions.contains("POST_NOTIFICATIONS"));
        let links = android_links(&config).join("\n");
        assert!(links.contains("android:scheme=\"nightshade\"") && links.contains("autoVerify"));
        let properties = android_properties(&config, Path::new("/nope"));
        assert!(
            properties.contains("appName=Night & Day\n") && properties.contains("nativeSource=\n")
        );
    }
}
