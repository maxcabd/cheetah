//! `cheetah publish` output is exactly what the runtime's over-the-air
//! updates accept for this app's build.

use runtime::updates::Bundle;
use std::path::Path;

#[test]
fn published_bundle_is_accepted_by_the_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::env::temp_dir().join(format!("nightshade-publish-{}", std::process::id()));
    let published = cheetah_cli::publish::publish(&root, "nightshade", None, Some(&out)).unwrap();
    let text = std::fs::read_to_string(&published.path).unwrap();
    let bundle = Bundle::validate(&text, &published.runtime).expect("runtime accepts the bundle");
    assert_eq!(bundle.version, published.version);
    assert!(
        Bundle::validate(&text, "0.0.0+0").is_err(),
        "other builds ignore it"
    );
    let _ = std::fs::remove_dir_all(&out);
}
