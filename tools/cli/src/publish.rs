//! `cheetah publish`: bundle an app's `.view` files for over-the-air
//! updates (runtime::updates). The bundle is one JSON file; host it anywhere
//! and point the app's `Updates` at its URL.

use crate::config::AppConfig;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Bundle format (runtime::updates::FORMAT).
pub const FORMAT: u32 = 1;

pub struct Published {
    pub path: PathBuf,
    pub runtime: String,
    pub version: String,
    pub files: Vec<String>,
}

/// The runtime a build of `config` reports: `version+build`.
pub fn runtime_version(config: &AppConfig) -> String {
    format!("{}+{}", config.version, config.build)
}

/// Every `.view` file under `apps/<app>`, as (name relative to the app, source).
pub fn view_files(root: &Path, app: &str) -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "target" && name != "dist" && !name.to_string_lossy().starts_with('.') {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|ext| ext == "view") {
                out.push(path);
            }
        }
    }
    let base = root.join("apps").join(app);
    let mut paths = Vec::new();
    walk(&base, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(&path).ok()?.replace("\r\n", "\n");
            let name = path
                .strip_prefix(&base)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((name, source))
        })
        .collect()
}

/// Write the bundle to `out` (default `target/cheetah-publish/<app>/`).
pub fn publish(
    root: &Path,
    app: &str,
    runtime: Option<&str>,
    out: Option<&Path>,
) -> Result<Published, String> {
    let config = AppConfig::load(root, app)?;
    let runtime = runtime
        .map(str::to_string)
        .unwrap_or_else(|| runtime_version(&config));
    let files = view_files(root, app);
    if files.is_empty() {
        return Err(format!("no .view files under apps/{app}"));
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, source) in &files {
        for byte in name.bytes().chain([0]).chain(source.bytes()).chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let version = format!("{hash:016x}");
    let bundle = json!({
        "format": FORMAT,
        "runtime": runtime,
        "version": version,
        "files": files.iter().map(|(name, source)| json!({"name": name, "source": source})).collect::<Vec<_>>(),
    });
    let directory = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("target/cheetah-publish").join(app));
    std::fs::create_dir_all(&directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    let path = directory.join(format!("views-{runtime}.json"));
    std::fs::write(&path, bundle.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Published {
        path,
        runtime,
        version,
        files: files.into_iter().map(|(name, _)| name).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_nightshade_views() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let out = std::env::temp_dir().join(format!("cheetah-publish-{}", std::process::id()));
        let published = publish(&root, "nightshade", None, Some(&out)).unwrap();
        assert!(
            published
                .files
                .iter()
                .any(|f| f.ends_with("views/sheet.view")),
            "{:?}",
            published.files
        );
        let text = std::fs::read_to_string(&published.path).unwrap();
        let bundle: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(bundle["runtime"], published.runtime);
        assert_eq!(bundle["format"], 1);
        // Same files, same version.
        assert_eq!(
            publish(&root, "nightshade", None, Some(&out))
                .unwrap()
                .version,
            published.version
        );
        let _ = std::fs::remove_dir_all(&out);
    }
}
