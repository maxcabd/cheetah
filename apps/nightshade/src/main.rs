//! Nightshade dev server: runs the app on this machine for a device in dev
//! mode. Under `dev-hot.ps1 (dx serve --hot-patch -p nightshade)`, edits to nightshade.rs are
//! patched into the running app (state kept) and the device updates at once.
//!
//!   nightshade [--demo] [--port 8787] [--lan]

// nightshade.rs is written against the runtime crate's own paths
// (`crate::app`, `crate::http`, ...) because device builds compile it there.
// These re-exports give it the same paths inside this crate.
mod app {
    pub use runtime::app::*;
}
#[cfg(test)]
mod diff {
    pub use runtime::diff::*;
}
mod element {
    pub use runtime::element::*;
}
mod http {
    pub use runtime::http::*;
}
mod native_api {
    pub use runtime::native_api::*;
}
mod resource {
    pub use runtime::resource::*;
}

#[allow(dead_code)]
mod nightshade;

use cheetah_devserver::{run, AppSource, Hot, Options};
use runtime::Runtime;

fn main() {
    let mut options = Options::from_args("usage: nightshade [--demo] [--port N] [--lan]");
    // Literal-only edits to this crate's views apply live, without a rebuild.
    options
        .live
        .push(concat!(env!("CARGO_MANIFEST_DIR"), "/src").into());
    let demo = options.rest.iter().any(|arg| arg == "--demo");
    let name = if demo { "nightshade-demo" } else { "nightshade" };
    run(
        name,
        options,
        AppSource::custom(move || Runtime::new(Hot(nightshade::NightshadeApp::new(demo)))),
    );
}
